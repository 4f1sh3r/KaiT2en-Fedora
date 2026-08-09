# KaiT2en brcmfmac PM_MAX test module

This DKMS package temporarily replaces Fedora's in-tree `brcmfmac` module so
KaiT2en maintainers and testers can evaluate the firmware's `PM_MAX` power-save
mode on T2 Macs. It does not replace `brcmutil` or the firmware-vendor plugins
such as Fedora's `brcmfmac_wcc` module.

The source is the `brcmfmac` and `brcm80211/include` subtree from Linux 7.1.6,
as shipped by Fedora in `kernel-7.1.6-201.fc44.src.rpm`. Files retain their
upstream SPDX identifiers and copyright notices. Fedora's
`patch-7.1-redhat.patch` did not change this subtree. The driver carries the
same opt-in change intended for upstream submission:

- expose a read-only `max_pm` module parameter;
- retain upstream's `PM_FAST` default when the parameter is absent;
- select `PM_MAX` when `max_pm=1` and firmware power saving is enabled.

The KaiT2en installer writes
`/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf` to activate the opt-in for
community testing. The restore script removes this file before restoring
Fedora's stock module. Until the parameter is accepted upstream, a DKMS build
failure on a new kernel also requires running the restore script before the
stock module can load.

Run the regular KaiT2en installer to register and build the module. It becomes
active after the next reboot. To remove it and restore Fedora's module, run:

```bash
sudo bash scripts/fedora/restore-stock-brcmfmac.sh
sudo reboot
```

The restore script accepts `--reload` to unload the active module and load the
stock one immediately. This briefly disconnects Wi-Fi, so only use that option
from a local session or while another network connection is active.

This is deliberately a test package, not yet a claim that `PM_MAX` is suitable
for every Broadcom FullMAC device. The vendored source must be reviewed and,
when required by kernel API changes, updated when KaiT2en raises its supported
kernel baseline.
