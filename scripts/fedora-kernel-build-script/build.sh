#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
PATCH_DIR=$ROOT/patches
BUILD_ROOT=${KERNEL_BUILD_ROOT:-$ROOT/build}
LOCALVERSION=-patched
CONFIG_FILE=
KERNEL_RELEASE=
JOBS=$(nproc)
CLEAN=0
LOCAL_INSTALL=0
DEFER_INSTALL=0
T2_CONFIG=0
PREPARE_ONLY=0
LOCALMODCONFIG=0
ALLOW_NO_PATCHES=0
BUILD_CONFIG_SCHEMA=6
ENABLE_CONFIGS=()
HAS_AMD_DGPU=0

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
  --defer-install        Build local targets without installing them
  --t2-config            Disable drivers not used by Apple T2 Macs
  --localmodconfig       Reduce the configuration to currently loaded modules
  --allow-no-patches     Permit an empty patch folder (configuration preview)
  --enable-config NAME   Enable an additional Kconfig symbol (repeatable)
  --prepare-only         Download, patch and configure without building
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
	--defer-install)
		DEFER_INSTALL=1
		shift
		;;
	--t2-config)
		T2_CONFIG=1
		shift
		;;
	--localmodconfig)
		LOCALMODCONFIG=1
		shift
		;;
	--allow-no-patches)
		ALLOW_NO_PATCHES=1
		shift
		;;
	--enable-config)
		[[ $# -ge 2 && $2 =~ ^[A-Z0-9_]+$ ]] || usage
		ENABLE_CONFIGS+=("$2")
		shift 2
		;;
	--prepare-only)
		PREPARE_ONLY=1
		shift
		;;
	--clean)
		CLEAN=1
		shift
		;;
	*) usage ;;
	esac
done

# T2 MacBook Pros with a discrete GPU use AMD graphics.  Keep this detection
# independent of loaded modules: localmodconfig may run while the dGPU is off.
if command -v lspci >/dev/null 2>&1 &&
		lspci -Dn 2>/dev/null | grep -Eqi ' (0300|0302): 1002:'; then
	HAS_AMD_DGPU=1
fi

[[ -d $PATCH_DIR ]] || {
	printf 'Patch directory does not exist: %s\n' "$PATCH_DIR" >&2
	exit 1
}
PATCH_DIR=$(cd -- "$PATCH_DIR" && pwd -P)

[[ $LOCALVERSION == -* ]] || {
	printf 'Local version must start with a hyphen: %s\n' "$LOCALVERSION" >&2
	exit 2
}

for command in cpio curl find gcc git install make nproc patch rpm2cpio sed sha256sum sort tar uname xz yes; do
	command -v "$command" >/dev/null || {
		printf 'Missing command: %s\n' "$command" >&2
		exit 1
	}
done
if ((LOCAL_INSTALL && !DEFER_INSTALL)); then
	command -v pkexec >/dev/null || {
		printf 'Missing command: pkexec\n' >&2
		exit 1
	}
	[[ -x /usr/local/libexec/t2-kernel-builder-cleanup ]] || {
		printf 'Kernel installation helper is not installed.\n' >&2
		exit 1
	}
fi

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
[[ $VERSION =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]] || {
	printf 'Expected a numeric kernel version, got: %s\n' "$VERSION" >&2
	exit 2
}
KERNEL_VERSION_MAJOR=${BASH_REMATCH[1]}
KERNEL_VERSION_PATCHLEVEL=${BASH_REMATCH[2]}
KERNEL_VERSION_SUBLEVEL=${BASH_REMATCH[3]}
KERNEL_LOCALVERSION=-${RELEASE}.x86_64${LOCALVERSION}

SRPM=kernel-$VERSION-$RELEASE.src.rpm
URL=https://kojipkgs.fedoraproject.org/packages/kernel/$VERSION/$RELEASE/src/$SRPM
WORK=$BUILD_ROOT/$KERNEL_RELEASE

if ((CLEAN)); then
	# Keep the verified SRPM and its extracted source payload. Recreate only the
	# derived patched/configured kernel tree when build inputs changed.
	rm -rf -- "$WORK/kernel"
	rm -f -- "$WORK/.prepared" "$WORK/input-hash" "$WORK/kernel-tree" \
		"$WORK/built-kernel-tree" "$WORK/built-kernel-release"
fi

mkdir -p "$WORK/download" "$WORK/sources" "$WORK/kernel"

