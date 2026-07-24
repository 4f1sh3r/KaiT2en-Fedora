# Installing the Broadcom firmware on Linux

Previous: [Prepare macOS and the Fedora installer](01-prepare-macos-and-fedora-usb.md) | Next: [Install KAIT2EN modules and apps](03-install-kait2en-modules-and-apps.md)

This guide installs the Broadcom Wi-Fi firmware copied from macOS. Bluetooth
firmware is discussed separately below because T2 Macs use different Bluetooth
architectures and there is currently no generally valid firmware installation
procedure for them.

## Requirements

Boot Linux once without the Broadcom Wi-Fi firmware installed. The kernel has to
try loading the missing firmware so the expected Linux filename appears in the
system log.

Check this with:

```bash
journalctl -b -k --grep='Direct firmware load for brcm/brcmfmac'
```

Expected output is similar to this:

```text
kernel: brcmfmac 0000:03:00.0: Direct firmware load for brcm/brcmfmac4364b2-pcie.apple,kauai-HRPN-u-7.5-X3.bin failed with error -2
kernel: brcmfmac 0000:03:00.0: Direct firmware load for brcm/brcmfmac4364b2-pcie.apple,kauai-HRPN-u-7.5.bin failed with error -2
kernel: brcmfmac 0000:03:00.0: Direct firmware load for brcm/brcmfmac4364b2-pcie.apple,kauai-HRPN-u.bin failed with error -2
kernel: brcmfmac 0000:03:00.0: Direct firmware load for brcm/brcmfmac4364b2-pcie.apple,kauai-HRPN.bin failed with error -2
kernel: brcmfmac 0000:03:00.0: Direct firmware load for brcm/brcmfmac4364b2-pcie.apple,kauai-X3.bin failed with error -2
```

The first filename is the best match requested by the kernel. The script below
uses that filename for the main firmware file.

## Install

Copy the firmware files from macOS to the `firmware` folder in this repository.
In Linux, open a terminal in the root folder of the KAIT2EN repository on your
USB drive.

The `firmware` folder should contain one file of each type:

```text
*.trx
*.clmb
*.txcb
P-*.txt
```

Then copy, paste and run:

```bash
bash <(cat << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

source_dir="firmware"

die() {
    echo "Error: $*" >&2
    exit 1
}

pick_one() {
    local label="$1"
    shift
    local files=("$@")

    if (( ${#files[@]} == 0 )); then
        die "No ${label} file found in ${source_dir}."
    fi

    if (( ${#files[@]} > 1 )); then
        echo "Found more than one ${label} file:"
        printf '  %s\n' "${files[@]}"
        die "Keep only the matching file in ${source_dir} and run the script again."
    fi

    printf '%s\n' "${files[0]}"
}

if [[ ! -d "${source_dir}" ]]; then
    die "Run this script from the root of the KaiT2en repository."
fi

trx_file="$(pick_one '.trx firmware' "${source_dir}"/*.trx)"
clm_file="$(pick_one '.clmb regulatory blob' "${source_dir}"/*.clmb)"
txcap_file="$(pick_one '.txcb TxCap blob' "${source_dir}"/*.txcb)"
nvram_file="$(pick_one 'P-*.txt NVRAM file' "${source_dir}"/P-*.txt)"

requested_bin="$(
    journalctl -b -k --no-pager |
    sed -n 's/.*Direct firmware load for \(brcm\/brcmfmac[^ ]*\.bin\) failed with error -2.*/\1/p' |
    head -n 1
)"

if [[ -z "${requested_bin}" ]]; then
    die "No missing brcmfmac .bin firmware request found in the current boot log."
fi

target_bin="${requested_bin#brcm/}"

if [[ "${requested_bin}" == "${target_bin}" ]]; then
    die "Unexpected firmware path in journal: ${requested_bin}"
fi

if [[ "${target_bin}" != brcmfmac*-pcie.apple,* ]]; then
    die "Unexpected brcmfmac filename: ${target_bin}"
fi

target_prefix="${target_bin%%.apple,*}"

clm_stem="$(basename "${clm_file}" .clmb)"
txcap_stem="$(basename "${txcap_file}" .txcb)"
nvram_name="$(basename "${nvram_file}")"

if [[ "${clm_stem}" != "${txcap_stem}" ]]; then
    die ".clmb and .txcb files do not use the same board name."
fi

if [[ "${nvram_name}" =~ ^P-([^_]+)_M-([^_]+)_V-([^_]+)__m-(.+)\.txt$ ]]; then
    nvram_board="${BASH_REMATCH[1]}"
    nvram_module="${BASH_REMATCH[2]}"
    nvram_vendor="${BASH_REMATCH[3]}"
    nvram_version="${BASH_REMATCH[4]}"
else
    die "Unexpected NVRAM filename: ${nvram_name}"
fi

if [[ "${nvram_board}" != "${clm_stem}" ]]; then
    die "NVRAM board name does not match .clmb/.txcb board name."
fi

target_clm="${target_prefix}.apple,${clm_stem}.clm_blob"
target_txcap="${target_prefix}.apple,${txcap_stem}.txcap_blob"
target_nvram="${target_prefix}.apple,${nvram_board}-${nvram_module}-${nvram_vendor}-${nvram_version}.txt"

dest_dir="/lib/firmware/brcm"

echo "Found source files:"
echo "  Firmware: ${trx_file}"
echo "  CLM:      ${clm_file}"
echo "  TxCap:    ${txcap_file}"
echo "  NVRAM:    ${nvram_file}"
echo
echo "Kernel requested:"
echo "  ${requested_bin}"
echo
echo "Will install:"
echo "  ${trx_file} -> ${dest_dir}/${target_bin}"
echo "  ${clm_file} -> ${dest_dir}/${target_clm}"
echo "  ${txcap_file} -> ${dest_dir}/${target_txcap}"
echo "  ${nvram_file} -> ${dest_dir}/${target_nvram}"
echo

read -r -p "Continue? [y/N] " answer
case "${answer}" in
    y|Y) ;;
    *) echo "Aborted."; exit 0 ;;
esac

sudo install -d -m 0755 "${dest_dir}"
sudo install -m 0644 "${trx_file}" "${dest_dir}/${target_bin}"
sudo install -m 0644 "${clm_file}" "${dest_dir}/${target_clm}"
sudo install -m 0644 "${txcap_file}" "${dest_dir}/${target_txcap}"
sudo install -m 0644 "${nvram_file}" "${dest_dir}/${target_nvram}"

echo
echo "Done. Reboot and check Wi-Fi."
EOF
)
```

