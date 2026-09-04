#!/usr/bin/env bash
# Calibration logger: all t2smc sensors + coretemp + fans + throttle MSRs at 1 Hz.
# Read-only. Root required (MSR access).
set -u

OUT="${1:?usage: thermal-log.sh <out.csv>}"
HW="$(dirname "$(grep -l '^t2smc$' /sys/class/hwmon/hwmon*/name)")"
CT="$(dirname "$(ls /sys/devices/platform/coretemp.0/hwmon/hwmon*/temp1_input)")"

modprobe msr 2>/dev/null

# --- static: RAPL units, TjMax, TCC offset ---
PU=$(( ( 0x$(rdmsr -0 0x606) >> 8 ) & 0x1F ))          # energy units exponent
POW_U=$(( 0x$(rdmsr -0 0x606) & 0xF ))                  # power units exponent
ENERGY_UJ_PER_TICK=$(awk -v e="$PU" 'BEGIN{printf "%.6f", 1000000/(2^e)}')
POWER_W_PER_TICK=$(awk -v e="$POW_U" 'BEGIN{printf "%.6f", 1/(2^e)}')
TJMAX=$(( ( 0x$(rdmsr -0 0x1A2) >> 16 ) & 0xFF ))
TCC=$(( ( 0x$(rdmsr -0 0x1A2) >> 24 ) & 0xF ))

# --- discover sensor keys in stable order ---
KEYS=(); PATHS=()
for l in "$HW"/temp*_label; do
    n=$(basename "$l" _label)
    KEYS+=( "$(cat "$l")" )
    PATHS+=( "$HW/${n}_input" )
done

{
  echo "# tjmax=$TJMAX tcc_offset=$TCC energy_uj_per_tick=$ENERGY_UJ_PER_TICK power_w_per_tick=$POWER_W_PER_TICK"
  echo "# bd_prochot_enabled=$(( 0x$(rdmsr -0 0x1FC) & 1 ))"
  printf 'ts,phase,pkg_c,core_max_c,pkg_dtj,fan1,fan2,fan1_tgt,fan2_tgt,pkg_w,mhz_avg,'
  printf 'pkg_therm,pkg_prochot,pkg_pl,core_prochot,core_thermal,core_rapl,core_vr,core_edp,'
  printf 'gfx_prochot,gfx_thermal,gfx_rapl,gfx_vr,pkg_thr_cnt,'
  printf '%s\n' "$(IFS=,; echo "${KEYS[*]}")"
} > "$OUT"

PHASE_FILE=/tmp/claude-thermal-phase
echo idle > "$PHASE_FILE"

prev_e=$(( 0x$(rdmsr -0 0x611) )); prev_t=$(date +%s.%N)

while :; do
    ts=$(date +%s)
    phase=$(cat "$PHASE_FILE" 2>/dev/null || echo "?")

    pkg_mc=$(cat "$CT/temp1_input"); pkg_c=$(( pkg_mc / 1000 ))
    core_max=0
    for f in "$CT"/temp[2-9]_input; do
        [ -e "$f" ] || continue
        v=$(( $(cat "$f") / 1000 )); [ "$v" -gt "$core_max" ] && core_max=$v
    done

    # package thermal status (0x1B1)
    p=$(( 0x$(rdmsr -0 0x1B1) ))
    pkg_therm=$(( p & 1 )); pkg_prochot=$(( (p >> 2) & 1 )); pkg_pl=$(( (p >> 10) & 1 ))
    pkg_dtj=$(( (p >> 16) & 0x7F ))

    # core perf limit reasons (0x64F), active bits only
    r=$(( 0x$(rdmsr -0 0x64F) ))
    c_prochot=$(( r & 1 )); c_thermal=$(( (r >> 1) & 1 ))
    c_rapl=$(( (r >> 5) & 1 )); c_vr=$(( (r >> 6) & 1 )); c_edp=$(( (r >> 8) & 1 ))

    # graphics perf limit reasons (0x6B0)
    g=$(( 0x$(rdmsr -0 0x6B0 2>/dev/null || echo 0) ))
    g_prochot=$(( g & 1 )); g_thermal=$(( (g >> 1) & 1 ))
    g_rapl=$(( (g >> 5) & 1 )); g_vr=$(( (g >> 6) & 1 ))

    # package power from energy delta
    e=$(( 0x$(rdmsr -0 0x611) )); t=$(date +%s.%N)
    de=$(( e - prev_e )); [ "$de" -lt 0 ] && de=$(( de + 4294967296 ))
    pkg_w=$(awk -v d="$de" -v u="$ENERGY_UJ_PER_TICK" -v t1="$prev_t" -v t2="$t" \
            'BEGIN{dt=t2-t1; if(dt<=0){print 0}else{printf "%.1f", (d*u/1e6)/dt}}')
    prev_e=$e; prev_t=$t

    mhz=$(awk '{s+=$1; n++} END{if(n)printf "%d", s/n/1000; else print 0}' \
          /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq 2>/dev/null || echo 0)

    thr=$(cat /sys/devices/system/cpu/cpu0/thermal_throttle/package_throttle_count)

    vals=""
    for pth in "${PATHS[@]}"; do
        v=$(cat "$pth" 2>/dev/null || echo 0)
        vals+=",$(awk -v v="$v" 'BEGIN{printf "%.2f", v/1000}')"
    done

    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s%s\n' \
        "$ts" "$phase" "$pkg_c" "$core_max" "$pkg_dtj" \
        "$(cat "$HW/fan1_input")" "$(cat "$HW/fan2_input")" \
        "$(cat "$HW/fan1_target")" "$(cat "$HW/fan2_target")" \
        "$pkg_w" "$mhz" \
        "$pkg_therm" "$pkg_prochot" "$pkg_pl" \
        "$c_prochot" "$c_thermal" "$c_rapl" "$c_vr" "$c_edp" \
        "$g_prochot" "$g_thermal" "$g_rapl" "$g_vr" \
        "$thr" "$vals" >> "$OUT"

    sleep 1
done
