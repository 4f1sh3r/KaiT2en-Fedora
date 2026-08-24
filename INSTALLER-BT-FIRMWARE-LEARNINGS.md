# BCM4377 Bluetooth firmware in the automatic installer

Branch: `installer-bt-firmware` (from upstream `c086192`).

## Problem

The guided installer carried only the four Apple Wi-Fi files. On Macs whose
Bluetooth controller sits on PCIe (BCM4377) the installed system therefore had
no Bluetooth. Upstream commit `821a93f` documents the manual procedure; this
branch automates it.

## What hci_bcm4377 actually does

Read out of `hci_bcm4377.ko` on this machine plus the mainline source. All of it
matters for the implementation:

- The controller is PCI `14e4:5fa0`. The driver also claims `5f69`, `5f71` and
  `5f72`, but those chips only exist in Apple Silicon Macs, never in a T2 Mac.
  Gating on `5fa0` alone is therefore both correct and the narrowest gate.
- `bcm4377_request_blob()` builds two candidate names per blob and tries them in
  this order:

  ```text
  brcm/brcmbt<id><stepping>-<board_type>-<vendor>.<ext>
  brcm/brcmbt<id><stepping>-<board_type>.<ext>
  ```

- It uses `firmware_request_nowarn()`. **The brcmfmac log pattern does not exist
  here**: there is no `Direct firmware load ... failed with error -2` line at
  all. The driver prints its own error instead:

  ```text
  Unable to load firmware; tried 'brcm/brcmbt4377b3-apple,formosa-u.bin' and 'brcm/brcmbt4377b3-apple,formosa.bin'
  ```

  That one line contains both candidates verbatim, which is why nothing in this
  branch has to guess a filename. `Trying to load firmware` is `dev_dbg` and
  stays invisible at the default log level.
- `stepping` and `vendor` come from OTP. The vendor tag is `u` (USI) or `m`
  (Murata) and is empty on generic boards — that is exactly why the driver has a
  second, vendor-less candidate. It matches Apple's `MFG_USI` / `MFG_MUR` /
  `MFG_GEN` filenames one to one.
- `board_type` on x86 comes from `bcm4377_dmi_board_table`, which upstream only
  fills for **MacBookAir9,1, MacBookPro15,4 and MacBookPro16,3** (all
  `apple,formosa`). Any other model logs `unable to determine board type` and
  `bcm4377_probe()` returns `-ENODEV`. No firmware file can fix that, so this
  case is detected and reported separately instead of installing files that will
  never be read.
- The `.ptb` blob is only requested after the `.bin` has booted, so a missing
  `.ptb` cannot show up in the same boot as a missing `.bin`. Both are therefore
  installed together, with the `.ptb` name derived from the `.bin` name.

## Implementation

The Wi-Fi delivery chain was reused unchanged: macOS stages the files → third
initramfs (`kait2en-wifi-initramfs.img`) → `90-kait2en-updates.sh` copies them
into `/run`, which survives the switch root → live systemd unit plus an Anaconda
task writing into `$sysroot`.

- `scripts/macos/prepare-fedora-installer.sh` collects
  `/usr/share/firmware/bluetooth/BCM4377*.{bin,ptb}` into a `bluetooth/`
  subdirectory of the existing firmware staging directory, but only when
  `system_profiler SPBluetoothDataType` reports a `BCM4377*` chipset. It copies
  *every* antenna variant Apple ships: macOS cannot read the OTP vendor, only
  the chip knows it.
- `packaging/installer/runtime/install-bt-firmware.sh` picks the right blob using
  the name from the kernel log, cross-checked against the name derived from the
  Apple filename.
- `kait2en-live-bluetooth` + `.service` do the same in the live session, so a
  Bluetooth keyboard can be paired before installing — and, more importantly,
  the whole path can be exercised without completing an installation.
- `InstallBluetoothFirmwareTask` never raises. Bluetooth is not needed to finish
  an installation, and a hard failure on hardware we cannot test would be worse
  than a missing blob. It leaves `/var/log/kait2en/bluetooth-firmware.log` in the
  installed system so the outcome can still be asked for weeks later.

## Deliberate decisions

- **Log first, filename second.** The kernel name is authoritative; the Apple
  filename is the cross-check. If the two disagree on chip or stepping the helper
  refuses — that means firmware from a different Mac. If they only disagree on
  the antenna vendor and there is exactly one blob, the helper installs it under
  the requested name and warns. The name the driver asks for is the only one that
  can ever be loaded, so refusing there would help nobody.
- **Two independent gates**: PCI `5fa0` on the Linux side, `BCM4377*` filenames
  on the macOS side. `tests/installer/static-check.sh` asserts both, and that no
  `.hcd` path ever appears under `packaging/installer`.
- **Bluetooth is optional everywhere.** A Mac without PCIe Bluetooth prints one
  line and continues; nothing in this branch can fail an installation.

## Verified locally

- `bash tests/installer/static-check.sh` (includes `tests/installer/bt-firmware.sh`
  and `tests/installer/live-bluetooth.sh`, which cover USI/MUR/GEN naming,
  several blobs, the vendor mismatch, a missing PTB, a foreign chip, an unknown
  board type, and a non-BCM4377 Mac).
- `./scripts/fedora/build-installer.sh --target fedora-44`, for the new cpio
  assertions on `updates.img` and the input initramfs.
- The guard on this machine (MacBookPro16,2, `14e4:4464`, UART Bluetooth):
  `kait2en-live-bluetooth` exits 0 with "no BCM4377 Bluetooth controller",
  `install-bt-firmware.sh` refuses, and neither writes a file.
- `kait2en-live-diagnostics` runs and reports the new Bluetooth block correctly
  on a machine without BCM4377.

## Open on hardware

Nobody involved owns a BCM4377 Mac, so the first tester report has to settle:

1. the exact wording of the `Unable to load firmware` line on their kernel,
2. whether their model is in the driver's DMI table,
3. whether Apple ships one or several `_PROD.signed.bin` blobs on that machine.

All three are visible in the diagnostics archive
(`sudo /run/kait2en/kait2en-live-diagnostics --rerun`), in its `summary.txt`
block alone.
