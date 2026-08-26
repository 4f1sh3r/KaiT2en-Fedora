#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
patch_dir=
disable_list=$repo_root/scripts/copr/configs/t2-disable.list
kernel_release=
buildid=
out_dir=$PWD/out
full_config=0

usage() {
	cat <<EOF
Usage: $0 --buildid SUFFIX [options]

Options:
  --kernel-release NVR.arch  Fedora kernel release (default: latest in repos)
  --buildid SUFFIX           Unique suffix after .kait2en (required)
  --patch-dir DIRECTORY      Experiment directory containing series (required)
  --out DIRECTORY            Output directory (default: ./out)
  --full-config              Do not append the T2 config reduction
EOF
	exit 2
}

while (($#)); do
	case $1 in
	--kernel-release)
		(($# >= 2)) || usage
		kernel_release=$2
		shift 2
		;;
	--buildid)
		(($# >= 2)) || usage
		buildid=$2
		shift 2
		;;
	--patch-dir)
		(($# >= 2)) || usage
		patch_dir=$2
		shift 2
		;;
	--out)
		(($# >= 2)) || usage
		out_dir=$2
		shift 2
		;;
	--full-config)
		full_config=1
		shift
		;;
	*) usage ;;
	esac
done

[[ $buildid =~ ^[A-Za-z0-9][A-Za-z0-9._]*$ ]] || {
	printf 'Build ID must contain only letters, numbers, dots, and underscores: %s\n' \
		"$buildid" >&2
	exit 2
}
[[ -n $patch_dir ]] || {
	printf '%s\n' 'A patch directory is required.' >&2
	exit 2
}
[[ -d $patch_dir ]] || {
	printf 'Patch directory does not exist: %s\n' "$patch_dir" >&2
	exit 1
}
[[ -s $patch_dir/series ]] || {
	printf 'Patch series does not exist or is empty: %s/series\n' \
		"$patch_dir" >&2
	exit 1
}
if ((!full_config)); then
	[[ -s $disable_list ]] || {
		printf 'T2 disable list does not exist or is empty: %s\n' "$disable_list" >&2
		exit 1
	}
fi

for command in awk dnf find grep install mktemp rpm rpmbuild sed sort; do
	command -v "$command" >/dev/null || {
		printf 'Missing command: %s\n' "$command" >&2
		exit 1
	}
done

mkdir -p "$out_dir"
out_dir=$(cd -- "$out_dir" && pwd -P)
patch_dir=$(cd -- "$patch_dir" && pwd -P)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
download_dir=$work/download
topdir=$work/rpmbuild
mkdir -p "$download_dir" \
	"$topdir/BUILD" "$topdir/BUILDROOT" "$topdir/RPMS" \
	"$topdir/SOURCES" "$topdir/SPECS" "$topdir/SRPMS"

kernel_query=kernel
if [[ -n $kernel_release ]]; then
	case $kernel_release in
	*.x86_64) kernel_query=kernel-${kernel_release%.x86_64} ;;
	*)
		printf 'Expected NVR.arch such as 7.1.9-200.fc44.x86_64: %s\n' \
			"$kernel_release" >&2
		exit 2
		;;
	esac
fi

printf 'Downloading Fedora source package for %s\n' "$kernel_query"
dnf download --source --destdir "$download_dir" "$kernel_query"
mapfile -t srpms < <(find "$download_dir" -maxdepth 1 -type f \
	-name 'kernel-*.src.rpm' -print | sort -V)
[[ ${#srpms[@]} -eq 1 ]] || {
	printf 'Expected exactly one kernel source RPM, found %d\n' "${#srpms[@]}" >&2
	printf '%s\n' "${srpms[@]}" >&2
	exit 1
}
source_rpm=${srpms[0]}

rpm -i --nodeps --define "_topdir $topdir" "$source_rpm"
spec=$topdir/SPECS/kernel.spec
sources=$topdir/SOURCES
[[ -s $spec ]] || {
	printf 'Kernel spec was not installed from %s\n' "$source_rpm" >&2
	exit 1
}

declare -A spec_anchors=(
	[linux-kernel-test-source]='^[[:space:]]*Patch999999:[[:space:]]*linux-kernel-test\.patch'
	[linux-kernel-test-apply]='ApplyOptionalPatch[[:space:]]+linux-kernel-test\.patch'
	[kernel-local-source]='^[[:space:]]*Source3001:[[:space:]]*kernel-local'
)
for anchor in "${!spec_anchors[@]}"; do
	grep -Eq "${spec_anchors[$anchor]}" "$spec" || {
		printf 'Fedora kernel spec no longer contains required anchor: %s\n' \
			"$anchor" >&2
		exit 1
	}
done
[[ -f $sources/linux-kernel-test.patch && -f $sources/kernel-local ]] || {
	printf 'Fedora kernel SRPM lacks linux-kernel-test.patch or kernel-local\n' >&2
	exit 1
}

declare -A listed_patches=()
patches=()
while IFS= read -r patch_name || [[ -n $patch_name ]]; do
	[[ -n $patch_name && $patch_name != \#* ]] || continue
	[[ $patch_name =~ ^[A-Za-z0-9][A-Za-z0-9._-]*\.patch$ ]] || {
		printf 'Invalid patch name in series: %s\n' "$patch_name" >&2
		exit 1
	}
	[[ -z ${listed_patches[$patch_name]+x} ]] || {
		printf 'Duplicate patch in series: %s\n' "$patch_name" >&2
		exit 1
	}
	[[ -f $patch_dir/$patch_name ]] || {
		printf 'Patch listed in series does not exist: %s\n' "$patch_name" >&2
		exit 1
	}
	listed_patches[$patch_name]=1
	patches+=("$patch_dir/$patch_name")
done <"$patch_dir/series"
((${#patches[@]} > 0)) || {
	printf 'Patch series contains no patches: %s/series\n' "$patch_dir" >&2
	exit 1
}
while IFS= read -r patch_file; do
	patch_name=${patch_file##*/}
	[[ -n ${listed_patches[$patch_name]+x} ]] || {
		printf 'Patch is not listed in series: %s\n' "$patch_name" >&2
		exit 1
	}
done < <(find "$patch_dir" -maxdepth 1 -type f -name '*.patch' \
	-print | LC_ALL=C sort)

: >"$sources/linux-kernel-test.patch"
for patch_file in "${patches[@]}"; do
	printf 'Adding %s\n' "${patch_file##*/}"
	cat "$patch_file" >>"$sources/linux-kernel-test.patch"
	printf '\n' >>"$sources/linux-kernel-test.patch"
done

if ((!full_config)); then
	{
		printf '\n# KaiT2en T2-only test-kernel reduction\n'
		while IFS= read -r symbol || [[ -n $symbol ]]; do
			[[ -n $symbol && $symbol != \#* ]] || continue
			[[ $symbol =~ ^CONFIG_[A-Z0-9_]+$ ]] || {
				printf 'Invalid config symbol: %s\n' "$symbol" >&2
				exit 1
			}
			printf '# %s is not set\n' "$symbol"
		done <"$disable_list"
	} >>"$sources/kernel-local"
fi

macro_file=$work/kernel-copr-macros
{
	printf '%%global buildid .kait2en.%s\n' "$buildid"
	for feature in debug debuginfo perf libperf tools doc headers cross_headers \
		selftests kabichk kernel_abi_stablelists configchecks; do
		printf '%%global _without_%s 1\n' "$feature"
	done
} >"$macro_file"
sed -i "1r $macro_file" "$spec"

printf 'Building patched kernel source RPM\n'
rpmbuild -bs --define "_topdir $topdir" "$spec"
mapfile -t built_srpms < <(find "$topdir/SRPMS" -maxdepth 1 -type f \
	-name 'kernel-*.src.rpm' -print | sort -V)
[[ ${#built_srpms[@]} -eq 1 ]] || {
	printf 'Expected exactly one built source RPM, found %d\n' \
		"${#built_srpms[@]}" >&2
	exit 1
}
built_srpm=${built_srpms[0]}
install -m 0644 "$built_srpm" "$out_dir/"
output_srpm=$out_dir/${built_srpm##*/}
nvr=$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$output_srpm")
kver=$(rpm -qp --qf '%{VERSION}-%{RELEASE}.x86_64' "$output_srpm")

printf 'srpm=%s\n' "$output_srpm"
printf 'srpm_name=%s\n' "${output_srpm##*/}"
printf 'nvr=%s\n' "$nvr"
printf 'kver=%s\n' "$kver"
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
	{
		printf 'srpm=%s\n' "$output_srpm"
		printf 'srpm_name=%s\n' "${output_srpm##*/}"
		printf 'nvr=%s\n' "$nvr"
		printf 'kver=%s\n' "$kver"
	} >>"$GITHUB_OUTPUT"
fi
