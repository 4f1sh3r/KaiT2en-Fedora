#!/bin/bash
# prochot-monitor.sh
# André Eikmeyer Aug,2026
#
# Live-status IA32_THERM_STATUS (MSR 0x19C) 
# Shows Bit 2 (PROCHOT#/FORCEPR#  currently active) and Bit 3 (log, sticky since boot).
#
# requires msr-tools (sudo dnf install rdmsr) and modprobe msr

set -u

MSR=0x19C

if ! command -v rdmsr >/dev/null 2>&1; then
    echo "rdmsr nicht gefunden. Installiere msr-tools (z.B. dnf install msr-tools)." >&2
    exit 1
fi

if [ "$(id -u)" -ne 0 ]; then
    echo "Bitte als root ausfuehren (sudo $0)." >&2
    exit 1
fi

modprobe msr

NCPU=$(nproc)

tput civis
trap 'tput cnorm; exit 0' INT TERM

FIRST=1

while true; do
    if [ "$FIRST" -eq 1 ]; then
        clear
        FIRST=0
    else
        tput cup 0 0
    fi

    echo "Prochot/Forcepr Live Monitor    $(date '+%H:%M:%S')"
    echo "ctrl+c to quit"
    echo
    printf "%-6s %-8s %-8s %-10s %-6s %-26s\n" "Core" "MHz" "PROCHOT" "Log(sticky)" "dTj" "Status"
    echo "----------------------------------------------------------------"

    ANY_ACTIVE=0

    for i in $(seq 0 $((NCPU - 1))); do
        VAL_HEX=$(rdmsr -p "$i" "$MSR")
        if [ -z "$VAL_HEX" ]; then
            printf "%-6s %-8s %-8s %-10s %-6s %-26s\n" "$i" "?" "?" "?" "?" "read error"
            continue
        fi

        VAL_DEC=$((16#$VAL_HEX))
        BIT0=$(( VAL_DEC & 1 ))          # thermal status, currently above threshold (internal hot)
        BIT2=$(( (VAL_DEC >> 2) & 1 ))   # PROCHOT#/FORCEPR# event, currently acitve (internally OR externally)
        BIT3=$(( (VAL_DEC >> 3) & 1 ))   # PROCHOT#/FORCEPR# Log
        READOUT=$(( (VAL_DEC >> 16) & 0x7F ))   # Digital Readout: distance to TjMax in C

        MHZ=$(cat "/sys/devices/system/cpu/cpu$i/cpufreq/scaling_cur_freq")
        if [ -n "$MHZ" ]; then
            MHZ=$((MHZ / 1000))
        else
            MHZ=$(awk -v c="$i" '
                $0 ~ "^processor" { cur=$3 }
                $0 ~ "^cpu MHz" && cur==c { print int($4); exit }
            ' /proc/cpuinfo)
        fi

        STATUS="ok"
        if [ "$BIT2" -eq 1 ]; then
            if [ "$BIT0" -eq 1 ] || [ "$READOUT" -le 5 ]; then
                STATUS="ACTIVE (hot, rather internally)"
            else
                STATUS="ACTIVE (cool, rather externally)"
            fi
            ANY_ACTIVE=1
        fi

        printf "%-6s %-8s %-8s %-10s %-6s %-26s\n" "$i" "${MHZ:-?}" "$BIT2" "$BIT3" "$READOUT" "$STATUS"
    done

    echo
    if [ "$ANY_ACTIVE" -eq 1 ]; then
        echo ">>> PROCHOT/FORCEPR ACTIVE <<<"
    else
        echo "NO PROCHOT/FORCEPR."
    fi
    echo "(Log-Bit remains sticky until reboot i.e. manual clearance)"
    echo "(dTj = Degree C below TjMax. Small/0 while PROCHOT points on internal, large points on external PROCHOT)"

    sleep 1
done