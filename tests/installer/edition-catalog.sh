#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
# The spin exclusion below is a `! rg ...` assertion, which a missing ripgrep
# would satisfy without reading the catalog. This check runs as its own CI step,
# so it cannot rely on the static checks having looked first.
if ! command -v rg >/dev/null 2>&1; then
	printf 'these checks require rg, from the ripgrep package\n' >&2
	exit 1
fi
target_name=${1:-fedora-44}
target="$repo_root/packaging/installer/targets/$target_name.conf"
[[ -f "$target" ]]
# shellcheck disable=SC1090
source "$target"
catalog="$repo_root/packaging/installer/targets/$EDITIONS_FILE"
[[ -s "$catalog" ]]

count=0
seen=" "
default_found=0
while IFS=$'\t' read -r id display variant subvariant filename url size sha; do
	[[ "$id" =~ ^[a-z0-9][a-z0-9-]*$ ]]
	[[ "$seen" != *" $id "* ]]
	seen="$seen$id "
	[[ -n "$display" && -n "$subvariant" ]]
	case "$variant" in Workstation|KDE|Spins) ;; *) exit 1 ;; esac
	[[ "$filename" == *-Live-"$FEDORA_RELEASE"-*.x86_64.iso ]]
	[[ "$url" == https://download.fedoraproject.org/*/"$filename" ]]
	[[ "$size" =~ ^[0-9]+$ && "$sha" =~ ^[0-9a-f]{64}$ ]]
	[[ "$id" != "$DEFAULT_EDITION" ]] || default_found=1
	count=$((count + 1))

	if [[ -n ${FEDORA_RELEASES_JSON:-} ]]; then
		command -v jq >/dev/null
		jq -e \
			--arg version "$FEDORA_RELEASE" \
			--arg variant "$variant" \
			--arg subvariant "$subvariant" \
			--arg url "$url" \
			--arg size "$size" \
			--arg sha "$sha" \
			'.[] | select(
				.version == $version and .arch == "x86_64" and
				.variant == $variant and .subvariant == $subvariant and
				.link == $url and (.size | tostring) == $size and
				.sha256 == $sha
			)' "$FEDORA_RELEASES_JSON" >/dev/null
	fi
done <"$catalog"

((count == 3))
((default_found == 1))
[[ "$seen" == ' workstation kde cosmic ' ]]
! rg -n 'Atomic|ostree|Labs|Server|netinst|IoT' "$catalog"
printf 'Validated %d classic Fedora desktop editions for Fedora %s.\n' \
	"$count" "$FEDORA_RELEASE"
