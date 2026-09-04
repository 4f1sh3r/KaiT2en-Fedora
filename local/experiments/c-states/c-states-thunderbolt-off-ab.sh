#!/usr/bin/env bash
# Temporary A/B test for the currently initialized Ice Lake Thunderbolt stack.
# SOUTHPORT_A LTR ignore remains enabled in both windows. All changed state is
# restored and verified at normal exit; the EXIT trap attempts the same on error.

set -euo pipefail
shopt -s nullglob

if (( EUID != 0 )); then
    echo "run as root" >&2
    exit 1
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
measure="$script_dir/c-states-measure.sh"
ltr_show=/sys/kernel/debug/pmc_core/ltr_show
ltr_ignore=/sys/kernel/debug/pmc_core/ltr_ignore
ltr_restore=/sys/kernel/debug/pmc_core/ltr_restore
controller_out=/tmp/cstates-thunderbolt-off-ab-controller.txt
normal_out=/tmp/cstates-thunderbolt-on-ltr-ignore.txt
off_out=/tmp/cstates-thunderbolt-off-ltr-ignore.txt
nhis=(0000:00:0d.2 0000:00:0d.3)

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

module_loaded() {
    [[ -d "/sys/module/$1" ]]
}

nhi_driver() {
    local nhi=$1
    if [[ -L "/sys/bus/pci/devices/$nhi/driver" ]]; then
        basename -- "$(readlink -f "/sys/bus/pci/devices/$nhi/driver")"
    else
        echo none
    fi
}

nhi_state() {
    local nhi=$1
    local runtime power
    runtime=$(<"/sys/bus/pci/devices/$nhi/power/runtime_status")
    power=$(<"/sys/bus/pci/devices/$nhi/power_state")
    printf '%s driver=%s runtime=%s power=%s' \
        "$nhi" "$(nhi_driver "$nhi")" "$runtime" "$power"
}

nhis_bound() {
    local nhi
    for nhi in "${nhis[@]}"; do
        [[ "$(nhi_driver "$nhi")" == thunderbolt ]] || return 1
    done
}

nhis_unbound() {
    local nhi
    for nhi in "${nhis[@]}"; do
        [[ "$(nhi_driver "$nhi")" == none ]] || return 1
    done
}

tb_topology_is_internal_only() {
    local path name
    for path in /sys/bus/thunderbolt/devices/*; do
        name=${path##*/}
        if [[ $name =~ ^domain[0-9]+$ || $name =~ ^[0-9]+-0$ ]]; then
            continue
        fi
        echo "unexpected Thunderbolt device: $name" >> "$controller_out"
        return 1
    done
}

restore_all() {
    local status=0

    if ! modprobe t2thunderbolt; then
        echo "trap_restore_t2thunderbolt=failed" >> "$controller_out"
        status=1
    fi
    if ! modprobe thunderbolt; then
        echo "trap_restore_thunderbolt=failed" >> "$controller_out"
        status=1
    fi
    if ! printf '0\n' > "$ltr_restore"; then
        echo "trap_restore_ltr=failed" >> "$controller_out"
        status=1
    fi

    return "$status"
}

on_signal() {
    trap - INT TERM HUP
    exit 128
}

trap 'restore_all || true' EXIT
trap on_signal INT TERM HUP

{
    echo "started=$(date --iso-8601=seconds)"
    echo "kernel=$(uname -r)"
    echo "cmdline=$(</proc/cmdline)"
    echo "wlan_operstate=$(</sys/class/net/wlp229s0/operstate)"
    if [[ -r /sys/class/power_supply/ADP1/online ]]; then
        echo "adapter_online=$(</sys/class/power_supply/ADP1/online)"
    fi
    echo "spa_before=$(spa_line)"
    for nhi in "${nhis[@]}"; do
        echo "nhi_before=$(nhi_state "$nhi")"
    done
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
if ! module_loaded thunderbolt || ! module_loaded t2thunderbolt; then
    echo "Thunderbolt modules are not both loaded; aborting" >> "$controller_out"
    exit 1
fi
nhis_bound || {
    echo "NHIs are not both bound to thunderbolt; aborting" >> "$controller_out"
    exit 1
}
tb_topology_is_internal_only || exit 1

printf '0\n' > "$ltr_ignore"
spa_line | grep -q 'LTR_IGNORE: 1' || {
    echo "failed to set SOUTHPORT_A ignore bit" >> "$controller_out"
    exit 1
}
echo "spa_ignored=$(spa_line)" >> "$controller_out"

bash "$measure" 90 15 > "$normal_out"

modprobe -r thunderbolt
module_loaded thunderbolt && {
    echo "thunderbolt still loaded after removal" >> "$controller_out"
    exit 1
}
nhis_unbound || {
    echo "NHIs did not unbind from thunderbolt" >> "$controller_out"
    exit 1
}

modprobe -r t2thunderbolt
module_loaded t2thunderbolt && {
    echo "t2thunderbolt still loaded after removal" >> "$controller_out"
    exit 1
}

for nhi in "${nhis[@]}"; do
    echo "nhi_off=$(nhi_state "$nhi")" >> "$controller_out"
done

bash "$measure" 90 15 > "$off_out"

modprobe t2thunderbolt
modprobe thunderbolt
sleep 5

if ! module_loaded thunderbolt || ! module_loaded t2thunderbolt; then
    echo "Thunderbolt modules did not both reload" >> "$controller_out"
    exit 1
fi
nhis_bound || {
    echo "NHIs did not both rebind to thunderbolt" >> "$controller_out"
    exit 1
}

printf '0\n' > "$ltr_restore"
spa_is_restored || {
    echo "SOUTHPORT_A restore verification failed" >> "$controller_out"
    exit 1
}

trap - EXIT INT TERM HUP

{
    for nhi in "${nhis[@]}"; do
        echo "nhi_after=$(nhi_state "$nhi")"
    done
    echo "spa_after=$(spa_line)"
    echo "completed=$(date --iso-8601=seconds)"
    echo "result=success"
} >> "$controller_out"
