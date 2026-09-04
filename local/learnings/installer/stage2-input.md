# Installer stage 2: dead internal keyboard and trackpad

Branch: `installer-stage2-input` (from upstream `3426048`).

## Symptom

A tester ran the automatic installer to completion. Stage 1 (first boot of the
installed system, still on the ISO kernel) was fine: the KDE account wizard and
the guided installer both worked with the internal keyboard and trackpad. After
the reboot into stage 2 the internal keyboard and trackpad were dead — the login
screen and the `sudo` password prompt both needed an external keyboard. After
the final reboot everything worked again.

This is not related to the live Wi-Fi work; it predates it and had been reported
in the community before.

## Evidence

Two stage-2 boots were collected from the tester (journal, `/proc/cmdline`,
`grubby --info=ALL`, `~/.local/state/kait2en/install.log`).

- Kernel arguments are intact in both boots, including
  `module_blacklist=...,hid_apple,...`. A lost bootloader configuration is ruled
  out.
- `t2bce_dma`, `t2bce_core` and `t2bce_vhci` load at ~1.35 s, i.e. inside the
  initramfs. The chain itself is healthy.
- `t2hid` and `hid_t2magicmouse` never appear at all: `grep -c t2hid` over both
  complete journals returns 0.
- The topcase (`05ac:027f`, `Apple Internal Keyboard / Trackpad`) enumerates but
  gets **no** bind message from any driver, unlike every other HID device in the
  same log (Razer, HOLTEK, Touch Bar Display, Touch Bar Backlight, Apple Headset
  all bind `hid-generic`). Apple keyboards are listed in the kernel's
  `hid_have_special_driver`, so `hid-generic` deliberately stays away, and
  `hid_apple` is blacklisted. The device ends the boot without a driver.

Timing, which is the actual proof:

| boot | topcase HID appears | switch root | gap |
| --- | --- | --- | --- |
| first stage-2 boot | 3.9845 s | 3.9886 s | 4 ms |
| later stage-2 boot | ~4.0219 s (Touch Bar) | 4.0253 s | 3.4 ms |

The second boot looks at first glance as if there were 1.5 s of headroom
(`usb 5-5` is found at 2.479 s), but that is only the USB device descriptor. The
virtual bus is slow: `usb 5-6` needs from 2.728 s ("new device") to 4.020 s
("device found"). The HID devices of the topcase materialize right at the switch
root in both boots.

## Root cause

`kait2en-prepare` builds the transition modules for the upgraded kernel, bakes
them into the initramfs and then **deletes them from the disk again**
(`rm -rf "$transition_dir"`, followed by `rpm -e kmod-kait2en-input`). On the
next boot the initramfs is their only copy.

`dracut --add-drivers` merely places modules into the image; it does not load
them. `t2bce_core` still comes up early through its PCI alias, and `t2bce_vhci`
follows through `MODULE_SOFTDEP("post: ...")`. But `t2hid` and
`hid_t2magicmouse` are only requested once the topcase has enumerated on the
virtual USB bus — and that request loses the race against the switch root. From
then on modprobe searches the real root, where the files no longer exist.

Stage 1 survives the same race because the RPM additionally leaves the modules
on disk, so the late modprobe succeeds. The final system survives because DKMS
installs everything permanently into `/lib/modules/<kernel>/extra/`. Stage 2 is
the only state without a safety net.

## Fix

`--add-drivers` → `--force-drivers` in `kait2en-prepare`. `--force-drivers`
additionally writes `etc/cmdline.d/20-force_drivers.conf` (`rd.driver.pre=...`)
into the image, which makes dracut modprobe the modules before the udev
coldplug. The HID drivers are then in the kernel long before the topcase
enumerates, so there is no race left to lose.

## Confirmed on hardware

Verified on the development MacBook Pro 16,2 without reinstalling anything. The
failure does not have to be reproduced: it is enough to show that the forced
load happens before the switch root, because in stage 2 the initramfs is the
only source. On-disk copies cannot cause an early load, so an early `t2hid` line
can only come from the image.

Method: back up `/boot/initramfs-$(uname -r).img`, rebuild it with
`--force-drivers` and the five modules, reboot, then compare timestamps. Purely
additive and therefore safe — the modules stay on disk as a fallback.

| | `t2hid` binds the topcase | switch root |
| --- | --- | --- |
| stock initramfs, modules loaded from disk | 11.999 s | 9.70 s |
| `--force-drivers` | **2.493 s** | 9.659 s |

The driver is in the kernel roughly 7 s before the switch root and has already
bound the internal keyboard and trackpad inside the initramfs. `Touch Bar
Backlight` moves from 12.6 s to 3.18 s as well, so the whole topcase stack comes
up early, not just one lucky device. The initramfs was restored afterwards.

## Verified along the way (dead ends, so nobody repeats them)

- The image itself was never the problem. `dracut --add-drivers` does include
  all five modules, and `modules.alias` inside the image resolves
  `hid:b0003g*v000005ACp0000027F` to `t2hid` correctly.
- `kait2en-prepare` ran to completion on the tester's machine (both
  `grubby --set-default` messages are in `install.log`), so its own `lsinitrd`
  verification did pass. Availability was never the issue, only timing.
- Alias order is not the issue either. `hid_apple` is listed before `t2hid` for
  the same alias on a working system too, and modprobe does not abort on the
  blacklisted first match.
- The kernel arguments do survive the kernel upgrade; `grubby` carries them over
  to the new BLS entry.

## Not changed on purpose

`InstallTransitionDriversTask` in the Anaconda addon uses `--add-drivers` as
well, but there the RPM keeps the modules on disk, so stage 1 has a working
fallback. Left alone to avoid touching a path that demonstrably works.

A second possible safety net — deferring `rm -rf "$transition_dir"` and
`rpm -e` to `kait2en-prepare --complete`, i.e. after stage 2 — was considered
and not implemented. It would leave `updates/kait2en-transition` in place while
`install.sh` runs DKMS, and depmod prefers `updates/` over `extra/`, so the
stale copies would shadow the fresh ones for the duration of the install. The
forced load solves the reported problem without that risk.
