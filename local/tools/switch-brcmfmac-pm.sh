#!/usr/bin/env bash

set -Eeuo pipefail

export LC_ALL=C

usage() {
	cat <<'EOF'
Usage: switch-brcmfmac-pm.sh {max|fast|status}

  max     Reload brcmfmac with max_pm=1 (PM_MAX when Wi-Fi power save is on)
  fast    Reload brcmfmac with max_pm=0 (PM_FAST when Wi-Fi power save is on)
  status  Show the currently selected brcmfmac default and Wi-Fi power-save state

The max/fast change is runtime-only. The existing modprobe configuration remains
unchanged, so the configured default (currently PM_MAX) returns after a reboot.

Reloading brcmfmac disconnects Wi-Fi for a short time.
EOF
}

module_loaded() {
	grep -q "^$1 " /proc/modules
}

selected_pm() {
	local value

	if ! module_loaded brcmfmac; then
		printf '%s\n' "unloaded"
		return
	fi

	value=$(< /sys/module/brcmfmac/parameters/max_pm)
	case "$value" in
	Y) printf '%s\n' "PM_MAX" ;;
	N) printf '%s\n' "PM_FAST" ;;
	*) printf 'unknown (%s)\n' "$value" ;;
	esac
}

show_status() {
	local iface

	printf 'brcmfmac selection: %s\n' "$(selected_pm)"

	if command -v iw >/dev/null 2>&1; then
		while read -r iface; do
			printf '%s: ' "$iface"
			iw dev "$iface" get power_save 2>/dev/null || printf '%s\n' "power-save state unavailable"
		done < <(iw dev 2>/dev/null | awk '$1 == "Interface" { print $2 }')
	fi

	if command -v nmcli >/dev/null 2>&1; then
		nmcli --colors no -f DEVICE,TYPE,STATE,CONNECTION device status 2>/dev/null || true
	fi
}

restore_radio() {
	if [[ ${wifi_was_enabled:-} == "enabled" ]]; then
		nmcli radio wifi on || true
	fi
}

reload_previous_state() {
	printf '%s\n' "Reload failed; attempting to restore the previous driver state." >&2
	if module_loaded brcmfmac_wcc; then
		modprobe -r brcmfmac_wcc || true
	fi
	if module_loaded brcmfmac; then
		modprobe -r brcmfmac || true
	fi
	if ! module_loaded brcmfmac; then
		modprobe brcmfmac "max_pm=$previous_value" || true
	fi
	if (( wcc_was_loaded )) && ! module_loaded brcmfmac_wcc; then
		modprobe brcmfmac_wcc || true
	fi
	restore_radio
}

case ${1:-} in
status)
	if (( EUID != 0 )); then
		exec sudo -- "$0" status
	fi
	show_status
	exit 0
	;;
max)
	target_value=1
	target_name=PM_MAX
	;;
fast)
	target_value=0
	target_name=PM_FAST
	;;
-h|--help|help)
	usage
	exit 0
	;;
*)
	usage >&2
	exit 2
	;;
esac

if (( EUID != 0 )); then
	exec sudo -- "$0" "$1"
fi

for command in modprobe nmcli; do
	if ! command -v "$command" >/dev/null 2>&1; then
		printf 'Required command not found: %s\n' "$command" >&2
		exit 1
	fi
done

if ! module_loaded brcmfmac; then
	printf '%s\n' "brcmfmac is not currently loaded." >&2
	exit 1
fi

if ! modinfo -p brcmfmac | grep -q '^max_pm:'; then
	printf '%s\n' "The loaded kernel's brcmfmac module has no max_pm parameter." >&2
	exit 1
fi

current_raw=$(< /sys/module/brcmfmac/parameters/max_pm)
case "$current_raw" in
Y) previous_value=1 ;;
N) previous_value=0 ;;
*)
	printf 'Unexpected max_pm value: %s\n' "$current_raw" >&2
	exit 1
	;;
esac

if [[ $previous_value == "$target_value" ]]; then
	printf 'brcmfmac is already configured for %s. No reload performed.\n' "$target_name"
	show_status
	exit 0
fi

printf 'Switching brcmfmac to %s. Wi-Fi will disconnect briefly.\n' "$target_name"

wifi_was_enabled=$(nmcli -g WIFI general 2>/dev/null || true)
wcc_was_loaded=0
if module_loaded brcmfmac_wcc; then
	wcc_was_loaded=1
fi

if [[ $wifi_was_enabled == "enabled" ]]; then
	nmcli radio wifi off
fi

if (( wcc_was_loaded )); then
	if ! modprobe -r brcmfmac_wcc; then
		restore_radio
		exit 1
	fi
fi

if ! modprobe -r brcmfmac; then
	if (( wcc_was_loaded )); then
		modprobe brcmfmac_wcc || true
	fi
	restore_radio
	exit 1
fi

if ! modprobe brcmfmac "max_pm=$target_value"; then
	reload_previous_state
	exit 1
fi

if (( wcc_was_loaded )) && ! modprobe brcmfmac_wcc; then
	reload_previous_state
	exit 1
fi

restore_radio

# Give udev and NetworkManager a moment to recreate and reconnect the interface.
for _ in {1..30}; do
	if nmcli -t -f TYPE,STATE device status 2>/dev/null | grep -q '^wifi:connected$'; then
		break
	fi
	sleep 0.5
done

actual_raw=$(< /sys/module/brcmfmac/parameters/max_pm)
if [[ $target_value == 1 && $actual_raw != Y ]] ||
	[[ $target_value == 0 && $actual_raw != N ]]; then
	printf 'Verification failed: max_pm is %s after reload.\n' "$actual_raw" >&2
	reload_previous_state
	exit 1
fi

printf 'Switch complete. This runtime selection will be reset by a reboot.\n'
show_status
