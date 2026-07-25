#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command cp install plymouth-set-default-theme rm

defer_initramfs=false
if [[ ${1:-} == --defer-initramfs ]]; then
	defer_initramfs=true
	shift
fi
(( $# == 0 )) || fail "usage: $0 [--defer-initramfs]"

THEME_SRC="$REPO_ROOT/assets/plymouth/kait2en"
THEME_DST="/usr/share/plymouth/themes/kait2en"
SPINNER_SRC="/usr/share/plymouth/themes/spinner"

[[ -r "$THEME_SRC/kait2en.plymouth" ]] ||
	fail "missing KaiT2en Plymouth theme definition"
[[ -r "$THEME_SRC/watermark.png" ]] ||
	fail "missing KaiT2en Plymouth logo"
[[ -r "$SPINNER_SRC/spinner.plymouth" ]] ||
	fail "Fedora spinner Plymouth theme is not installed"

info "installing KaiT2en Plymouth theme"

rm -rf "$THEME_DST"
install -d -o root -g root -m 0755 "$THEME_DST"
cp -a "$SPINNER_SRC/." "$THEME_DST/"
rm -f "$THEME_DST/spinner.plymouth"
install -o root -g root -m 0644 \
	"$THEME_SRC/kait2en.plymouth" "$THEME_DST/kait2en.plymouth"
install -o root -g root -m 0644 \
	"$THEME_SRC/watermark.png" "$THEME_DST/watermark.png"

if "$defer_initramfs"; then
	plymouth-set-default-theme kait2en
else
	plymouth-set-default-theme -R kait2en
fi

info "KaiT2en Plymouth theme installed"
