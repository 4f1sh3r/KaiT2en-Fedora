# Installer live Wi-Fi learnings

Branch: `installer-live-wifi`. Goal: Wi-Fi already available in the Fedora live
session of the KaiT2en automatic installer, not only after the installation.

## Everything needed was already on the stick

`scripts/macos/prepare-fedora-installer.sh` packs the four Apple firmware files
into `kait2en-wifi-initramfs.img`, GRUB loads it as a third initrd, and the
dracut `pre-pivot` hook copies them to `/run/kait2en/apple-firmware`. `/run`
survives the switch root (commit b6d0154), so the live session can reach them.
Only the naming and a driver reload were missing.

## brcmfmac firmware naming: the request chain is mostly silent on new kernels

Measured with dynamic debug on kernel 7.1.5-200.fc44 (BCM4364, MacBook Pro T2):

```
echo 'file drivers/base/firmware_loader/main.c +p' > /sys/kernel/debug/dynamic_debug/control
```

brcmfmac asks for, in this order:

1. `brcm/brcmfmac4364b3-pcie.apple,trinidad-HRPN-u-7.7-X3.bin`
2. `brcm/brcmfmac4364b3-pcie.apple,trinidad-HRPN-u-7.7.bin`
3. `brcm/brcmfmac4364b3-pcie.apple,trinidad-HRPN-u.bin`
4. `brcm/brcmfmac4364b3-pcie.apple,trinidad-HRPN.bin`
5. `brcm/brcmfmac4364b3-pcie.apple,trinidad-X3.bin`
6. `brcm/brcmfmac4364b3-pcie.apple,trinidad.bin`
7. `brcm/brcmfmac4364b3-pcie.bin`

**Only candidate 7 produces `Direct firmware load ... failed with error -2`.**
Candidates 1-6 are requested without a warning, so they never reach the journal.
The manual guide (`howto/installation/manual/02-install-broadcom-firmware.md`)
shows every candidate warning, which is how older kernels behaved. This means
the journal-only detection in `install-wifi-firmware.sh` could not have found a
board-specific name on such a kernel; it would have found the generic name and
died on the `brcmfmac*-pcie.apple,*` check.

What is always logged, on both old and new kernels, is:

```
brcmfmac: brcmf_fw_alloc_request: using brcm/brcmfmac4364b3-pcie for chip BCM4364/4
```

That gives the chip prefix. The board identity comes from the NVRAM filename
(`P-trinidad_M-HRPN_V-u__m-7.7.txt` -> `trinidad-HRPN-u-7.7`), which yields
candidate 2 exactly. Verified on hardware: installing the `.trx` under that name
loads the firmware (`TxCap blob found`, `Firmware: BCM4364/4 ... FWID 01-68d7ff80`).

`install-wifi-firmware.sh` therefore now tries the warned board-specific name
first (best, it carries the `-X3` antenna variant) and falls back to the
constructed name. This also un-breaks the installer path on newer kernels.

## The antenna variant must not be pasted in front of the module details

This is what broke the first live boot, and it was a latent bug in the naming
logic that predates this branch. macOS names the files it carries like this on
a MacBookPro16,2:

```
trinidad.trx
trinidad-X3.clmb
trinidad-X3.txcb
P-trinidad-X3_M-HRPN_V-u__m-7.7.txt
```

The board name in the blobs and in the NVRAM file carries the antenna variant
(`trinidad-X3`); the `.trx` file carries the bare board (`trinidad`). The old
code took the board straight out of the NVRAM filename and appended the module
details, producing `apple,trinidad-X3-HRPN-u-7.7`. Comparing that against the
candidate list above shows it is not one of the seven names brcmfmac ever asks
for. brcmfmac asks either for `<board>-<module>-<vendor>-<version>` (candidate
2) or for `<board>-<antenna>` (candidate 5) — never for both parts at once.

On the live stick the result was: the `.bin` was found (the warned path takes
the logged name verbatim, so it was right), the NVRAM was not, and the dongle
never came up:

```
brcmfmac 0000:e5:00.0: Direct firmware load for brcm/brcmfmac4364b3-pcie.txt failed with error -2
brcmfmac 0000:e5:00.0: brcmf_pcie_download_fw_nvram: FW failed to initialize
ieee80211 phy0: brcmf_fw_crashed: Firmware has halted or crashed
```

The fix derives the bare board from the `.trx` filename and uses it for the
NVRAM name and for the constructed `.bin` name, while the blobs keep the
antenna-specific name they already had. Reproduced and verified on hardware
with the real macOS filenames (see below).

