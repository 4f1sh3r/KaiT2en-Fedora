# brcmfmac PM_MAX test build

This directory is intentionally untracked. Nothing here has been installed.

- Target kernel: `7.1.6-201.fc44.x86_64`
- Source RPM: `kernel-7.1.6-201.fc44.src.rpm`
- Source file: upstream/Fedora `linux-7.1.6` `cfg80211.c`
- Fedora `patch-7.1-redhat.patch` does not modify the affected brcmfmac files.
- Module SHA-256: `0b0acf30932a5cc789103326889664b62ae1d9415d2eeb0da9ebbcd2199b0e5b`
- Module parameter: `power_save_mode=1` selects `PM_MAX`; `2` selects the
  stock/default `PM_FAST`. Other values are rejected.
- Secure Boot was disabled while preparing the build. The running kernel was
  already tainted `O/E` by KaiT2en external unsigned modules.

The module is deliberately not copied into `/lib/modules` and does not replace
the packaged Fedora module. Because SELinux correctly rejects module loading
from a `user_home_t` workspace, the controller creates a private temporary copy,
labels that copy `modules_object_t`, loads it with `insmod`, and deletes it on
both success and failure. It samples WLAN/LTR and reloads the packaged module
before it exits. Its EXIT trap invokes `restore-stock.sh` after any failure or
interruption.

Do not run the controller on another kernel. Before running it:

1. Keep wired Ethernet active as the non-WLAN default route.
2. Keep WLAN enabled and associated.
3. Do not suspend, undock, or change USB-C devices during the run.
4. Start it only after the maintainer has explicitly confirmed readiness.

Expected result file: `/tmp/cstates-brcmfmac-pm-max-ltr-test.txt`.

`run-ltr-test.sh --preflight` performs all checks without unloading or loading
anything. Running the controller without that option performs the active test.

`restore-stock.sh` can be run independently if the controller is interrupted.
Successful restore means the `power_save_mode` sysfs parameter is absent again,
the packaged `brcmfmac` and `brcmfmac_wcc` are loaded, and WLAN reconnects.

After the LTR gate succeeds, `run-cstates-ab.sh` compares a quiet 45-second
stock/`PM_FAST` window with a quiet 45-second `PM_MAX` window. It has the same
kernel, checksum, Ethernet-route, LTR, SELinux staging, and automatic-restore
guards. `run-cstates-ab.sh --preflight` is read-only. Its result files are:

- `/tmp/cstates-brcmfmac-pm-max-ab-controller.txt`
- `/tmp/cstates-brcmfmac-pm-fast-stock.txt`
- `/tmp/cstates-brcmfmac-pm-max.txt`

`run-cstates-ab.sh --standalone` is the explicit battery/WLAN-only variant. It
requires AC offline and WLAN to be the only IPv4 default route. The Codex
connection can disappear briefly while brcmfmac reloads, but the transient
systemd service continues independently and restores the packaged driver.
Combine it with `--preflight` for a read-only standalone readiness check.

`--pm-max-only` skips the first stock measurement. It is used when a valid
same-session stock window has already been saved and only the missing PM_MAX
window needs to be collected. LTR acceptance is based on the settled timeline,
not on a racy single sample immediately after WLAN reassociation.
