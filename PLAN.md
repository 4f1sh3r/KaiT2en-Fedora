# KaiT2en plan

KaiT2en is a handmade Fedora path for Intel T2 Macs. It stays close to standard
Fedora and adds the missing T2 pieces from this repository.

## Current direction

- Fedora only
- macOS must stay installed
- repository is used as an offline USB kit
- commands run from the KaiT2en repository root
- no hidden one-click installer
- scripts may automate clear, separate tasks

## Module strategy

Use DKMS for the first working release.

Reasons:

- simpler than akmods
- easier for users to understand and modify
- enough for the expected user base
- no Fedora RPM repository needed at the start
- can move to akmods later if the project grows

Initial modules:

- `t2bce`, replacing the old `apple-bce` name
- `t2smc`
- `t2bdrm`, replacing `appletbdrm`
- `t2touchbar`, replacing the Touch Bar keyboard and backlight pieces
- `hid_t2magicmouse`, replacing Fedora `hid_magicmouse` for Magic Mouse and T2 trackpads
- `t2mfi_fastcharge`, replacing `apple_mfi_fastcharge`
- `t2gmux`, replacing `apple_gmux`
- `t2thunderbolt`, for working Thunderbolt on current T2 Macs

Initial apps:

- `react-drm`, mandatory Touch Bar runtime
- `t2-fan-control`
- `t2-smc-control`

Planned repo layout:

```text
modules/
  t2bce/
  t2smc/
  t2bdrm/
  t2touchbar/
  hid_t2magicmouse/
  t2mfi_fastcharge/
  t2gmux/
  t2thunderbolt/

apps/
  react-drm/
  t2-fan-control/
  t2-smc-control/

systemd/
  kait2en-suspend.service

scripts/fedora/
  install-kernel-args.sh
  install-dkms-modules.sh
  install-suspend-service.sh
  rebuild-initramfs.sh
```

## Kernel arguments

Default KaiT2en kernel arguments:

```text
intel_iommu=on
iommu=pt
pm_async=off
acpi_osi=!Darwin
acpi_osi=Windows 2012
pcie_ports=native
mem_sleep_default=deep
initcall_blacklist=cmos_init,magicmouse_driver_init
module_blacklist=acpi_tad,applesmc,macsmc,hid_apple,hid_appletb_bl,hid_appletb_kbd,hid_magicmouse,appletbdrm,thunderbolt,apple_bce,apple_mfi_fastcharge,apple_gmux
```

Installing KaiT2en modules into a kernel `updates` path is not enough when the
module names differ. The original drivers still have matching modaliases and can
bind first. Blacklisting the replaced drivers is required.

Not default:

```text
pcie_aspm=force
pcie_aspm.policy=powersupersave
nvme_core.default_ps_max_latency_us=200
t2gmux.force_igd=1
```

`t2gmux.force_igd=1` is installed automatically on T2 MacBook Pro models with a
discrete GPU. It must not be set globally because iMacs do not have the same
display pipeline.

Other model-specific quirks go into separate documents.

## Driver ownership audit

Candidates to review and possibly replace with KaiT2en-maintained drivers:

```text
hid_apple
hid_appletb_kbd
hid_appletb_bl
hid_magicmouse
appletbdrm
apple_gmux
apple_mfi_fastcharge
applesmc
macsmc
```

Goal: clear separation between Fedora upstream drivers and KaiT2en-maintained
T2 drivers. Prefer `t2*` naming for drivers we maintain. Alternatively replace
module author with `kait2en` so that users can differentiate.

`t2thunderbolt` is different from the other drivers because it is not T2
specific and follows a busy upstream subsystem. Keep it in the first release
because it is required for current machines, but audit the long-term maintenance
cost before promising to carry it forever.

## Install flow

Step 04 should install the KaiT2en modules:

1. install Fedora build dependencies
2. set KaiT2en kernel arguments with `grubby`
3. install the KaiT2en module blacklist with `grubby`
4. install DKMS modules from local `modules/`
5. rebuild initramfs
6. install apps
7. install and enable `kait2en-suspend.service`
8. reboot
9. verify modules and hardware

## Suspend handling

Install one systemd service for hardware-related suspend fixes:

```text
kait2en-suspend.service
```

The service runs a helper script before `sleep.target` and again on resume. The
script detects local hardware and only applies matching fixes.

Current fixes:

- unload and reload `amdgpu` on T2 MacBook Pro models with dGPU suspend issues
- unload `brcmfmac_wcc`, `brcmfmac`, `hci_bcm4377` before suspend on BCM4377 systems
- reload only modules that were unloaded by the helper

## Updates

Do not make automatic KaiT2en updates the default.

Preferred first update model:

- GitHub release bundle
- contains source, scripts and release notes
- user consciously downloads and installs it
- DKMS rebuilds modules locally

Possible later model:

- akmods
- RPM packages
- optional Fedora repository

Do this only if the project grows enough to justify the maintenance cost.

## Later documents

- Fedora installation after partitioning
- DKMS module installation
- suspend and resume services
- model-specific quirks
- driver ownership audit
