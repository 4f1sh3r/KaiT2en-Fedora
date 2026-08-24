#!/usr/bin/env bash

set -uo pipefail

if (( EUID != 0 )); then
	echo "run as root" >&2
	exit 1
fi

iface=wlp229s0

module_loaded() {
	[[ -d "/sys/module/$1" ]]
}

wait_for_wlan() {
	local attempt

	for ((attempt = 0; attempt < 60; attempt++)); do
		if [[ -e "/sys/class/net/$iface" ]] &&
		   [[ "$(<"/sys/class/net/$iface/operstate")" == up ]] &&
		   iw dev "$iface" link 2>/dev/null | grep -q '^Connected to '; then
			return 0
		fi
		sleep 1
	done
	return 1
}

status=0

nmcli radio wifi off || status=1
sleep 2

if module_loaded brcmfmac_wcc; then
	modprobe -r brcmfmac_wcc || status=1
fi
if module_loaded brcmfmac; then
	modprobe -r brcmfmac || status=1
fi

if module_loaded brcmfmac; then
	echo "could not unload the active brcmfmac module" >&2
	status=1
else
	modprobe brcmfmac || status=1
	modprobe brcmfmac_wcc || status=1
fi

nmcli radio wifi on || status=1

if ! wait_for_wlan; then
	echo "WLAN did not reconnect after stock restore" >&2
	status=1
fi

if [[ -e /sys/module/brcmfmac/parameters/power_save_mode ]]; then
	echo "test brcmfmac is still active after restore" >&2
	status=1
fi

if (( status == 0 )); then
	echo "stock_restore=success"
else
	echo "stock_restore=failed" >&2
fi

exit "$status"