shopt -s nullglob
PATCHES=("$PATCH_DIR"/*.patch)
shopt -u nullglob
if ((${#PATCHES[@]} == 0 && !ALLOW_NO_PATCHES)); then
	printf 'No patches found in %s\n' "$PATCH_DIR" >&2
	exit 1
fi

INPUT_HASH=$({
	printf '%s\0%s\0' "$KERNEL_RELEASE" "$KERNEL_LOCALVERSION"
	printf 'build-config-schema=%s\0' "$BUILD_CONFIG_SCHEMA"
	printf 'patch-count=%s\0' "${#PATCHES[@]}"
	if ((T2_CONFIG)); then
		printf 't2-config\0'
		printf 'amd-dgpu=%s\0' "$HAS_AMD_DGPU"
	fi
	if ((LOCALMODCONFIG)); then
		printf 'localmodconfig\0'
	fi
	printf 'enable-config=%s\0' "${ENABLE_CONFIGS[@]}"
	if ((${#PATCHES[@]})); then
		sha256sum "${PATCHES[@]}"
	fi
	if [[ -n $CONFIG_FILE ]]; then
		sha256sum "$CONFIG_FILE"
	fi
} | sha256sum | awk '{print $1}')

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
	(
	prepare_complete=0
	cleanup_incomplete_prepare() {
		local status=$?

		if ((prepare_complete == 0)); then
			rm -rf -- "$WORK/kernel"
			rm -f -- "$WORK/.prepared" "$WORK/input-hash" "$WORK/kernel-tree"
		fi
		exit "$status"
	}
	trap cleanup_incomplete_prepare EXIT
	trap 'exit 130' INT
	trap 'exit 143' TERM

	# A failed or interrupted earlier preparation may have left an extracted,
	# partially patched tree behind. Never apply Fedora's patch set twice.
	rm -rf -- "$WORK/kernel"
	mkdir -p "$WORK/kernel"
	printf 'Preparing Fedora kernel sources\n'
	tar --no-same-owner -xf "$TARBALL" -C "$WORK/kernel"
	TREE=$(find "$WORK/kernel" -mindepth 1 -maxdepth 1 -type d -name 'linux-*' -print -quit)
	[[ -n $TREE ]]
	git -C "$TREE" init -q
	# Fedora's generated downstream patch can contain intentional trailing
	# whitespace. Keep diagnostics enabled for user-supplied patches below.
	git -C "$TREE" apply --whitespace=nowarn "$REDHAT_PATCH"
	install -m 0644 "$WORK/sources/Makefile.rhelver" "$TREE/Makefile.rhelver"
	for patch_file in "${PATCHES[@]}"; do
		printf 'Applying %s\n' "${patch_file##*/}"
		git -C "$TREE" apply "$patch_file"
	done

	# Fedora merge-window rc0 tarballs still carry the previous release in the
	# upstream Makefile.  The RPM spec rewrites PATCHLEVEL before building; do
	# the equivalent for direct local builds and keep all three fields aligned
	# with the selected Fedora package version.
	sed -i \
		-e "s/^VERSION = .*/VERSION = $KERNEL_VERSION_MAJOR/" \
		-e "s/^PATCHLEVEL = .*/PATCHLEVEL = $KERNEL_VERSION_PATCHLEVEL/" \
		-e "s/^SUBLEVEL = .*/SUBLEVEL = $KERNEL_VERSION_SUBLEVEL/" \
		"$TREE/Makefile"

	if [[ -n $CONFIG_FILE ]]; then
		install -m 0644 "$CONFIG_FILE" "$TREE/.config"
	else
		install -m 0644 "$WORK/sources/kernel-x86_64-fedora.config" "$TREE/.config"
	fi

	"$TREE/scripts/config" --file "$TREE/.config" \
		--set-str LOCALVERSION "$KERNEL_LOCALVERSION" \
		--disable LOCALVERSION_AUTO \
		--set-str SYSTEM_TRUSTED_KEYS '' \
		--set-str SYSTEM_REVOCATION_KEYS ''

	if ((T2_CONFIG)); then
		"$TREE/scripts/config" --file "$TREE/.config" --disable \
			DRM_AMDGPU --disable DRM_NOUVEAU --disable DRM_RADEON --disable DRM_XE \
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
	if ((LOCALMODCONFIG)); then
		printf '[kait2en-progress] phase=localmodconfig\n'
		# Fedora's localmodconfig invokes oldconfig and otherwise waits forever
		# for answers when new symbols remain after module streamlining.
		set +o pipefail
		yes '' | make -C "$TREE" localmodconfig
		localmod_status=${PIPESTATUS[1]}
		set -o pipefail
		((localmod_status == 0)) || exit "$localmod_status"
	fi
	if ((T2_CONFIG)); then
		# t2hid is needed for the internal keyboard/trackpad even on models
		# without a Touch Bar.  The shared t2touchbar DKMS build also links its
		# keyboard module, which requires sparse-keymap symbols from the kernel.
		# USB4 must also survive localmodconfig: pcie_ports=compat can keep the
		# in-tree Thunderbolt driver unloaded while the profile is captured.
		"$TREE/scripts/config" --file "$TREE/.config" \
			--enable INPUT_SPARSEKMAP \
			--enable HOTPLUG_PCI \
			--enable HOTPLUG_PCI_PCIE

		if ((HAS_AMD_DGPU)); then
			# t2gmux replaces apple_gmux at runtime, but builds against helpers
			# exposed by apple-gmux.h only when CONFIG_APPLE_GMUX is configured.
			"$TREE/scripts/config" --file "$TREE/.config" \
				--enable BACKLIGHT_CLASS_DEVICE \
				--enable VGA_SWITCHEROO \
				--module APPLE_GMUX
		fi
	fi
	for symbol in "${ENABLE_CONFIGS[@]}"; do
		"$TREE/scripts/config" --file "$TREE/.config" --enable "$symbol"
	done
	if ((T2_CONFIG)); then
		# Keep the upstream drivers replaced by Kait2en as modules.  Their Kconfig
		# entries select infrastructure needed by the DKMS replacements, while
		# Kait2en's module_blacklist prevents the upstream modules from binding.
		# This must come after GUI overrides so none can accidentally become
		# built-in and bypass the module blacklist.
		for symbol in ACPI_TAD SENSORS_APPLESMC HID_APPLE HID_APPLETB_BL \
				HID_APPLETB_KBD HID_MAGICMOUSE DRM_APPLETBDRM \
				APPLE_MFI_FASTCHARGE USB4; do
			"$TREE/scripts/config" --file "$TREE/.config" --module "$symbol"
		done

		if ((HAS_AMD_DGPU)); then
			# The runtime-PM installer replaces these two modules with builds from
			# this tree.  Preserve their complete Kconfig dependency graph even if
			# localmodconfig ran while the discrete GPU was powered off.
			"$TREE/scripts/config" --file "$TREE/.config" \
				--module APPLE_GMUX \
				--module DRM_AMDGPU \
				--module SND_HDA_INTEL \
				--module SND_HDA_CODEC_HDMI
		fi
	fi
	make -C "$TREE" olddefconfig
	printf '%s\n' "$TREE" >"$WORK/kernel-tree"
	printf '%s\n' "$INPUT_HASH" >"$WORK/input-hash"
	touch "$WORK/.prepared"
	prepare_complete=1
	trap - EXIT INT TERM
	)
