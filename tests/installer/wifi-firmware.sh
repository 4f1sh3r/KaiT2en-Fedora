#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
helper="$repo_root/packaging/installer/runtime/install-wifi-firmware.sh"
work=$(mktemp -d "${TMPDIR:-/tmp}/kait2en-wifi-test.XXXXXX")
trap 'rm -rf "$work"' EXIT

run_case() {
	local name=$1 chip=$2 board=$3 module=$4 vendor=$5 version=$6 suffix=$7
	local source="$work/$name/source" root="$work/$name/root"
	local request="brcm/brcmfmac${chip}-pcie.apple,${board}-${module}-${vendor}-${version}-${suffix}.bin"
	local prefix="brcmfmac${chip}-pcie"

	mkdir -p "$source" "$root"
	printf firmware >"$source/${name}.trx"
	printf clm >"$source/${board}.clmb"
	printf txcap >"$source/${board}.txcb"
	printf nvram >"$source/P-${board}_M-${module}_V-${vendor}__m-${version}.txt"

	KAIT2EN_FIRMWARE_REQUEST=$request bash "$helper" --source "$source" --root "$root" >/dev/null

	local destination="$root/usr/lib/firmware/brcm"
	[[ -f "$destination/${request#brcm/}" ]]
	[[ -f "$destination/${prefix}.apple,${board}.clm_blob" ]]
	[[ -f "$destination/${prefix}.apple,${board}.txcap_blob" ]]
	[[ -f "$destination/${prefix}.apple,${board}-${module}-${vendor}-${version}.txt" ]]
	[[ $(find "$destination" -maxdepth 1 -type f | wc -l) -eq 4 ]]
}

run_case bcm4364 4364b3 trinidad-X3 HRPN u 7.7 X3
run_case bcm4377 4377b3 bali-X0 HRPN u 8.0 X0

# A generic fallback name is intentionally rejected. The helper must use the
# hardware-specific filename emitted by brcmfmac, exactly like the manual guide.
mkdir -p "$work/reject/source" "$work/reject/root"
printf x >"$work/reject/source/test.trx"
printf x >"$work/reject/source/board.clmb"
printf x >"$work/reject/source/board.txcb"
printf x >"$work/reject/source/P-board_M-module_V-vendor__m-1.txt"
if KAIT2EN_FIRMWARE_REQUEST=brcm/brcmfmac4364-pcie.txt \
	bash "$helper" --source "$work/reject/source" --root "$work/reject/root" \
	>/dev/null 2>&1; then
	printf 'generic brcmfmac firmware request was unexpectedly accepted\n' >&2
	exit 1
fi
