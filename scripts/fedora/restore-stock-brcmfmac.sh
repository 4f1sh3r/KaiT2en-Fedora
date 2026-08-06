#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command dkms dracut find grep install lsinitrd modinfo modprobe depmod rm sed

PACKAGE=brcmfmac_kait2en
KERNEL_HOOK=/etc/kernel/install.d/39-kait2en-dkms-cleanup.install
DKMS_POST_TRANSACTION_OVERRIDE=/etc/dkms/framework.conf.d/kait2en-disable-post-transaction.conf
reload=false

usage() {
	cat <<'EOF'
Usage: sudo bash scripts/fedora/restore-stock-brcmfmac.sh [--reload]

Remove KaiT2en's brcmfmac DKMS package and restore Fedora's in-tree module.
Without --reload, a currently loaded KaiT2en module remains in memory until
the next reboot. --reload switches the running system immediately and briefly
disconnects Wi-Fi.
EOF
}

case ${1:-} in
	'') ;;
	--reload) reload=true ;;
	-h|--help)
		usage
		exit 0
		;;
	*)
		usage >&2
		fail "unknown argument: $1"
		;;
esac
[[ $# -le 1 ]] || {
	usage >&2
	fail "too many arguments"
}

dkms_versions() {
	dkms status -m "$PACKAGE" 2>/dev/null |
		sed -n "s|^$PACKAGE/\\([^,:]*\\)[,:].*|\\1|p"
}

restore_dkms_post_transaction() {
	rm -f "$DKMS_POST_TRANSACTION_OVERRIDE"
}

disable_dkms_post_transaction() {
	local tmp
	install -d -o root -g root -m 0755 /etc/dkms/framework.conf.d
	tmp=$(mktemp)
	printf 'post_transaction=""\n' >"$tmp"
	install -o root -g root -m 0644 "$tmp" "$DKMS_POST_TRANSACTION_OVERRIDE"
	rm -f "$tmp"
	trap restore_dkms_post_transaction EXIT
}

module_is_kait2en() {
	[[ -e /sys/module/brcmfmac/parameters/power_save_mode ]] &&
		[[ $(modinfo -F kait2en_pm_default brcmfmac 2>/dev/null || true) == PM_MAX ]]
}

loaded_kait2en=false
wcc_was_loaded=false
[[ -e /sys/module/brcmfmac/parameters/power_save_mode ]] && loaded_kait2en=true
[[ -d /sys/module/brcmfmac_wcc ]] && wcc_was_loaded=true

disable_dkms_post_transaction
while IFS= read -r version; do
	[[ -n "$version" ]] || continue
	info "removing DKMS package $PACKAGE/$version"
	dkms remove -m "$PACKAGE" -v "$version" --all
done < <(dkms_versions)
restore_dkms_post_transaction
trap - EXIT

while IFS= read -r -d '' source; do
	base=${source##*/}
	[[ "$source" == /usr/src/brcmfmac_kait2en-* && "$base" == brcmfmac_kait2en-* ]] ||
		fail "refusing to remove unexpected source path: $source"
	info "removing DKMS source $base"
	rm -rf "$source"
done < <(find /usr/src -mindepth 1 -maxdepth 1 -type d \
	-name 'brcmfmac_kait2en-*' -print0)

if dkms status -m "$PACKAGE" 2>/dev/null | grep -q .; then
	fail "DKMS still contains $PACKAGE state"
fi

if [[ -f "$KERNEL_HOOK" ]] && grep -Eq '^[[:space:]]*brcmfmac_kait2en[[:space:]]*$' "$KERNEL_HOOK"; then
	tmp=$(mktemp)
	sed '/^[[:space:]]*brcmfmac_kait2en[[:space:]]*$/d' "$KERNEL_HOOK" >"$tmp"
	install -o root -g root -m 0755 "$tmp" "$KERNEL_HOOK"
	rm -f "$tmp"
	info "removed $PACKAGE from the installed DKMS kernel hook"
fi

depmod -a "$(kernel_release)"

for image in /boot/initramfs-*.img; do
	[[ -f "$image" ]] || continue
	if ! lsinitrd "$image" | grep -E '/(extra|updates/dkms)/brcmfmac\.ko' >/dev/null; then
		continue
	fi

	base=${image##*/}
	kernel=${base#initramfs-}
	kernel=${kernel%.img}
	[[ -d "/lib/modules/$kernel" ]] ||
		fail "cannot rebuild $image: /lib/modules/$kernel is missing"
	info "rebuilding $image without KaiT2en brcmfmac"
	dracut --force "$image" "$kernel"
done

if $reload && $loaded_kait2en; then
	info "unloading the KaiT2en brcmfmac module; Wi-Fi will disconnect briefly"
	if $wcc_was_loaded; then
		modprobe -r brcmfmac_wcc
	fi
	if ! modprobe -r brcmfmac; then
		$wcc_was_loaded && modprobe brcmfmac_wcc || true
		fail "could not unload brcmfmac; the on-disk stock restore is complete"
	fi
	modprobe brcmfmac
	$wcc_was_loaded && modprobe brcmfmac_wcc

	[[ ! -e /sys/module/brcmfmac/parameters/power_save_mode ]] ||
		fail "KaiT2en brcmfmac is still active after reload"
	info "Fedora's stock brcmfmac module is active"
elif $loaded_kait2en; then
	warn "KaiT2en brcmfmac remains active in memory until the next reboot"
	warn "reboot to complete the runtime restore, or rerun with --reload"
elif module_is_kait2en; then
	fail "module resolution still selects KaiT2en brcmfmac"
else
	info "Fedora's stock brcmfmac module remains active"
fi

info "KaiT2en brcmfmac DKMS files removed"
