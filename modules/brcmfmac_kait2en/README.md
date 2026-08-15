# KaiT2en brcmfmac PM_MAX test module

**This experiment has been withdrawn and is no longer installed by the
KaiT2en installer.** Extended BCM4364 testing found intermittent, reproducible
50-250 ms latency spikes with `PM_MAX`. The source remains here only to
preserve the test history and provide the tested restore path for existing
testers.

The source is the `brcmfmac` and `brcm80211/include` subtree from Linux 7.1.6,
as shipped by Fedora in `kernel-7.1.6-201.fc44.src.rpm`. Files retain their
upstream SPDX identifiers and copyright notices. Fedora's
`patch-7.1-redhat.patch` did not change this subtree. The driver carries the
same opt-in change intended for upstream submission:

- expose a read-only `max_pm` module parameter;
- retain upstream's `PM_FAST` default when the parameter is absent;
- select `PM_MAX` when `max_pm=1` and firmware power saving is enabled.

Earlier branch revisions wrote
`/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf` and installed this module through
DKMS. To remove that installation and restore Fedora's module, run:

```bash
sudo bash scripts/fedora/restore-stock-brcmfmac.sh
sudo reboot
```

The restore script accepts `--reload` to unload the active module and load the
stock one immediately. This briefly disconnects Wi-Fi, so only use that option
from a local session or while another network connection is active.

Do not reinstall this module for normal use.
