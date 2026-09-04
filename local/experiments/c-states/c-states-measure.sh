#!/usr/bin/env bash
# Read-only package C-state baseline helper. Run as root so turbostat and PMC
# debugfs counters are available. This script never changes power settings.
# Optional settle time lets the operator lock the screen before measurement.

set -euo pipefail

duration="${1:-30}"
settle="${2:-0}"
case "$duration" in
    ''|*[!0-9]*)
        echo "usage: sudo bash $0 [duration-seconds] [settle-seconds]" >&2
        exit 2
        ;;
esac
case "$settle" in
    ''|*[!0-9]*)
        echo "usage: sudo bash $0 [duration-seconds] [settle-seconds]" >&2
        exit 2
        ;;
esac
if (( duration < 5 || duration > 600 )); then
    echo "duration must be between 5 and 600 seconds" >&2
    exit 2
fi
if (( settle < 0 || settle > 120 )); then
    echo "settle time must be between 0 and 120 seconds" >&2
    exit 2
fi
if (( EUID != 0 )); then
    echo "run as root: sudo bash $0 $duration" >&2
    exit 1
fi

measure_tmp="$(mktemp -d /tmp/kait2en-cstates.XXXXXX)"
trap 'rm -rf -- "$measure_tmp"' EXIT

snapshot_interrupts() {
    awk '
        /^[[:space:]]*[0-9]+:/ {
            irq=$1; sub(/:$/, "", irq); total=0; i=2
            while (i <= NF && $i ~ /^[0-9]+$/) { total += $i; i++ }
            printf "%s %s", irq, total
            while (i <= NF) { printf " %s", $i; i++ }
            print ""
        }
    ' /proc/interrupts | sort
}

snapshot_gpes() {
    for path in /sys/firmware/acpi/interrupts/gpe* /sys/firmware/acpi/interrupts/sci; do
        [[ -r "$path" ]] || continue
        read -r count _ < "$path"
        [[ "$count" =~ ^[0-9]+$ ]] || continue
        printf '%s %s\n' "$(basename "$path")" "$count"
    done | sort
}

snapshot_pmc() {
    local path=/sys/kernel/debug/pmc_core/package_cstate_show
    [[ -r "$path" ]] || return 0
    awk -F: '{ key=$1; gsub(/[[:space:]]+/, "", key); value=$2; gsub(/[[:space:]]+/, "", value); print key, value }' "$path"
}

print_deltas() {
    local before="$1" after="$2" heading="$3"
    echo
    echo "$heading"
    awk '
        NR==FNR { before[$1]=$2; next }
        ($1 in before) && $2 >= before[$1] { print $1, $2-before[$1] }
    ' "$before" "$after" | sort -k2,2nr | head -n 30
}

print_interrupt_deltas() {
    local before="$1" after="$2"
    echo
    echo "interrupt_deltas"
    awk '
        NR==FNR { before[$1]=$2; next }
        ($1 in before) && $2 >= before[$1] {
            printf "%s %d", $1, $2-before[$1]
            for (i=3; i<=NF; i++) printf " %s", $i
            print ""
        }
    ' "$before" "$after" | sort -k2,2nr | head -n 30
}

snapshot_i915_dc() {
    local path=/sys/kernel/debug/dri/0000:00:02.0/i915_dmc_info
    [[ -r "$path" ]] || return 0
    awk '
        /DC3 -> DC5 count:/ { print "DC3_to_DC5", $5 }
        /DC5 -> DC6 count:/ { print "DC5_to_DC6", $5 }
    ' "$path"
}

snapshot_network() {
    local iface=wlp229s0 path key
    path="/sys/class/net/$iface"
    [[ -d "$path" ]] || return 0
    for key in rx_bytes rx_packets tx_bytes tx_packets; do
        [[ -r "$path/statistics/$key" ]] || continue
        printf '%s %s\n' "$key" "$(<"$path/statistics/$key")"
    done
}

sample_pmc_ltr() {
    local path=/sys/kernel/debug/pmc_core/ltr_show
    local stop line now
    [[ -r "$path" ]] || return 0
    stop=$((SECONDS + duration))
    while (( SECONDS < stop )); do
        printf -v now '%(%s)T' -1
        while IFS= read -r line; do
            case "$line" in
                *SOUTHPORT_A*|*SOUTHPORT_B*)
                    printf '%s\t%s\n' "$now" "$line"
                    ;;
            esac
        done < "$path"
        sleep 2
    done
}

echo "timestamp=$(date --iso-8601=seconds)"
echo "duration_seconds=$duration"
echo "settle_seconds=$settle"
echo "kernel=$(uname -r)"
echo "cmdline=$(</proc/cmdline)"
echo "aspm_policy=$(</sys/module/pcie_aspm/parameters/policy)"
echo "cpuidle_driver=$(</sys/devices/system/cpu/cpuidle/current_driver)"
echo "cpuidle_governor=$(</sys/devices/system/cpu/cpuidle/current_governor_ro)"
if [[ -r /sys/class/power_supply/ADP1/online ]]; then
    echo "power_source=$(</sys/class/power_supply/ADP1/online)"
