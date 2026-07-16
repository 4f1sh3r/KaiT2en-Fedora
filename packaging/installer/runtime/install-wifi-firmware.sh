#!/usr/bin/env bash

set -Eeuo pipefail
shopt -s nullglob

source_dir=
root=/

usage() {
	printf 'Usage: %s --source DIR [--root DIR]\n' "${0##*/}"
}

die() {
	printf 'Error: %s\n' "$*" >&2
	exit 1
}

pick_one() {
	local label=$1
	shift
	local files=("$@")

	((${#files[@]} == 1)) ||
		die "expected exactly one $label file in $source_dir, found ${#files[@]}"
	[[ -s "${files[0]}" ]] || die "$label file is empty: ${files[0]}"
	printf '%s\n' "${files[0]}"
}

while (($# > 0)); do
	case "$1" in
		--source)
			(($# >= 2)) || { usage >&2; exit 2; }
			source_dir=$2
			shift 2
			;;
		--root)
			(($# >= 2)) || { usage >&2; exit 2; }
			root=$2
			shift 2
			;;
		-h|--help)
			usage
			exit 0
			;;
		*)
			usage >&2
			exit 2
			;;
	esac
done

[[ -d "$source_dir" ]] || die "firmware directory not found: $source_dir"
[[ -d "$root" ]] || die "target root not found: $root"

trx_file=$(pick_one '.trx firmware' "$source_dir"/*.trx)
clm_file=$(pick_one '.clmb regulatory blob' "$source_dir"/*.clmb)
txcap_file=$(pick_one '.txcb TxCap blob' "$source_dir"/*.txcb)
nvram_file=$(pick_one 'P-*.txt NVRAM' "$source_dir"/P-*.txt)

if [[ -n ${KAIT2EN_FIRMWARE_REQUEST:-} ]]; then
	requested_bin=$KAIT2EN_FIRMWARE_REQUEST
else
	requested_bin=$(
		journalctl -b -k --no-pager |
			sed -n 's/.*Direct firmware load for \(brcm\/brcmfmac[^ ]*\.bin\) failed with error -2.*/\1/p' |
			head -n 1
	)
fi

[[ -n "$requested_bin" ]] ||
	die 'no missing brcmfmac .bin firmware request found in the current boot log'
target_bin=${requested_bin#brcm/}
[[ "$requested_bin" != "$target_bin" ]] ||
	die "unexpected firmware path in journal: $requested_bin"
[[ "$target_bin" == brcmfmac*-pcie.apple,* ]] ||
	die "unexpected brcmfmac filename: $target_bin"

target_prefix=${target_bin%%.apple,*}
clm_stem=$(basename "$clm_file" .clmb)
txcap_stem=$(basename "$txcap_file" .txcb)
nvram_name=$(basename "$nvram_file")

[[ "$clm_stem" == "$txcap_stem" ]] ||
	die '.clmb and .txcb files do not use the same board name'
if [[ "$nvram_name" =~ ^P-([^_]+)_M-([^_]+)_V-([^_]+)__m-(.+)\.txt$ ]]; then
	nvram_board=${BASH_REMATCH[1]}
	nvram_module=${BASH_REMATCH[2]}
	nvram_vendor=${BASH_REMATCH[3]}
	nvram_version=${BASH_REMATCH[4]}
else
	die "unexpected NVRAM filename: $nvram_name"
fi
[[ "$nvram_board" == "$clm_stem" ]] ||
	die 'NVRAM board name does not match .clmb/.txcb board name'

target_clm="${target_prefix}.apple,${clm_stem}.clm_blob"
target_txcap="${target_prefix}.apple,${txcap_stem}.txcap_blob"
target_nvram="${target_prefix}.apple,${nvram_board}-${nvram_module}-${nvram_vendor}-${nvram_version}.txt"
dest_dir="$root/usr/lib/firmware/brcm"

install -d -m 0755 "$dest_dir"
install -m 0644 "$trx_file" "$dest_dir/$target_bin"
install -m 0644 "$clm_file" "$dest_dir/$target_clm"
install -m 0644 "$txcap_file" "$dest_dir/$target_txcap"
install -m 0644 "$nvram_file" "$dest_dir/$target_nvram"

printf 'KaiT2en Wi-Fi firmware installed:\n'
printf '  /usr/lib/firmware/brcm/%s\n' \
	"$target_bin" "$target_clm" "$target_txcap" "$target_nvram"
