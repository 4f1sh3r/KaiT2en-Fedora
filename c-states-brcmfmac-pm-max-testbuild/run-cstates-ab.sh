#!/usr/bin/env bash
# Quiet-desktop A/B: packaged brcmfmac PM_FAST, then test brcmfmac PM_MAX.
# The packaged Fedora modules and WLAN association are restored on every exit.

set -Eeuo pipefail

if (( EUID != 0 )); then
	echo "run as root" >&2
	exit 1
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(dirname -- "$script_dir")"
test_module="$script_dir/brcmfmac.ko"
staged_module="/tmp/cstates-brcmfmac-pm-max-ab-$$.ko"
restore="$script_dir/restore-stock.sh"
measure="$repo_dir/c-states-measure.sh"
iface=wlp229s0
expected_kernel=7.1.6-201.fc44.x86_64
expected_sha=0b0acf30932a5cc789103326889664b62ae1d9415d2eeb0da9ebbcd2199b0e5b
ltr_show=/sys/kernel/debug/pmc_core/ltr_show
controller_out=/tmp/cstates-brcmfmac-pm-max-ab-controller.txt
stock_out=/tmp/cstates-brcmfmac-pm-fast-stock.txt
pm_max_out=/tmp/cstates-brcmfmac-pm-max.txt
measurement_seconds=45
initial_grace_seconds=12
settle_seconds=5
restore_armed=0
preflight_only=0
standalone_mode=0
pm_max_only=0

for arg in "$@"; do
	case "$arg" in
		--preflight) preflight_only=1 ;;
		--standalone) standalone_mode=1 ;;
		--pm-max-only) pm_max_only=1 ;;
		*)
			echo "usage: ${0##*/} [--preflight] [--standalone] [--pm-max-only]" >&2
			exit 2
			;;
	esac
done

spa_line() {
	awk '$1 == 0 && /SOUTHPORT_A/ { print; found=1 } END { exit !found }' \
		"$ltr_show"
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

restore_on_exit() {
	local exit_status=$?
	trap - EXIT INT TERM HUP

	if (( restore_armed == 1 )); then
		{
			echo "emergency_restore_begin=$(date --iso-8601=seconds)"
			"$restore" || true
			echo "emergency_restore_end=$(date --iso-8601=seconds)"
		} >> "$controller_out" 2>&1
	fi
	rm -f -- "$staged_module"
	exit "$exit_status"
}

trap restore_on_exit EXIT INT TERM HUP

for path in "$test_module" "$restore" "$measure" "$ltr_show"; do
	[[ -e "$path" ]] || {
		echo "required path missing: $path" >&2
		exit 1
	}
done

[[ "$(uname -r)" == "$expected_kernel" ]] || {
	echo "wrong kernel: $(uname -r), expected $expected_kernel" >&2
	exit 1
}
[[ "$(sha256sum "$test_module" | awk '{print $1}')" == "$expected_sha" ]] || {
	echo "test module checksum mismatch" >&2
	exit 1
}
[[ "$(nmcli radio wifi)" == enabled ]] || {
	echo "WLAN radio is not enabled" >&2
	exit 1
}
wait_for_wlan || {
	echo "WLAN is not associated before the test" >&2
	exit 1
}
if (( standalone_mode == 1 )); then
	[[ -r /sys/class/power_supply/ADP1/online ]] &&
	[[ "$(</sys/class/power_supply/ADP1/online)" == 0 ]] || {
		echo "standalone mode requires battery power" >&2
		exit 1
	}
	ip -4 route show default | awk -v wlan="$iface" '
		$5 == wlan { wifi=1 }
		$5 != wlan { other=1 }
		END { exit !(wifi && !other) }
	' || {
		echo "standalone mode requires WLAN as the only IPv4 default route" >&2
		exit 1
	}
else
	ip -4 route show default | awk -v wlan="$iface" '$5 != wlan { found=1 } END { exit !found }' || {
		echo "no non-WLAN IPv4 default route; use --standalone only on battery" >&2
		exit 1
	}
fi
spa_line | grep -q 'LTR_IGNORE: 0' || {
	echo "SOUTHPORT_A LTR is ignored; aborting" >&2
	exit 1
}
[[ ! -e /sys/module/brcmfmac/parameters/power_save_mode ]] || {
	echo "a test brcmfmac is already active" >&2
	exit 1
}

if (( preflight_only == 1 )); then
	trap - EXIT INT TERM HUP
	echo "preflight=success"
	exit 0
fi

install -m 0600 -- "$test_module" "$staged_module"
chcon -t modules_object_t -- "$staged_module"
rm -f -- "$pm_max_out"
if (( pm_max_only == 0 )); then
	rm -f -- "$stock_out"
fi

{
	echo "started=$(date --iso-8601=seconds)"
	echo "kernel=$(uname -r)"
	echo "module_sha256=$expected_sha"
	echo "measurement_seconds=$measurement_seconds"
	echo "initial_grace_seconds=$initial_grace_seconds"
	echo "settle_seconds=$settle_seconds"
	echo "standalone_mode=$standalone_mode"
	echo "pm_max_only=$pm_max_only"
	echo "route_before=$(ip -4 route show default | tr '\n' ';')"
	if [[ -r /sys/class/power_supply/ADP1/online ]]; then
		echo "adapter_online=$(</sys/class/power_supply/ADP1/online)"
	fi
	echo "spa_stock_before=$(spa_line)"
	echo "link_stock_begin"
	iw dev "$iface" link
	echo "link_stock_end"
} > "$controller_out"

sleep "$initial_grace_seconds"
if (( pm_max_only == 0 )); then
	bash "$measure" "$measurement_seconds" "$settle_seconds" > "$stock_out"
fi

restore_armed=1
nmcli radio wifi off
sleep 2
modprobe -r brcmfmac_wcc
modprobe -r brcmfmac
insmod "$staged_module" power_save_mode=1
modprobe brcmfmac_wcc
nmcli radio wifi on
wait_for_wlan

[[ "$(</sys/module/brcmfmac/parameters/power_save_mode)" == 1 ]] || {
	echo "PM_MAX test parameter is not active" >> "$controller_out"
	exit 1
}
iw dev "$iface" set power_save on

{
	echo "pm_max_parameter=$(</sys/module/brcmfmac/parameters/power_save_mode)"
	echo "spa_pm_max_before=$(spa_line)"
	echo "link_pm_max_begin"
	iw dev "$iface" link
	echo "link_pm_max_end"
} >> "$controller_out"

bash "$measure" "$measurement_seconds" "$settle_seconds" > "$pm_max_out"

echo "stock_restore_begin=$(date --iso-8601=seconds)" >> "$controller_out"
"$restore" >> "$controller_out" 2>&1
restore_armed=0
rm -f -- "$staged_module"

[[ ! -e /sys/module/brcmfmac/parameters/power_save_mode ]] || {
	echo "test parameter remains after restore" >> "$controller_out"
	exit 1
}
wait_for_wlan

{
	echo "stock_restore_end=$(date --iso-8601=seconds)"
	echo "spa_after=$(spa_line)"
	echo "link_restored_begin"
	iw dev "$iface" link
	echo "link_restored_end"
	echo "completed=$(date --iso-8601=seconds)"
	echo "result=success"
} >> "$controller_out"

trap - EXIT INT TERM HUP
