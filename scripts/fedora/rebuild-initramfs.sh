#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command depmod dracut install

INPUT_MODULES=(t2bce_dma t2bce_core t2bce_vhci t2hid)
DRACUT_CONF="/etc/dracut.conf.d/90-kait2en-input.conf"
KVER="$(kernel_release)"

install -d -m 0755 /etc/dracut.conf.d
printf '# Managed by scripts/fedora/rebuild-initramfs.sh\nforce_drivers+=" %s "\n' \
	"${INPUT_MODULES[*]}" >"$DRACUT_CONF"

declare -A kernel_releases=(["$KVER"]=1)
for module_directory in /lib/modules/*; do
	[[ -d "$module_directory" ]] || continue
	release="${module_directory##*/}"
	if [[ ! -f "/boot/vmlinuz-$release" ]]; then
		warn "ignoring orphaned module directory without a kernel image: $release"
		continue
	fi
	kernel_releases["$release"]=1
done

failed_releases=()
rebuilt_releases=()
for release in "${!kernel_releases[@]}"; do
	info "rebuilding initramfs for $release"
	if [[ ! -s "/lib/modules/$release/modules.dep" ]]; then
		info "module dependency metadata is missing for $release; running depmod"
		if ! depmod -a "$release"; then
			warn "could not generate module dependency metadata for $release"
			failed_releases+=("$release")
			continue
		fi
	fi
	if ! dracut --force "/boot/initramfs-$release.img" "$release"; then
		warn "could not rebuild initramfs for $release; continuing"
		failed_releases+=("$release")
	else
		rebuilt_releases+=("$release")
	fi
done

if (( ${#failed_releases[@]} > 0 )); then
	fail "initramfs rebuild failed for: ${failed_releases[*]}"
fi
info "initramfs rebuild completed for ${#rebuilt_releases[@]} installed kernel(s)"
