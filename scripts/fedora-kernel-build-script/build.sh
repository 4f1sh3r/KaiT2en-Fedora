#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
PATCH_DIR=$ROOT/patches
BUILD_ROOT=$ROOT/build
LOCALVERSION=-patched
CONFIG_FILE=
KERNEL_RELEASE=
JOBS=$(nproc)
CLEAN=0
LOCAL_INSTALL=0
T2_CONFIG=0

usage() {
	cat <<EOF
Usage: $0 [KERNEL_RELEASE] [options]

Without KERNEL_RELEASE, an interactive list is loaded from Fedora Koji.

Options:
  --config FILE          Use this kernel configuration
  --patch-dir DIRECTORY  Apply patches from this directory (default: patches/)
  --localversion SUFFIX  Set a unique suffix (default: -patched)
  --jobs NUMBER          Parallel build jobs (default: $JOBS)
  --local-install        Build and install directly instead of creating RPMs
  --t2-config            Disable drivers not used by Apple T2 Macs
  --clean                Remove this version's existing build first
EOF
	exit 2
}

if [[ $# -gt 0 && $1 != --* ]]; then
	KERNEL_RELEASE=$1
	shift
fi

while [[ $# -gt 0 ]]; do
	case $1 in
	--config)
		[[ $# -ge 2 ]] || usage
		CONFIG_FILE=$2
		shift 2
		;;
	--patch-dir)
		[[ $# -ge 2 ]] || usage
		PATCH_DIR=$2
		shift 2
		;;
	--localversion)
		[[ $# -ge 2 ]] || usage
		LOCALVERSION=$2
		shift 2
		;;
	--jobs)
		[[ $# -ge 2 && $2 =~ ^[1-9][0-9]*$ ]] || usage
		JOBS=$2
		shift 2
		;;
	--local-install)
		LOCAL_INSTALL=1
		shift
		;;
	--t2-config)
		T2_CONFIG=1
		shift
		;;
	--clean)
		CLEAN=1
		shift
		;;
	*) usage ;;
	esac
done

[[ -d $PATCH_DIR ]] || {
	printf 'Patch directory does not exist: %s\n' "$PATCH_DIR" >&2
	exit 1
}
PATCH_DIR=$(cd -- "$PATCH_DIR" && pwd -P)

[[ $LOCALVERSION == -* ]] || {
	printf 'Local version must start with a hyphen: %s\n' "$LOCALVERSION" >&2
	exit 2
}

for command in cpio curl find gcc git install make nproc patch rpm2cpio sed sha256sum sort tar uname xz; do
	command -v "$command" >/dev/null || {
		printf 'Missing command: %s\n' "$command" >&2
		exit 1
	}
done

select_kernel_release() {
	local fedora_release version release choice
	local -a versions releases candidates

	[[ -t 0 ]] || {
		printf 'KERNEL_RELEASE is required when standard input is not interactive.\n' >&2
		exit 2
	}

	# shellcheck disable=SC1091
	source /etc/os-release
	fedora_release=${VERSION_ID%%.*}

	mapfile -t versions < <(
		curl -fsSL https://kojipkgs.fedoraproject.org/packages/kernel/ |
			sed -n 's/.*href="\([0-9][^"]*\)\/".*/\1/p' |
			sort -V |
			tail -n 10
	)

	for version in "${versions[@]}"; do
		mapfile -t releases < <(
			curl -fsSL "https://kojipkgs.fedoraproject.org/packages/kernel/$version/" |
				sed -n 's/.*href="\([^"]*\.fc[0-9][0-9]*\)\/".*/\1/p' |
				sort -V
		)
		release=
		for choice in "${releases[@]}"; do
			release=$choice
		done
		[[ -n $release ]] && candidates+=("$version-$release.x86_64")
	done

	((${#candidates[@]})) || {
		printf 'No kernel builds found on Koji.\n' >&2
		exit 1
	}

	printf 'Available kernels:\n'
	for choice in "${!candidates[@]}"; do
		printf '  %d) %s\n' "$((choice + 1))" "${candidates[$choice]}"
	done
	printf 'Select kernel: '
	read -r choice
	[[ $choice =~ ^[1-9][0-9]*$ && choice -le ${#candidates[@]} ]] || {
		printf 'Invalid selection.\n' >&2
		exit 2
	}
	KERNEL_RELEASE=${candidates[$((choice - 1))]}
}
if [[ -z $KERNEL_RELEASE ]]; then
	select_kernel_release
fi

case $KERNEL_RELEASE in
*.x86_64) PACKAGE_RELEASE=${KERNEL_RELEASE%.x86_64} ;;
*)
	printf 'Expected a Fedora x86_64 release such as 7.1.7-200.fc44.x86_64\n' >&2
	exit 2
	;;
esac

VERSION=${PACKAGE_RELEASE%%-*}
RELEASE=${PACKAGE_RELEASE#*-}
[[ -n $VERSION && -n $RELEASE && $VERSION != "$RELEASE" ]] || usage

SRPM=kernel-$VERSION-$RELEASE.src.rpm
URL=https://kojipkgs.fedoraproject.org/packages/kernel/$VERSION/$RELEASE/src/$SRPM
WORK=$BUILD_ROOT/$KERNEL_RELEASE

if ((CLEAN)); then
	rm -rf -- "$WORK"
fi

mkdir -p "$WORK/download" "$WORK/sources" "$WORK/kernel"

shopt -s nullglob
PATCHES=("$PATCH_DIR"/*.patch)
shopt -u nullglob
if ((${#PATCHES[@]} == 0)); then
	printf 'No patches found in %s\n' "$PATCH_DIR" >&2
	exit 1
fi

INPUT_HASH=$(
	printf '%s\0%s\0' "$KERNEL_RELEASE" "$LOCALVERSION"
	if ((T2_CONFIG)); then
		printf 't2-config\0'
	fi
	sha256sum "${PATCHES[@]}"
	if [[ -n $CONFIG_FILE ]]; then
		sha256sum "$CONFIG_FILE"
	fi
)
INPUT_HASH=$(printf '%s' "$INPUT_HASH" | sha256sum | awk '{print $1}')

if [[ -f $WORK/input-hash && $(<"$WORK/input-hash") != "$INPUT_HASH" ]]; then
	printf 'Build inputs changed. Re-run with --clean.\n' >&2
	exit 1
fi

if [[ ! -s $WORK/download/$SRPM ]]; then
	printf 'Downloading %s\n' "$SRPM"
	curl --fail --location --continue-at - --output "$WORK/download/$SRPM" "$URL"
fi

if [[ ! -f $WORK/.extracted ]]; then
	printf 'Extracting Fedora source package\n'
	(
		cd "$WORK/sources"
		rpm2cpio "$WORK/download/$SRPM" | cpio -idm --quiet
	)
	touch "$WORK/.extracted"
fi

TARBALL=$(find "$WORK/sources" -maxdepth 1 -name 'linux-*.tar.xz' -print -quit)
REDHAT_PATCH=$(find "$WORK/sources" -maxdepth 1 -name 'patch-*-redhat.patch' -print -quit)
[[ -n $TARBALL && -n $REDHAT_PATCH ]] || {
	printf 'Incomplete Fedora source package\n' >&2
	exit 1
}

if [[ ! -f $WORK/.prepared ]]; then
	printf 'Preparing Fedora kernel sources\n'
	tar --no-same-owner -xf "$TARBALL" -C "$WORK/kernel"
	TREE=$(find "$WORK/kernel" -mindepth 1 -maxdepth 1 -type d -name 'linux-*' -print -quit)
	[[ -n $TREE ]]
	git -C "$TREE" init -q
	git -C "$TREE" apply "$REDHAT_PATCH"
	install -m 0644 "$WORK/sources/Makefile.rhelver" "$TREE/Makefile.rhelver"
	for patch_file in "${PATCHES[@]}"; do
		printf 'Applying %s\n' "${patch_file##*/}"
		git -C "$TREE" apply "$patch_file"
	done

	if [[ -n $CONFIG_FILE ]]; then
		install -m 0644 "$CONFIG_FILE" "$TREE/.config"
	else
		install -m 0644 "$WORK/sources/kernel-x86_64-fedora.config" "$TREE/.config"
	fi

	"$TREE/scripts/config" --file "$TREE/.config" \
		--set-str LOCALVERSION "$LOCALVERSION" \
		--disable LOCALVERSION_AUTO \
		--set-str SYSTEM_TRUSTED_KEYS '' \
		--set-str SYSTEM_REVOCATION_KEYS ''

		if ((T2_CONFIG)); then
		"$TREE/scripts/config" --file "$TREE/.config" --disable \
			DRM_NOUVEAU --disable DRM_RADEON --disable DRM_XE \
			--disable CHROME_PLATFORMS --disable SURFACE_PLATFORMS \
			--disable ACER_WMI --disable ASUS_WMI --disable ASUS_NB_WMI \
			--disable DELL_LAPTOP --disable DELL_WMI \
			--disable DELL_WMI_AIO --disable DELL_WMI_DESCRIPTOR \
			--disable FUJITSU_LAPTOP --disable FUJITSU_TABLET \
			--disable GIGABYTE_WMI --disable HP_WMI \
			--disable HUAWEI_WMI --disable IDEAPAD_LAPTOP \
			--disable LG_LAPTOP --disable MSI_LAPTOP --disable MSI_WMI \
			--disable PANASONIC_LAPTOP --disable SAMSUNG_LAPTOP \
			--disable SONY_LAPTOP --disable THINKPAD_ACPI \
			--disable TOSHIBA_ACPI --disable XIAOMI_WMI \
			--disable COMPAL_LAPTOP --disable EEEPC_LAPTOP --disable EEEPC_WMI \
			--disable TOPSTAR_LAPTOP --disable PEAQ_WMI --disable MXM_WMI \
			--disable WINMATE_FM07_KEYS --disable BARCO_P50_GPIO \
			--disable PCENGINES_APU2 --disable THINKPAD_LMI \
			--disable YOGABOOK --disable YOGABOOK_WMI \
			--disable DRM_VMWGFX --disable DRM_VIRTIO_GPU --disable DRM_QXL \
			--disable DRM_BOCHS --disable DRM_CIRRUS_QEMU --disable DRM_HYPERV \
			--disable DRM_VBOXVIDEO --disable DRM_GMA500 \
			--disable DRM_MGAG200 --disable DRM_AST \
			--disable DRM_ARMADA --disable DRM_EXYNOS --disable DRM_ROCKCHIP \
			--disable DRM_MEDIATEK --disable DRM_MSM --disable DRM_TEGRA \
			--disable DRM_HISI_HIBMC --disable DRM_LOONGSON \
			--disable DRM_VC4 --disable DRM_V3D --disable DRM_ETNAVIV \
			--disable DRM_PANFROST --disable DRM_LIMA \
			--disable CPU_SUP_CENTAUR --disable CPU_SUP_ZHAOXIN --disable CPU_SUP_HYGON \
			--disable PATA_ALI --disable PATA_VIA --disable PATA_SIS \
			--disable PATA_AMD --disable PATA_ATIIXP --disable PATA_JMICRON
	fi
	make -C "$TREE" olddefconfig
	printf '%s\n' "$TREE" >"$WORK/kernel-tree"
	printf '%s\n' "$INPUT_HASH" >"$WORK/input-hash"
	touch "$WORK/.prepared"
fi

TREE=$(<"$WORK/kernel-tree")
install -m 0644 "$WORK/sources/Makefile.rhelver" "$TREE/Makefile.rhelver"
printf 'Building %s%s with %s jobs\n' "$VERSION" "$LOCALVERSION" "$JOBS"

if ((LOCAL_INSTALL)); then
	sudo -v
	(
		while sleep 60; do
			sudo -n -v || exit
		done
	) &
	SUDO_KEEPALIVE_PID=$!
	trap 'kill "$SUDO_KEEPALIVE_PID" 2>/dev/null || true; wait "$SUDO_KEEPALIVE_PID" 2>/dev/null || true' EXIT

	make -C "$TREE" -j"$JOBS" bzImage modules
	KERNELRELEASE=$(make -s -C "$TREE" kernelrelease)
	printf 'Installing %s locally\n' "$KERNELRELEASE"
	sudo make -C "$TREE" INSTALL_MOD_STRIP=1 modules_install
	sudo make -C "$TREE" install
	printf '\nInstalled kernel release: %s\n' "$KERNELRELEASE"
	exit 0
fi

make -C "$TREE" -j"$JOBS" RPMOPTS=--nodeps binrpm-pkg

printf '\nBuilt RPMs:\n'
find "$TREE/rpmbuild/RPMS" -type f -name '*.rpm' -print | sort
