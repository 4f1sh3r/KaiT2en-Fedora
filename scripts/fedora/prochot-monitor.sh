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
    echo "rdmsr not found. Install msr-tools (e.g. dnf install msr-tools)." >&2
    exit 1
fi

if [ "$(id -u)" -ne 0 ]; then
    echo "Please run as root (sudo $0)." >&2
    exit 1
fi

modprobe msr

NCPU=$(nproc)

# Prepare terminal
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
    echo "Ctrl+C to quit"
    echo
    printf "%-6s %-8s %-8s %-10s %-10s\n" "Core" "MHz" "PROCHOT" "Log(sticky)" "Status"
    echo "--------------------------------------------------"

    ANY_ACTIVE=0

    for i in $(seq 0 $((NCPU - 1))); do
        VAL_HEX=$(rdmsr -p "$i" "$MSR")
        if [ -z "$VAL_HEX" ]; then
            printf "%-6s %-8s %-8s %-10s %-10s\n" "$i" "?" "?" "?" "read error"
            continue
        fi

        VAL_DEC=$((16#$VAL_HEX))
        BIT2=$(( (VAL_DEC >> 1) & 1 ))   # PROCHOT#/FORCEPR# event, currently active
        BIT3=$(( (VAL_DEC >> 2) & 1 ))   # PROCHOT#/FORCEPR# log

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
            STATUS="ACTIVE!"
            ANY_ACTIVE=1
        fi

        printf "%-6s %-8s %-8s %-10s %-10s\n" "$i" "${MHZ:-?}" "$BIT2" "$BIT3" "$STATUS"
    done

    echo
    if [ "$ANY_ACTIVE" -eq 1 ]; then
        echo ">>> PROCHOT/FORCEPR currently ACTIVE on at least one core <<<"
    else
        echo "No core currently shows active PROCHOT/FORCEPR."
    fi
    echo "(Log bit is sticky until reboot or manual clear and says nothing about right now.)"

    sleep 1
done
