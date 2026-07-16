#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
cd "$repo_root"

shell_files=(
	packaging/installer/build-in-container.sh
	packaging/installer/build-input-kmod.sh
	packaging/installer/initramfs/20-kait2en-input.sh.in
	packaging/installer/initramfs/90-kait2en-updates.sh
	packaging/installer/macos-release-bootstrap.sh.in
	packaging/installer/runtime/install-wifi-firmware.sh
	packaging/installer/runtime/kait2en-install
	packaging/installer/runtime/kait2en-launch-terminal
	packaging/installer/runtime/kait2en-prepare
	scripts/fedora/build-installer.sh
	scripts/macos/prepare-fedora-installer.sh
	tests/installer/edition-catalog.sh
	tests/installer/install-launcher.sh
	tests/installer/static-check.sh
	tests/installer/prepare-install.sh
	tests/installer/release-bootstrap.sh
	tests/installer/terminal-launcher.sh
	tests/installer/wifi-firmware.sh
)
for file in "${shell_files[@]}"; do
	bash -n "$file"
done

if command -v shellcheck >/dev/null 2>&1; then
	shellcheck --severity=warning -x "${shell_files[@]}"
fi

while IFS= read -r file; do
	python3 -c \
		'import ast, pathlib, sys; ast.parse(pathlib.Path(sys.argv[1]).read_text())' \
		"$file"
done < <(git ls-files 'packaging/installer/anaconda-addon/*.py' \
	'packaging/installer/anaconda-addon/**/*.py')

! rg -n 'OEMDRV|rhdd3|inst\.dd=|inst\.ks=|kait2en\.wifi_required' \
	packaging/installer/grub.cfg.in \
	packaging/installer/initramfs \
	packaging/installer/anaconda-addon \
	scripts/macos
! rg -n 'brcmfmac(4364|4377).*alias|generic.*brcmfmac|brcmfmac[^ ]*-pcie\.txt' \
	packaging/installer
! rg -n 'INPUT_COMPAT_PATCH|compat_patch|packaging/installer/patches' \
	packaging/installer/runtime/kait2en-prepare
grep -Fq '"$transition_source" "$target_kernel" "$work/rpm"' \
	packaging/installer/runtime/kait2en-prepare
grep -Fq '"etc", "xdg", "autostart"' \
	packaging/installer/anaconda-addon/com_kait2en_input/service/installation.py
! rg -n 'find_regular_user|home\.lstrip|os\.chown' \
	packaging/installer/anaconda-addon/com_kait2en_input/service/installation.py
grep -Fq 'KAIT2EN_AUTOSTART_FILE:-/etc/xdg/autostart/kait2en-install.desktop' \
	packaging/installer/runtime/kait2en-prepare
! rg -n '\$HOME/\.config/autostart' \
	packaging/installer/runtime/kait2en-install
if rg -n 'kait2en-first-boot|KAIT2EN_FIRST_BOOT' packaging/installer; then
	exit 1
fi

# macOS Bash 3.2 treats an expanded empty array as unbound under `set -u`.
grep -Fq 'ORIGINAL_ARGC=$#' scripts/macos/prepare-fedora-installer.sh
grep -Fq 'if ((ORIGINAL_ARGC == 0)); then' scripts/macos/prepare-fedora-installer.sh
grep -Fq 'plist_value "$disk" WholeDisk' scripts/macos/prepare-fedora-installer.sh
! grep -Fq 'plist_value "$disk" Whole ' scripts/macos/prepare-fedora-installer.sh
grep -Fq 'shasum -a 256 -c' packaging/installer/macos-release-bootstrap.sh.in
grep -Fq 'KAIT2EN_TTY:-/dev/tty' packaging/installer/macos-release-bootstrap.sh.in

patch_name=$(
	# shellcheck disable=SC1091
	source packaging/installer/targets/fedora-44.conf
	printf '%s\n' "$INPUT_COMPAT_PATCH"
)
[[ -f "packaging/installer/patches/$patch_name" ]]

git apply --unidiff-zero --check "packaging/installer/patches/$patch_name"

bash tests/installer/wifi-firmware.sh
bash tests/installer/prepare-install.sh
bash tests/installer/install-launcher.sh
bash tests/installer/release-bootstrap.sh
bash tests/installer/terminal-launcher.sh
bash tests/installer/edition-catalog.sh
printf 'Installer static checks passed.\n'
