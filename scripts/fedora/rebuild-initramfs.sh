#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command dracut install

INPUT_MODULES=(t2bce_dma t2bce_core t2bce_vhci t2hid)
DRACUT_CONF="/etc/dracut.conf.d/90-kait2en-input.conf"
KVER="$(kernel_release)"

install -d -m 0755 /etc/dracut.conf.d
printf '# Managed by scripts/fedora/rebuild-initramfs.sh\nforce_drivers+=" %s "\n' \
	"${INPUT_MODULES[*]}" >"$DRACUT_CONF"

declare -A kernel_releases=(["$KVER"]=1)
for module_directory in /lib/modules/*; do
	[[ -d "$module_directory" ]] || continue
	kernel_releases["${module_directory##*/}"]=1
done

failed_releases=()
for release in "${!kernel_releases[@]}"; do
	info "rebuilding initramfs for $release"
	if ! dracut --force "/boot/initramfs-$release.img" "$release"; then
		warn "could not rebuild initramfs for $release; continuing"
		failed_releases+=("$release")
	fi
done

if (( ${#failed_releases[@]} > 0 )); then
	warn "initramfs rebuild failed for: ${failed_releases[*]}"
fi
info "initramfs rebuild completed for all installed kernels"