After reboot, check that the firmware loaded:

```bash
journalctl -b -k --grep='brcmfmac'
```

Successful output should include a firmware version and should not include
missing `brcmfmac` firmware errors. Wi-Fi should work now.

## Bluetooth firmware

Do not install a generic `/lib/firmware/brcm/BCM.hcd`.

Some T2 Macs expose Bluetooth as a separate Broadcom UART controller. On the
tested `BCM2E7C` device Linux identifies the controller as BCM4364B0 and may log:

```text
Bluetooth: hci0: BCM: firmware Patch file not found, tried:
Bluetooth: hci0: BCM: 'brcm/BCM.hcd'
```

This message alone does not prove that firmware is required. Apple's Boot Camp
driver associates `BCM2E7C` with its UART driver but does not install an HCD RAM
patch for that ACPI device. The controller can also reach the normal Bluetooth
management interface without `BCM.hcd`. Installing a MiniDriver extracted from
macOS as this generic file has caused controller errors and power-management
problems in testing.

Other T2 Macs use BCM4377. Wi-Fi and Bluetooth are part of the same chip but
appear as separate PCIe functions with separate firmware. Bluetooth is handled
by `hci_bcm4377` on PCI device `14e4:5fa0`; Wi-Fi uses `brcmfmac` on
`14e4:4488`. The Bluetooth function uses board-specific `.bin` and `.ptb`
firmware and does not use the UART `.hcd` path.

BCM4377 currently has a separate suspend problem: the device can prevent
suspend unless `hci_bcm4377`, `brcmfmac_wcc` and `brcmfmac` are unloaded before
suspend and loaded again after resume. KAIT2EN handles that workaround in its
suspend service. This behaviour does not establish that an additional
Bluetooth firmware blob is missing.

The exact controller and driver can be checked with:

```bash
journalctl -b -k --grep='Bluetooth|BCM|hci_bcm4377'
find /sys/bus/acpi/devices -maxdepth 1 -type l -printf '%f\n' | grep '^BCM'
```

Firmware selection for other ACPI IDs must be established from the matching
model's Boot Camp package or hardware logs. Do not infer it from another Mac's
chip name or copy an arbitrary `.hcd` file.

Next: [Install KAIT2EN modules and apps](03-install-kait2en-modules-and-apps.md)
