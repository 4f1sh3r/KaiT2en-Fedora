#!/usr/bin/env bash
# Reversible locked-screen package C-state measurement with Wi-Fi radio off.

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
result=/tmp/cstates-wifi-off-locked.txt

restore_wifi() {
    nmcli radio wifi on >/dev/null 2>&1 || true
}
trap restore_wifi EXIT INT TERM HUP

nmcli radio wifi off
bash "$script_dir/c-states-measure.sh" 120 15 > "$result" 2>&1

restore_wifi
trap - EXIT INT TERM HUP
sleep 12
{
    echo
    echo "wifi_restore_check"
    echo "timestamp=$(date --iso-8601=seconds)"
    echo "wifi_radio=$(nmcli -t -f WIFI general 2>/dev/null || true)"
    ip -brief link show wlp229s0 2>/dev/null || true
} >> "$result"
