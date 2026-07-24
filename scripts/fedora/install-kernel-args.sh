#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command grubby grep sed

has_intel_pci_device() {
	local device path wanted

	for path in /sys/bus/pci/devices/*; do
		[[ -r "$path/vendor" && -r "$path/device" ]] || continue
		[[ $(<"$path/vendor") == 0x8086 ]] || continue
		device="$(<"$path/device")"
		for wanted in "$@"; do
			[[ "$device" == "$wanted" ]] && return 0
		done
	done

	return 1
}

repair_empty_grub_cmdline() {
	local arg cmdline escaped
	local -a args kept_args=()

	[[ -f /etc/default/grub ]] || return 0
	grep -qx 'GRUB_CMDLINE_LINUX=""' /etc/default/grub || return 0
	[[ -r /etc/kernel/cmdline ]] ||
		fail "cannot repair empty GRUB_CMDLINE_LINUX without /etc/kernel/cmdline"

	IFS= read -r cmdline </etc/kernel/cmdline
	read -r -a args <<<"$cmdline"
	for arg in "${args[@]}"; do
		case "$arg" in
			BOOT_IMAGE=*|root=*|rootflags=*|ro)
				continue
				;;
		esac
		kept_args+=("$arg")
	done

	((${#kept_args[@]})) ||
		fail "cannot repair GRUB_CMDLINE_LINUX from an empty kernel command line"

	cmdline="${kept_args[*]}"
	escaped="${cmdline//\\/\\\\}"
	escaped="${escaped//&/\\&}"
	escaped="${escaped//|/\\|}"
	sed -i \
		"s|^GRUB_CMDLINE_LINUX=\"\"$|GRUB_CMDLINE_LINUX=\"$escaped\"|" \
		/etc/default/grub
	grep -q '^GRUB_CMDLINE_LINUX="[^"]' /etc/default/grub ||
		fail "failed to repair GRUB_CMDLINE_LINUX"
	info "restored GRUB_CMDLINE_LINUX from the installed kernel command line"
}

# Clean up both quoted forms previously installed by KaiT2en.
REMOVE_ARGS=(
	"'acpi_osi=Windows 2012'"
	"acpi_osi='Windows 2012'"
	intel_iommu
	iommu
	pm_async
	pcie_aspm
	pcie_aspm.policy
	nvme_core.default_ps_max_latency_us
	apple_gmux.force_igd
	t2gmux.force_igd
	mem_sleep_default
	initcall_blacklist
	module_blacklist
)

ADD_ARGS=(
	"intel_iommu=on"
	"iommu=pt"
	"pm_async=off"
	"mem_sleep_default=deep"
	"initcall_blacklist=cmos_init,magicmouse_driver_init"
)

MODULE_BLACKLIST="module_blacklist=acpi_tad,applesmc,macsmc,hid_apple,hid_appletb_bl,hid_appletb_kbd,hid_magicmouse,appletbdrm,apple_bce,apple_mfi_fastcharge,apple_gmux"

ADD_ARGS+=("$MODULE_BLACKLIST")

if has_intel_pci_device 0x15e8 0x15eb; then
	info "Titan Ridge detected; removing obsolete ACPI OSI and PCIe port overrides"
	REMOVE_ARGS+=(acpi_osi pcie_ports)
elif has_intel_pci_device 0x8a0d 0x8a17; then
	info "Ice Lake Thunderbolt detected; removing obsolete ACPI OSI overrides"
	REMOVE_ARGS+=(acpi_osi)
else
	warn "unknown Thunderbolt generation; leaving ACPI OSI and PCIe port arguments unchanged"
fi

KERNEL_ARGS="${ADD_ARGS[*]}"
OLD_KERNEL_ARGS="${REMOVE_ARGS[*]}"

info "updating Kait2en kernel arguments and driver blacklist"
grubby --update-kernel=ALL \
	--remove-args="$OLD_KERNEL_ARGS" \
	--args="$KERNEL_ARGS"
repair_empty_grub_cmdline

info "current default kernel arguments:"
grubby --info=DEFAULT | sed -n 's/^args=//p'