**Where the antenna token came from: upstream c6a8d79 / bd692ce (2026-07-24).**
Those commits replaced the macOS-side firmware collection — parsing
`log show --last boot` for `Copying "..."` lines — with reading `RequestedFiles`
from IORegistry, in `scripts/macos/prepare-fedora-installer.sh` and in the
manual guide's collection snippet. macOS ships both a generic and an
antenna-specific variant of the blobs and the NVRAM; the old log-based method
picked up the generic names (`trinidad.clmb`, `P-trinidad_M-HRPN_V-u__m-7.7.txt`),
IORegistry reports the ones the driver actually requested
(`trinidad-X3.clmb`, `P-trinidad-X3_M-HRPN_V-u__m-7.7.txt`). The `.trx` is
unaffected and stays `trinidad.trx`.

The naming logic downstream was never adjusted, so the installer regressed three
days later. That reconciles the two observations: Wi-Fi from this installer used
to work on a MacBookPro16,2, and the same machine now fails. The collection
rework itself is right — it fixed a Mac where the log-based method found
nothing — and the fix here is downstream of it, so both hold. Since
`board_base` comes from the `.trx` name, the helper now produces correct names
for either collection method.

**Blast radius: this was never live-only.** The Anaconda task calls the same
helper with the same source directory, on the same live 6.19.10 kernel, so it
takes the warned path, gets the `.bin` right and writes the very same NVRAM name
into `$sysroot`. Installed systems built from a stick prepared after c6a8d79
would carry it too; that has not been observed directly, only derived. The
identical bug sat in the inline script of
`howto/installation/manual/02-install-broadcom-firmware.md` and was fixed there
too.

Why the first hardware test missed it: that test built its source directory by
reverse-renaming the *installed* files, and the installed NVRAM name has no
antenna token, so the fixture came out as `trinidad.clmb` /
`P-trinidad_M-HRPN...`. It never contained the `-X3` that the real macOS files
carry. Fixtures reconstructed from the output of the thing under test cannot
falsify it — the diagnostics archive from the failed live boot was the first
source of the real names.

## brcmfmac cannot be unloaded on its own

`modprobe -r brcmfmac` fails with `Module brcmfmac is in use` because
`brcmfmac_wcc` holds a reference on it. The vendor module has to be removed
first. If the driver still cannot be unloaded, unbind/bind through
`/sys/bus/pci/drivers/brcmfmac/` is the fallback.

## A stale wpa_supplicant survives the driver reload and breaks scanning

After the interface disappeared and came back, wpa_supplicant kept the old
registration and fell back to the WEXT backend:

```
ioctl[SIOCSIWSCAN]: Inappropriate ioctl for device
wlp229s0: CTRL-EVENT-SCAN-FAILED ret=-1
```

`iw dev wlp229s0 scan` found 14 networks at the same time, so the radio was
fine. `systemctl try-restart wpa_supplicant` fixes it and NetworkManager
autoconnects afterwards. The live script does this whenever a wireless
interface showed up.

Unrelated but worth knowing on this machine: the saved NetworkManager profiles
exist twice, bound to `wlp3s0` (dead) and `wlp229s0`. Only the `... 1` variants
can activate.

## Delivery without touching the live root

The live root is a read-only squashfs on the stick, so the service is dropped
into `/run/systemd/system` from the `pre-pivot` hook instead of
`$NEWROOT/etc/systemd/system`. `systemd-analyze unit-paths` confirms `/run` is
searched, and nothing in `/run` can ride the live payload copy into the
installed system. Fallback if a live boot ever fails to pick the unit up: write
it into `$NEWROOT/etc/systemd/system` and delete it from `$sysroot` in the
Anaconda addon.

## Test result on hardware

Firmware removed, driver reloaded, then `kait2en-live-wifi` run with the
reverse-renamed source directory: firmware installed, driver reloaded,
`wlp229s0` back, NetworkManager connected. Total runtime 1.6 s.

This ran on 7.1.5-200.fc44, which is the kind of kernel the **installed** system
boots after `kait2en-prepare` upgrades Fedora — not only the live kernel. So the
constructed name is confirmed to be a candidate on a post-upgrade kernel, which
is what the Anaconda path depends on. The construction rests on brcmfmac
trimming board-type tokens from the right, so the name is a candidate by
construction. If a future kernel ever drops that trimming, the installed system
comes up without Wi-Fi firmware and nothing warns about it.

`./scripts/fedora/build-installer.sh --target fedora-44` builds cleanly and the
three new files are in `kait2en-input-initramfs.img`. Rootless podman on this
machine needs `--security-opt label=disable` and cannot chown the artifact back
to the host user; that is a local quirk, CI builds with docker.

## Test release v1.9.0-live-wifi (the fixed one)

```bash
curl -fsSL https://github.com/4f1sh3r/KaiT2en-Fedora/releases/download/v1.9.0-live-wifi/install-kait2en-fedora.sh |
  KAIT2EN_RELEASE_BASE_URL=https://github.com/4f1sh3r/KaiT2en-Fedora/releases/download/v1.9.0-live-wifi bash
```

