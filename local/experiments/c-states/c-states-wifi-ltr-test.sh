#!/usr/bin/env bash
# Short, reversible Wi-Fi/LTR correlation test. No PCI or LTR settings change.

set -u

result=/tmp/kait2en-wifi-ltr-ab.txt
exec > "$result" 2>&1

restore_wifi() {
    nmcli radio wifi on >/dev/null 2>&1 || true
}
trap restore_wifi EXIT INT TERM HUP

snapshot() {
    local label="$1"
    echo
    echo "[$label]"
    echo "timestamp=$(date --iso-8601=seconds)"
    echo "wifi_radio=$(nmcli -t -f WIFI general 2>/dev/null || true)"
    echo "wlan_runtime_control=$(</sys/bus/pci/devices/0000:e5:00.0/power/control)"
    echo "wlan_runtime_status=$(</sys/bus/pci/devices/0000:e5:00.0/power/runtime_status)"
    echo "wlan_d_state=$(awk '/^D[0-3]/ { print $1 }' /sys/bus/pci/devices/0000:e5:00.0/power_state 2>/dev/null || true)"
    awk '/SOUTHPORT_A|SOUTHPORT_B|CURRENT_PLATFORM|AGGREGATED_SYSTEM/' \
        /sys/kernel/debug/pmc_core/ltr_show
    ip -brief link show wlp229s0 2>/dev/null || true
}

echo "wifi_ltr_ab_test"
snapshot before

nmcli radio wifi off
sleep 3
snapshot wifi_off_3s
sleep 7
snapshot wifi_off_10s

restore_wifi
trap - EXIT INT TERM HUP
sleep 12
snapshot restored_12s