fi

TREE=$(<"$WORK/kernel-tree")

if ((PREPARE_ONLY)); then
	printf 'Prepared kernel tree: %s\n' "$TREE"
	printf 'Prepared kernel release: %s\n' "$(make -s -C "$TREE" kernelrelease)"
	exit 0
fi
install -m 0644 "$WORK/sources/Makefile.rhelver" "$TREE/Makefile.rhelver"
printf 'Building %s%s with %s jobs\n' "$VERSION" "$KERNEL_LOCALVERSION" "$JOBS"

if ((LOCAL_INSTALL)); then
	BUILD_TARGETS=(bzImage modules)
	BUILD_OPTIONS=()
else
	BUILD_TARGETS=(binrpm-pkg)
	BUILD_OPTIONS=(RPMOPTS=--nodeps)
fi

if ((LOCAL_INSTALL)); then
	make -C "$TREE" -j"$JOBS" "${BUILD_TARGETS[@]}"
	KERNELRELEASE=$(make -s -C "$TREE" kernelrelease)
	if ((DEFER_INSTALL)); then
		printf '%s\n' "$TREE" >"$WORK/built-kernel-tree"
		printf '%s\n' "$KERNELRELEASE" >"$WORK/built-kernel-release"
		printf 'Built kernel tree: %s\n' "$TREE"
		printf 'Built kernel release: %s\n' "$KERNELRELEASE"
		exit 0
	fi
	printf '[kait2en-progress] phase=installing\n'
	printf 'Installing %s locally\n' "$KERNELRELEASE"
	pkexec /usr/local/libexec/t2-kernel-builder-cleanup install-kernel "$TREE" "$KERNELRELEASE"
	printf '\nInstalled kernel release: %s\n' "$KERNELRELEASE"
	exit 0
fi

make -C "$TREE" -j"$JOBS" "${BUILD_OPTIONS[@]}" "${BUILD_TARGETS[@]}"

printf '\nBuilt RPMs:\n'
find "$TREE/rpmbuild/RPMS" -type f -name '*.rpm' -print | sort
