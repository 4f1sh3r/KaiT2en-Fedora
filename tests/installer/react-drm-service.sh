#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
# shellcheck source=../../scripts/fedora/lib.sh
source "$repo_root/scripts/fedora/lib.sh"

mock_states=()
mock_index=0

mock_systemctl() {
	local state
	[[ "$*" == "is-active --quiet react-drm.service" ]] || {
		printf 'unexpected argv: %s\n' "$*" >&2
		exit 1
	}
	state=${mock_states[$mock_index]:-inactive}
	mock_index=$((mock_index + 1))
	[[ "$state" == active ]]
}

run_success_case() {
	local expected_checks=$1
	shift
	mock_states=("$@")
	mock_index=0
	wait_for_stable_service react-drm.service 10 3 0 mock_systemctl
	[[ $mock_index -eq $expected_checks ]]
}

run_success_case 4 inactive active active active
run_success_case 6 active active inactive active active active

mock_states=(active inactive active inactive active)
mock_index=0
if wait_for_stable_service react-drm.service 5 2 0 mock_systemctl; then
	printf 'unstable service was unexpectedly accepted\n' >&2
	exit 1
fi
[[ $mock_index -eq 5 ]]
