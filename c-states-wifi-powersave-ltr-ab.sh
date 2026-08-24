#!/usr/bin/env bash
# Short reversible LTR test: brcmfmac PM_FAST (power save on) versus PM_OFF.

set -euo pipefail

if (( EUID != 0 )); then
    echo "run as root" >&2
    exit 1
fi

iface=wlp229s0
ltr_show=/sys/kernel/debug/pmc_core/ltr_show
out=/tmp/cstates-wifi-powersave-ltr-ab.txt

for command in iw awk grep date sleep; do
    command -v "$command" >/dev/null || {
        echo "required command missing: $command" >&2
        exit 1
    }
done
[[ -r "$ltr_show" ]] || {
    echo "required path missing: $ltr_show" >&2
    exit 1
}
[[ -e "/sys/class/net/$iface" ]] || {
    echo "interface missing: $iface" >&2
    exit 1
}

spa_line() {
    awk '$1 == 0 && /SOUTHPORT_A/ { print; found=1 } END { exit !found }' \
        "$ltr_show"
}

power_save_state() {
    iw dev "$iface" get power_save | awk '{print $3}'
}

restore() {
    iw dev "$iface" set power_save on || true
}

trap restore EXIT INT TERM HUP

[[ "$(</sys/class/net/$iface/operstate)" == up ]] || {
    echo "WLAN is not up" >&2
    exit 1
}
[[ "$(power_save_state)" == on ]] || {
    echo "WLAN power save is not initially on" >&2
    exit 1
}
spa_line | grep -q 'LTR_IGNORE: 0' || {
    echo "SOUTHPORT_A LTR is ignored; aborting" >&2
    exit 1
}

sample_window() {
    local label=$1
    local samples=$2
    local i

    for ((i = 0; i < samples; i++)); do
        printf '%s label=%s operstate=%s powersave=%s spa=' \
            "$(date --iso-8601=ns)" "$label" \
            "$(</sys/class/net/$iface/operstate)" "$(power_save_state)"
        spa_line
        sleep 0.5
    done
}

{
    echo "started=$(date --iso-8601=seconds)"
    echo "link_before_begin"
    iw dev "$iface" link
    echo "link_before_end"
    sample_window pm_fast 30

    iw dev "$iface" set power_save off
    sleep 2
    sample_window pm_off 30

    iw dev "$iface" set power_save on
    sleep 2
    sample_window pm_fast_restored 20

    echo "link_after_begin"
    iw dev "$iface" link
    echo "link_after_end"
    echo "spa_after=$(spa_line)"
    echo "completed=$(date --iso-8601=seconds)"
    echo "result=success"
} > "$out"

trap - EXIT INT TERM HUP
