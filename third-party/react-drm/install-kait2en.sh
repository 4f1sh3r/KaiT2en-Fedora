#!/usr/bin/env bash
#
# react-drm installer — KaiT2en (T2 Fedora fork) profile.
#
# The KaiT2en-Fedora distro ships its own installer under
# scripts/fedora/install-apps.sh (this checkout lives inside that tree). This
# wrapper delegates to it, exactly as the KaiT2en distro depends on. It does not
# use this repo's standalone install.sh.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
KAIT2EN_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
INSTALLER="$KAIT2EN_ROOT/scripts/fedora/install-apps.sh"

case "${1:-}" in
	""|install) ;;
	*)
		printf 'usage: %s [install]\n' "${0##*/}" >&2
		exit 2
		;;
esac

if [[ ! -x "$INSTALLER" ]]; then
	printf 'react-drm: KaiT2en installer not found: %s\n' "$INSTALLER" >&2
	exit 1
fi

if (( EUID == 0 )); then
	exec "$INSTALLER" --react-drm-only
fi

command -v sudo >/dev/null 2>&1 || {
	printf 'react-drm: sudo is required\n' >&2
	exit 1
}
exec sudo "$INSTALLER" --react-drm-only
