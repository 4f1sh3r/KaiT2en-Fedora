#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora

BRANDING_SRC="$REPO_ROOT/assets/gdm/00-kait2en"
LOGO_SRC="$REPO_ROOT/assets/gdm/kait2en-gdm-logo.png"
BACKGROUND_SRC="$REPO_ROOT/assets/gdm/gdm-black.png"
BRANDING_DST=/etc/dconf/db/gdm.d/00-kait2en
LOGO_DST=/usr/share/pixmaps/kait2en-gdm-logo.png
BACKGROUND_DST=/usr/share/backgrounds/kait2en/gdm-black.png

missing_requirements=()
for command_name in dconf glib-compile-resources gresource install mktemp readlink rm; do
	command -v "$command_name" >/dev/null 2>&1 ||
		missing_requirements+=("command $command_name")
done
[[ -d /etc/dconf/db/gdm.d ]] || missing_requirements+=("GDM dconf database")
[[ -r "$BRANDING_SRC" ]] || missing_requirements+=("asset 00-kait2en")
[[ -r "$LOGO_SRC" ]] || missing_requirements+=("asset kait2en-gdm-logo.png")
[[ -r "$BACKGROUND_SRC" ]] || missing_requirements+=("asset gdm-black.png")

if (( ${#missing_requirements[@]} > 0 )); then
	warn "skipping GDM branding; missing: ${missing_requirements[*]}"
	exit 0
fi

patch_gnome_shell_theme() {
	local css_file file_name manifest resource_path theme_resource work
	theme_resource=$(readlink -f /usr/share/gnome-shell/gnome-shell-theme.gresource) || return 1
	[[ -r "$theme_resource" ]] || return 1

	work=$(mktemp -d /tmp/kait2en-gdm-theme.XXXXXX) || return 1
	manifest="$work/gnome-shell-theme.gresource.xml"

	printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
		'<gresources>' \
		'  <gresource prefix="/org/gnome/shell/theme">' >"$manifest"

	while IFS= read -r resource_path; do
		[[ "$resource_path" == /org/gnome/shell/theme/* ]] || continue
		file_name=${resource_path##*/}
		if ! gresource extract "$theme_resource" "$resource_path" >"$work/$file_name"; then
			rm -rf "$work"
			return 1
		fi
		printf '    <file>%s</file>\n' "$file_name" >>"$manifest"
	done < <(gresource list "$theme_resource")
	printf '%s\n' '  </gresource>' '</gresources>' >>"$manifest"

	for css_file in "$work"/gnome-shell-*.css; do
		[[ -f "$css_file" ]] || continue
		if ! grep -Fq 'KaiT2en GDM background' "$css_file"; then
			printf '\n/* KaiT2en GDM background */\n#lockDialogGroup, .login-dialog {\n  background-color: #000000;\n  background-image: none;\n}\n' >>"$css_file"
		fi
	done

	if ! glib-compile-resources "$manifest" \
			--sourcedir="$work" --target="$work/gnome-shell-theme.gresource"; then
		rm -rf "$work"
		return 1
	fi
	if ! install -o root -g root -m 0644 \
			"$work/gnome-shell-theme.gresource" "$theme_resource"; then
		rm -rf "$work"
		return 1
	fi
	rm -rf "$work"
}

info "installing KaiT2en GDM branding"
if ! install -d -o root -g root -m 0755 "${BACKGROUND_DST%/*}"; then
	warn "could not create the GDM background directory; leaving the existing login screen unchanged"
	exit 0
fi
if ! install -o root -g root -m 0644 "$LOGO_SRC" "$LOGO_DST"; then
	warn "could not install the GDM logo; leaving the existing login screen unchanged"
	exit 0
fi
if ! install -o root -g root -m 0644 "$BACKGROUND_SRC" "$BACKGROUND_DST"; then
	warn "could not install the GDM background; leaving the existing login screen unchanged"
	exit 0
fi
if ! install -o root -g root -m 0644 "$BRANDING_SRC" "$BRANDING_DST"; then
	warn "could not install the GDM settings; leaving the existing login screen unchanged"
	exit 0
fi
if ! dconf update; then
	warn "could not update the GDM dconf database; branding will not be active"
	exit 0
fi
if ! patch_gnome_shell_theme; then
	warn "could not patch the GNOME Shell login background; continuing"
	exit 0
fi

info "KaiT2en GDM branding installed"
