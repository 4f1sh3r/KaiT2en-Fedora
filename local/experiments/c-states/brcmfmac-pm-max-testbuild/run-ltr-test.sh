#!/usr/bin/env bash

set -Eeuo pipefail

if (( EUID != 0 )); then
	echo "run as root" >&2
	exit 1
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
test_module="$script_dir/brcmfmac.ko"
staged_module="/tmp/cstates-brcmfmac-pm-max-$$.ko"
restore="$script_dir/restore-stock.sh"
iface=wlp229s0
expected_kernel=7.1.6-201.fc44.x86_64
expected_sha=0b0acf30932a5cc789103326889664b62ae1d9415d2eeb0da9ebbcd2199b0e5b
ltr_show=/sys/kernel/debug/pmc_core/ltr_show
out=/tmp/cstates-brcmfmac-pm-max-ltr-test.txt
restore_armed=0
preflight_only=0

case "${1:-}" in
	"") ;;
	--preflight) preflight_only=1 ;;
	*)
		echo "usage: ${0##*/} [--preflight]" >&2
		exit 2
		;;
esac

spa_line() {
	awk '$1 == 0 && /SOUTHPORT_A/ { print; found=1 } END { exit !found }' \
		"$ltr_show"
}

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

sample_ltr() {
	local label=$1
	local samples=$2
	local i

	for ((i = 0; i < samples; i++)); do
		printf '%s label=%s operstate=%s powersave=%s spa=' \
			"$(date --iso-8601=ns)" "$label" \
			"$(<"/sys/class/net/$iface/operstate")" \
			"$(iw dev "$iface" get power_save | awk '{print $3}')"
		spa_line
		sleep 0.5
	done
}

emergency_restore() {
	local exit_status=$?
	trap - EXIT INT TERM HUP

	if (( restore_armed == 1 )); then
		{
			echo "emergency_restore_begin=$(date --iso-8601=seconds)"
			"$restore" || true
			echo "emergency_restore_end=$(date --iso-8601=seconds)"
		} >> "$out" 2>&1
	fi
	rm -f -- "$staged_module"
	exit "$exit_status"
}

trap emergency_restore EXIT INT TERM HUP

for path in "$test_module" "$restore" "$ltr_show"; do
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
ip -4 route show default | awk -v wlan="$iface" '$5 != wlan { found=1 } END { exit !found }' || {
	echo "no non-WLAN IPv4 default route; refusing to interrupt Codex connectivity" >&2
	exit 1
}
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

{
	echo "started=$(date --iso-8601=seconds)"
	echo "kernel=$(uname -r)"
	echo "module_sha256=$expected_sha"
	echo "route_before=$(ip -4 route show default | tr '\n' ';')"
	echo "link_stock_begin"
	iw dev "$iface" link
	echo "link_stock_end"
	sample_ltr pm_fast_stock 20
} > "$out"

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
	echo "PM_MAX test parameter is not active" >> "$out"
	exit 1
}
iw dev "$iface" set power_save on

{
	echo "pm_max_parameter=$(</sys/module/brcmfmac/parameters/power_save_mode)"
	echo "link_pm_max_begin"
	iw dev "$iface" link
	echo "link_pm_max_end"
	sample_ltr pm_max 30
	echo "spa_pm_max_after=$(spa_line)"
	echo "stock_restore_begin=$(date --iso-8601=seconds)"
} >> "$out"

"$restore" >> "$out" 2>&1
restore_armed=0

{
	echo "stock_restore_end=$(date --iso-8601=seconds)"
	echo "link_restored_begin"
	iw dev "$iface" link
	echo "link_restored_end"
	sample_ltr pm_fast_restored 10
	echo "spa_after=$(spa_line)"
	echo "completed=$(date --iso-8601=seconds)"
	echo "result=success"
} >> "$out"

rm -f -- "$staged_module"
trap - EXIT INT TERM HUP