else
    echo "power_source=unknown"
fi

if (( settle > 0 )); then
    echo "settling_before_measurement=true"
    sleep "$settle"
fi

snapshot_interrupts > "$measure_tmp/interrupts.before"
snapshot_gpes > "$measure_tmp/gpes.before"
snapshot_pmc > "$measure_tmp/pmc.before"
snapshot_i915_dc > "$measure_tmp/i915-dc.before"
snapshot_network > "$measure_tmp/network.before"

sample_pmc_ltr > "$measure_tmp/ltr-timeline" &
ltr_sampler_pid=$!

pidstat_pid=""
if command -v pidstat >/dev/null 2>&1; then
    LC_ALL=C pidstat -u -p ALL "$duration" 1 > "$measure_tmp/pidstat" &
    pidstat_pid=$!
fi

echo
echo "turbostat_summary"
turbostat --quiet --Summary --interval "$duration" --num_iterations 1

if [[ -n "$pidstat_pid" ]]; then
    wait "$pidstat_pid" || true
fi
kill "$ltr_sampler_pid" 2>/dev/null || true
wait "$ltr_sampler_pid" || true

snapshot_interrupts > "$measure_tmp/interrupts.after"
snapshot_gpes > "$measure_tmp/gpes.after"
snapshot_pmc > "$measure_tmp/pmc.after"
snapshot_i915_dc > "$measure_tmp/i915-dc.after"
snapshot_network > "$measure_tmp/network.after"

print_interrupt_deltas "$measure_tmp/interrupts.before" "$measure_tmp/interrupts.after"
print_deltas "$measure_tmp/gpes.before" "$measure_tmp/gpes.after" "acpi_gpe_deltas"
print_deltas "$measure_tmp/pmc.before" "$measure_tmp/pmc.after" "pmc_package_cstate_raw_deltas"
print_deltas "$measure_tmp/i915-dc.before" "$measure_tmp/i915-dc.after" "i915_display_cstate_deltas"
print_deltas "$measure_tmp/network.before" "$measure_tmp/network.after" "wlan_counter_deltas"

echo
echo "intel_pmc_ltr_timeline_counts"
if [[ -s "$measure_tmp/ltr-timeline" ]]; then
    awk '
        {
            port = ($0 ~ /SOUTHPORT_A/) ? "SOUTHPORT_A" : "SOUTHPORT_B"
            if (match($0, /Non-Snoop\(ns\):[[:space:]]*([0-9]+)/, value)) {
                counts[port, value[1]]++
            }
        }
        END {
            for (key in counts) {
                split(key, parts, SUBSEP)
                print parts[1], parts[2] "ns", counts[key] " samples"
            }
        }
    ' "$measure_tmp/ltr-timeline" | sort
else
    echo "unavailable"
fi

echo
echo "pidstat_top_average"
if [[ -s "$measure_tmp/pidstat" ]]; then
    awk '
        $1 == "Average:" && ($8 + 0) > 0 {
            printf "%s %.2f%% %s\n", $3, $8, $10
        }
    ' "$measure_tmp/pidstat" | sort -k2,2nr | head -n 30
else
    echo "pidstat unavailable"
fi

echo
echo "i915_display_features"
for diag in i915_fbc_status i915_edp_psr_status; do
    path="/sys/kernel/debug/dri/0000:00:02.0/$diag"
    printf '%s: ' "$diag"
    if [[ -e "$path" ]]; then
        head -n 1 "$path" 2>&1 || true
    else
        echo "unavailable"
    fi
done

echo
echo "intel_pmc_ltr"
if [[ -r /sys/kernel/debug/pmc_core/ltr_show ]]; then
    cat /sys/kernel/debug/pmc_core/ltr_show
else
    echo "unavailable"
fi

echo
echo "intel_pmc_pch_power_gating"
if [[ -r /sys/kernel/debug/pmc_core/pch_ip_power_gating_status ]]; then
    cat /sys/kernel/debug/pmc_core/pch_ip_power_gating_status
else
    echo "unavailable"
fi

echo
echo "cpu_dma_latency_holders"
if command -v lsof >/dev/null 2>&1; then
    lsof -nP /dev/cpu_dma_latency 2>/dev/null || true
else
    echo "lsof unavailable"
fi

echo
echo "selected_runtime_pm"
for dev in 0000:00:02.0 0000:00:0d.0 0000:00:0d.2 0000:00:0d.3 0000:00:14.0 0000:00:1c.0 0000:00:1c.4 0000:e5:00.0 0000:e6:00.0 0000:e6:00.1 0000:e6:00.3; do
    path="/sys/bus/pci/devices/$dev/power"
    [[ -d "$path" ]] || continue
    printf '%s control=%s status=%s active_ms=%s suspended_ms=%s\n' \
        "$dev" \
        "$(<"$path/control")" \
        "$(<"$path/runtime_status")" \
        "$(<"$path/runtime_active_time")" \
        "$(<"$path/runtime_suspended_time")"
done
