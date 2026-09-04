# Installer live gmux learnings

Branch: `fix/installer-live-apple-gmux`
Started: 2026-08-07

## Why this branch exists

On the MacBookPro16,1 that could not boot the installer (see
`local/learnings/installer/grub-menu.md`), the live system loads `i915` but never
binds the iGPU. The whole dmesg contains exactly one deferred probe and no
`Initialized i915` at all:

```
[   19.527469] pci 0000:00:02.0: deferred probe pending: (reason unknown)
```

The cause is `vga_switcheroo_client_probe_defer()`. It defers when three things
hold at once: the gmux ACPI device is present, the card is not the boot VGA,
and no vga_switcheroo handler is registered.

All three hold in the live system on a dual-GPU Mac. The first is a pure ACPI
lookup that does not care whether any module is loaded —
`include/linux/apple-gmux.h`:

```c
static inline bool apple_gmux_present(void)
{
	return acpi_dev_found(GMUX_ACPI_HID);   /* "APP000B" */
}
```

The boot VGA is the dGPU, and no handler registers because `apple_gmux` is
blacklisted on the live kernel command line while `t2gmux` only ever reaches
the installed system through DKMS. So `i915` waits for something that cannot
arrive, and the iGPU stays unused for the whole live session.

## The change

`apple_gmux` removed from `kait2en_blacklist` in
`packaging/installer/grub.cfg.in`. The blacklist exists because `t2gmux`
replaces `apple_gmux` in the KaiT2en stack — a replacement reason, not a
"this driver is harmful" reason. Where the replacement does not exist, the
blacklist is pure collateral damage.

## Blast radius

The same blacklist string lives in three places. Only the first one is touched:

- `packaging/installer/grub.cfg.in` — the live system. Changed.
- `packaging/installer/anaconda-addon/com_kait2en_input/service/constants.py.in`
  — written into the freshly installed system by the Anaconda addon, i.e. the
  first boot before `kait2en-install` runs. `t2gmux` does not exist there
  either, so the same argument applies, but it is a separate decision with a
  larger blast radius. Left alone.
- `scripts/fedora/install-kernel-args.sh` — the final installed system. Here
  `apple_gmux` **must** stay blacklisted: `t2gmux` binds the same `APP000B`
  ACPI device and is the intended driver. Left alone.

Single-GPU T2 MacBooks are unaffected. Verified on the dev Mac
(`afis-fedora-mac`): there is no `APP000B` ACPI device at all, so no gmux
driver ever probes and the backlight comes from `intel_backlight`. The change
can therefore only take effect on dual-GPU MacBook Pros and T2 iMacs.

## Open risk, for the maintainers to weigh

The live system would now run upstream `apple_gmux` while the installed system
runs `t2gmux`. If the fork exists because upstream misbehaves on T2 hardware,
the live session inherits that. `modules/t2gmux/t2gmux.c` looks like upstream
plus the MMIO/"T2" config, and upstream has had MMIO gmux support since 6.3,
but the two were not diffed — no upstream source tree was available locally.
Worth confirming before this is relied upon.

Second, untested effect: on dual-GPU Macs `i915` will now actually bind in the
live session. That is the point of the change, but it is a behaviour change for
machines that currently boot fine — a second DRM device appears and
vga_switcheroo becomes active. On the reporter's machine amdgpu owns the
firmware framebuffer and should stay primary.

Expected side benefit: `apple_gmux` registers `gmux_backlight`, so panel
brightness should work in the live session. It cannot collide with amdgpu,
which already logs `[drm] Skipping amdgpu DM backlight registration`.

## Pre-existing defect found in tests/installer/static-check.sh

Every negative assertion written as `! rg …` or `! grep …` in that script is a
no-op. Bash does not apply `set -e` when a command's return value is inverted
with `!`, so those lines can match and the script still exits 0. Verified:

```bash
#!/usr/bin/env bash
set -Eeuo pipefail
! grep -q root /etc/passwd
echo "REACHED END"      # prints, exit 0
```

That covers the `OEMDRV|rhdd3|inst.dd=`, `brcmfmac`, `INPUT_COMPAT_PATCH`,
`--add-drivers`, `kait2en-live-wifi`, `.hcd`, `$HOME/.config/autostart`,
`find_regular_user`, the `plist_value … Whole` and `actions/*@v4` checks. The
one negative check that works is the `if rg …; then exit 1; fi` block for
`kait2en-first-boot`, which is why the new check here uses that form.

Not fixed in this branch — it is a separate change and would likely surface
real failures that need triage.
