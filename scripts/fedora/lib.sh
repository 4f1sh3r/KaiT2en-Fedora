#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"

info() {
	printf '[kait2en] %s\n' "$*"
}

warn() {
	printf '[kait2en] warning: %s\n' "$*" >&2
}

fail() {
	printf '[kait2en] error: %s\n' "$*" >&2
	exit 1
}

require_root() {
	[[ ${EUID:-$(id -u)} -eq 0 ]] || fail "run this script with sudo"
}

require_repo_root() {
	[[ -d "$REPO_ROOT/modules" && -d "$REPO_ROOT/apps" ]] ||
		fail "repository layout is incomplete"
}

require_fedora() {
	[[ -r /etc/os-release ]] || fail "/etc/os-release is missing"
	# shellcheck disable=SC1091
	. /etc/os-release
	[[ ${ID:-} == fedora || " ${ID_LIKE:-} " == *" fedora "* ]] ||
		fail "this script is Fedora-only"
}

require_command() {
	local cmd
	for cmd in "$@"; do
		command -v "$cmd" >/dev/null 2>&1 || fail "missing command: $cmd"
	done
}

install_kait2en_fonts() {
	local font_source="$REPO_ROOT/assets/fonts"
	local font_dir=/usr/local/share/fonts/kait2en
	local license_dir=/usr/local/share/licenses/kait2en-fonts

	[[ -f "$font_source/JetBrainsMono-Regular.ttf" &&
		-f "$font_source/JetBrainsMono-Medium.ttf" &&
		-f "$font_source/OFL.txt" ]] || fail "bundled JetBrains Mono files are missing"
	install -d -o root -g root -m 0755 "$font_dir" "$license_dir"
	install -o root -g root -m 0644 \
		"$font_source/JetBrainsMono-Regular.ttf" \
		"$font_source/JetBrainsMono-Medium.ttf" \
		"$font_dir/"
	install -o root -g root -m 0644 "$font_source/OFL.txt" "$license_dir/OFL.txt"
	if command -v fc-cache >/dev/null 2>&1; then
		fc-cache -f "$font_dir" || warn "unable to refresh the font cache; continuing"
	else
		warn "fc-cache is unavailable; JetBrains Mono will appear after the next font-cache refresh"
	fi
}

kernel_release() {
	printf '%s\n' "${KERNEL_RELEASE:-$(uname -r)}"
}

require_kernel_headers() {
	local release
	# install_module calls dkms without -k, so the build always targets the
	# running kernel whatever KERNEL_RELEASE says.
	release="$(uname -r)"

	[[ -d "/lib/modules/$release/build" ]] ||
		fail "kernel-devel-$release is missing; run install-dependencies.sh first"
}

require_min_kernel() {
	local min_major=$1 min_minor=$2 release major minor

	release="$(kernel_release)"
	if [[ ! "$release" =~ ^([0-9]+)\.([0-9]+) ]]; then
		fail "unable to determine Linux kernel version from: $release"
	fi

	major="${BASH_REMATCH[1]}"
	minor="${BASH_REMATCH[2]}"

	if (( major < min_major || (major == min_major && minor < min_minor) )); then
		fail "KaiT2en requires Linux kernel ${min_major}.${min_minor} or newer. Update Fedora first, reboot into the updated kernel, then run this installer again. Current kernel: $release"
	fi
}
