#!/usr/bin/env bash
# Temporary, read-mostly A/B test for the PMC SOUTHPORT_A LTR constraint.
# The only write sets LTR ignore index 0 and the EXIT trap restores index 0.

set -euo pipefail

if (( EUID != 0 )); then
    echo "run as root" >&2
    exit 1
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
measure="$script_dir/c-states-measure.sh"
ltr_show=/sys/kernel/debug/pmc_core/ltr_show
ltr_ignore=/sys/kernel/debug/pmc_core/ltr_ignore
ltr_restore=/sys/kernel/debug/pmc_core/ltr_restore
normal_out=/tmp/cstates-ltr-normal-display-on.txt
ignore_out=/tmp/cstates-ltr-ignore-display-on.txt
controller_out=/tmp/cstates-ltr-ab-controller.txt

for path in "$measure" "$ltr_show" "$ltr_ignore" "$ltr_restore"; do
    [[ -e "$path" ]] || {
        echo "required path missing: $path" >&2
        exit 1
    }
done

spa_line() {
    awk '$1 == 0 && /SOUTHPORT_A/ { print; found=1 } END { exit !found }' \
        "$ltr_show"
}

spa_is_restored() {
    spa_line | grep -q 'LTR_IGNORE: 0'
}

restore_ltr() {
    printf '0\n' > "$ltr_restore" || true
}

trap restore_ltr EXIT INT TERM HUP

{
    echo "started=$(date --iso-8601=seconds)"
    echo "kernel=$(uname -r)"
    echo "cmdline=$(</proc/cmdline)"
    echo "wlan_operstate=$(</sys/class/net/wlp229s0/operstate)"
    if [[ -r /sys/class/power_supply/ADP1/online ]]; then
        echo "adapter_online=$(</sys/class/power_supply/ADP1/online)"
    fi
    echo "spa_before=$(spa_line)"
} > "$controller_out"

[[ "$(</sys/class/net/wlp229s0/operstate)" == up ]] || {
    echo "WLAN is not up; aborting" >> "$controller_out"
    exit 1
}

if [[ -r /sys/class/power_supply/ADP1/online ]] &&
   [[ "$(</sys/class/power_supply/ADP1/online)" != 0 ]]; then
    echo "AC adapter is online; aborting" >> "$controller_out"
    exit 1
fi

spa_is_restored || {
    echo "SOUTHPORT_A was already ignored; aborting" >> "$controller_out"
    exit 1
}

bash "$measure" 90 15 > "$normal_out"

printf '0\n' > "$ltr_ignore"
spa_line | grep -q 'LTR_IGNORE: 1' || {
    echo "failed to set SOUTHPORT_A ignore bit" >> "$controller_out"
    exit 1
}
echo "spa_ignored=$(spa_line)" >> "$controller_out"

sleep 3
bash "$measure" 90 0 > "$ignore_out"

restore_ltr
spa_is_restored || {
    echo "SOUTHPORT_A restore verification failed" >> "$controller_out"
    exit 1
}
trap - EXIT INT TERM HUP

{
    echo "spa_after=$(spa_line)"
    echo "completed=$(date --iso-8601=seconds)"
    echo "result=success"
} >> "$controller_out"