Markers that the stick really carries the fix, in increasing order of strength:
the downloaded archive is `KaiT2en-Fedora-44-1.9-Installer-x86_64.zip`; in the
live session `sha256sum /run/kait2en/install-wifi-firmware.sh` is
`dc47ee72cb253e2960877546708d7e36b9ead7989c39897c5066335937cbebcc` (the failed
1.8 stick had `5fc886829c887f061cbb8c197479746699d13253b04d1ad15bf340c50624e9ef`).
Both the `/run` copy and the Anaconda `updates.img` copy were extracted from the
**published CI artifact** and carry that hash. Check the published zip, not a
local build: the two differ in size (330550 vs 330621 bytes) because the image
is not byte-reproducible across machines, while the copied helper scripts are
verbatim and must hash identically.

Note `kait2en-live-wifi` itself is unchanged from 1.8
(`90bba49c9eceecd9f23ffbc5ee16c9fe416417a004b11712a63527c7ac2d0789`), so its
hash cannot tell the two sticks apart — only the helper's can.

## Earlier test release (broken NVRAM name)

Branch `installer-live-wifi` and tag `v1.8.0-live-wifi` are pushed to the
`4f1sh3r` fork. CI built and published the kit, and the tag is the fork's
`latest` release, so the plain bootstrap one-liner picks it up:

```bash
curl -fsSL https://github.com/4f1sh3r/KaiT2en-Fedora/releases/download/v1.8.0-live-wifi/install-kait2en-fedora.sh | bash
```

Pinned to this exact release regardless of what `latest` points at later:

```bash
curl -fsSL https://github.com/4f1sh3r/KaiT2en-Fedora/releases/download/v1.8.0-live-wifi/install-kait2en-fedora.sh |
  KAIT2EN_RELEASE_BASE_URL=https://github.com/4f1sh3r/KaiT2en-Fedora/releases/download/v1.8.0-live-wifi bash
```

Diagnostics in the live session, no network needed:

```bash
sudo /run/kait2en/kait2en-live-diagnostics --rerun
```

Same script over the network, if the stick copy is the thing that is missing:

```bash
curl -fsSL https://raw.githubusercontent.com/4f1sh3r/KaiT2en-Fedora/v1.8.0-live-wifi/packaging/installer/runtime/kait2en-live-diagnostics |
  sudo bash -s -- --rerun
```

## The delivery path works

Confirmed on a real live boot of the `v1.8.0-live-wifi` stick. `/run/kait2en`
held the firmware, all three helpers and `updates.img`; the unit was in
`/run/systemd/system` with its `multi-user.target.wants` symlink; systemd loaded
it as `enabled-runtime` and ran it to `active (exited)` with status 0. So the
pre-pivot hook plus `/run` surviving the switch root is a working way to put a
service into the live session, and the fallback into `$NEWROOT/etc/systemd/system`
is not needed.

systemd did log `The unit file ... changed on disk. Run 'systemctl daemon-reload'`.
Harmless here — the unit had already run — but it means the hook writes the unit
after systemd first scanned `/run/systemd/system`.

## Reproducing on hardware without rebuilding a stick

Build the source directory with the **real** macOS filenames (from the failed
boot's diagnostics), not by reverse-renaming installed files:

```
apple,trinidad-HRPN-u-7.7-X3.bin -> trinidad.trx
apple,trinidad.clm_blob          -> trinidad-X3.clmb
apple,trinidad.txcap_blob        -> trinidad-X3.txcb
apple,trinidad-HRPN-u-7.7.txt    -> P-trinidad-X3_M-HRPN_V-u__m-7.7.txt
```

Wi-Fi is the only link on this machine, so the run has to be one detached
script that removes the firmware, runs both the broken and the fixed helper, and
restores the backup from a `trap restore EXIT INT TERM` with a `sleep`-based
watchdog that kills itself if a phase hangs. Session connectivity comes back on
its own afterwards. Old helper: no interface, dongle setup failed. New helper:

```
brcmfmac: brcmf_c_process_txcap_blob: TxCap blob found, loading
brcmfmac: brcmf_c_preinit_dcmds: Firmware: BCM4364/4 ... FWID 01-68d7ff80
brcmfmac 0000:e5:00.0 wlp229s0: renamed from wlan0
```

This machine runs 7.1.5, so it exercises the **quiet** path. The warned path
differs only in the `.bin` name (candidate 1 instead of candidate 2), and that
name is the one this machine has installed and working, so both are covered.
`tests/installer/wifi-firmware.sh` now stubs a 6.19.10-style warned log against
the antenna-suffixed source names so the branch that actually broke stays tested.

## Still unverified

Whether the fixed stick brings up Wi-Fi in the live session on its own, and
whether an installation from it produces an installed system with working
firmware. Both need a rebuilt stick and a live boot:

```
systemctl status kait2en-live-wifi
ls /usr/lib/firmware/brcm/*.apple,*
```
