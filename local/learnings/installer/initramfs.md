# Installer initramfs learnings

Related: PR #50, `scripts/fedora/rebuild-initramfs.sh`
Started: 2026-08-14
Measured on: MacBookPro16,2, Fedora 44, kernel 7.1.7-200.fc44.x86_64

## Why this exists

Users who install with LUKS end up with a dead keyboard at the passphrase
prompt. The install itself finishes fine, the first two boots are fine, and
only the boot after the main installer run is broken. The workaround that
fixed it for an affected user was a dracut drop-in with
`force_drivers+=" t2bce_dma t2bce_core t2bce_vhci "`.

## Root cause: Fedora's dracut never sees out-of-tree modules

Fedora builds hostonly images (`hostonly="yes"` in
`/usr/lib/dracut/dracut.conf.d/01-dist.conf`). Hostonly detection is not the
problem: `dracut-install --modalias` does list `t2bce_core`, `t2bce_dma`,
`t2bce_vhci` and `t2hid` as host relevant. The modules simply never become
candidates in the first place.

`/usr/lib/dracut/modules.d/70kernel-modules/module-setup.sh` collects modules
with

```sh
awk -F: '/^\// {print $1}' "$srcmods/modules.dep" 2> /dev/null | instmods
```

Fedora's `modules.dep` uses relative paths (`extra/t2bce_dma.ko.xz:`), so the
`/^\//` filter matches nothing and the line is a no-op. Everything under
`/usr/lib/modules/<kver>/extra` and `updates`, which is exactly where DKMS
puts our modules, is left out.

Measured on the running system before the fix: 435 modules in the initramfs,
all of them from `kernel/`, zero from `extra/` or `updates/`.

Without LUKS nobody notices, because the modules load normally after
switch-root. With LUKS the passphrase prompt runs inside the initramfs, so
the keyboard has to exist before that.

## Every place that writes an initramfs

| Site | Call | Modules present before the fix |
|---|---|---|
| `anaconda-addon/.../installation.py` | `dracut --force --add-drivers "<5>"` | yes |
| `packaging/installer/runtime/kait2en-prepare` | `dracut --force --force-drivers "<5>"` | yes |
| `scripts/fedora/rebuild-initramfs.sh` | `dracut --force` | no |
| `scripts/fedora/install-gpu-runtime-pm.sh` | `dracut --force` | no |
| kernel update, `/usr/lib/kernel/install.d/50-dracut.install` | `dracut --force` | no |
| `howto/07-updating.md`, `website/docs/post-install/updating.md` | `sudo dracut --force ...` | no |

This is why fixing only `rebuild-initramfs.sh` is not enough: the machine
breaks again at the next kernel update, and our own docs re-break it. A
persistent drop-in in `/etc/dracut.conf.d/` covers all six at once, because
every one of them is a plain `dracut` run that reads the same config
directory.

The rescue image is covered too. `51-dracut-rescue.install` builds with
`dracut -f --no-hostonly --no-uefi` and still reads `/etc/dracut.conf.d`, so
the rescue entry gets a working keyboard as well. That explains the second
half of the original report, where the rescue kernel did not help either.
Existing rescue images are not regenerated retroactively, only on the next
kernel install.

## Which modules the LUKS prompt actually needs

`usbcore`, `xhci-hcd`, `xhci-pci`, `hid`, `hid-generic` and `usbhid` are all
built into the Fedora kernel (`/lib/modules/<kver>/modules.builtin`), so only
the out-of-tree part has to be forced.

- `t2bce_dma`, `t2bce_core`, `t2bce_vhci` bring up the virtual USB bus. Not
  optional.
- `t2hid` is what the internal keyboard binds to on a running system
  (`0003:05AC:027E.008C`, `.008D`, `.0090`, `.0091`). Without it the builtin
  `hid-generic` takes the device instead. Typing works either way, which is
  why the three module workaround was enough, but `t2hid` sets no `.match`
  callback (`modules/t2touchbar/t2hid.c:1281`), so whether it takes the device
  back after switch-root was never verified. Forcing it avoids the question.
- `hid_t2magicmouse` is only the trackpad. A passphrase prompt is keyboard
  only, so it does not belong in the initramfs.

## A missing forced module cannot break a kernel update

Verified by putting a module name that does not exist into the drop-in:

```
dracut-install: Failed to find module 't2_does_not_exist'
dracut[E]: FAILED:  /usr/lib/dracut/dracut-install ... -m t2bce_dma t2bce_core t2bce_vhci t2hid t2_does_not_exist
EXIT=0
```

dracut logs the failure, exits 0, and writes the image anyway. The valid
modules still land in it, and only the missing one is skipped. The
`rd.driver.pre=` line is written for the missing module too, which produces a
harmless failed modprobe in the initramfs.

Consequence: nothing downstream can be bricked by this, but nothing warns
either. A `lsinitrd` check after the build is the only thing that turns a
silent "no keyboard at the LUKS prompt" into a visible installer error.

## The trap: a stale module in the initramfs loads and stays loaded

This is new behaviour introduced by `force_drivers` and worth remembering.

`force_drivers` writes `rd.driver.pre=<module>` into
`etc/cmdline.d/20-force_drivers.conf`, so the module is loaded from the
**initramfs copy** before switch-root. After switch-root it is already loaded
and nothing reloads it from disk. A rebuilt module on disk is therefore not
what is running.

Fedora builds without `CONFIG_MODVERSIONS` (`# CONFIG_MODVERSIONS is not set`
in `/boot/config-<kver>`), so there are no symbol CRCs and no "disagrees about
version of symbol" warning. Modules link by symbol name alone. A stale
`t2bce_core` in the initramfs binds to a freshly built `t2bce_audio` without
complaint and runs old code silently.

It only becomes loud in one case, and then in a misleading way: when a new
exported symbol is added to `t2bce_core`, the on-disk `t2bce_audio` or
`t2bce_vhci` that needs it fails with `Unknown symbol in module` after a
reboot, even though everything on disk was built correctly. The fault is not
in the module that fails to load, it is the old `t2bce_core` that came from
the initramfs.

When this happens, rebuild the initramfs rather than debugging the module:

```bash
sudo dracut --force "/boot/initramfs-$(uname -r).img" "$(uname -r)"
```

Which paths are safe:

- `install.sh` rebuilds the initramfs after the DKMS step, so the full
  installer is fine.
- A kernel update is fine. `40-dkms.install` builds before
  `50-dracut.install`, and DKMS' own
  `post_transaction="dracut --regenerate-all --force"`
  (`/etc/dkms/framework.conf:73`) refreshes the other kernels' images too.
- Running `install-dkms-modules.sh` on its own and rebooting is **not** safe.
  That script deliberately builds no initramfs. This is the documented dev
  workflow, and it is exactly why `CLAUDE.md` requires comparing
  `modinfo <module>` against `/sys/module/<module>/srcversion` before letting
  anyone test. That comparison catches this case.
