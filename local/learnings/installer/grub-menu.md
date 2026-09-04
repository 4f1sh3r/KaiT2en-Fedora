# Installer GRUB menu learnings

Branch: `fix/installer-grub-troubleshooting-entries`
Started: 2026-08-07

## Why this branch exists

A user reported that the KaiT2en Fedora installer USB does not boot on a
MacBook Pro 16,1 (2019, Core i7, Radeon Pro 5300M): the Fedora logo appears,
the screen goes black, the fans ramp up, and the machine powers off. Secure
Boot was disabled and external boot was allowed, so the failure is not a
firmware policy problem.

The user could not work around it, because the boot options cannot be edited
on a T2 Mac and no external keyboard was available.

## The internal keyboard is not the problem, the EFI input protocol is

The internal keyboard of a T2 Mac is not a real USB device. It is emulated by
the T2 and reaches the pre-boot environment through Apple's EFI console input.
Apple only implements the basic `EFI_SIMPLE_TEXT_INPUT_PROTOCOL`, which reports
plain characters but carries no modifier state and no function keys. The
extended `EFI_SIMPLE_TEXT_INPUT_EX_PROTOCOL`, which GRUB would need to see
`Ctrl-X` or `F10`, is not available.

Consequences:

- Letters, arrow keys and `Enter` work in the GRUB menu. Menu navigation and
  entry selection are fine.
- `e` opens the GRUB line editor, but neither `Ctrl-X` nor `F10` can leave it,
  so an edited entry can never be started. The editor is effectively a dead
  end on this hardware.
- An external USB keyboard would not change this. The limit is in the firmware
  protocol, not in the keyboard.
- `terminal_input console` in our `grub.cfg.in` is already the correct choice.
  GRUB's own USB HID stack (`usb_keyboard`) is not an option either, because
  the internal keyboard sits behind the T2 VHCI rather than a standard xHCI
  controller.

The single-character GRUB command line (`c`) does work, since it needs no
modifiers and `boot` is typed as a plain word. That is a viable manual escape
hatch, but far too error prone to document as the normal path.

## What this means for the installer design

Any boot option a user might need on a T2 Mac has to exist as its own menu
entry. There is no editing path. This is a hard design constraint for
`packaging/installer/grub.cfg.in`, not a nice-to-have.

Added four troubleshooting entries, all keeping the KaiT2en initramfs overlays
so the internal keyboard and trackpad survive into the live system:

1. boot messages only (`plymouth.enable=0`, no `quiet rhgb`) — a pure
   diagnostic, meant to fail visibly so the last console lines can be
   photographed.
2. `amdgpu` added to `module_blacklist` — best outcome on a dual-GPU MacBook
   Pro, since i915 keeps full acceleration.
3. `nomodeset` — firmware framebuffer plus software rendering, slow but enough
   to complete an installation.
4. `amdgpu` blacklisted, `nomodeset` and `pcie_ports=compat` — last resort,
   and the only entry aimed at a non-graphics failure class.

## Refactor detail: GRUB variables

The shared parts of the KaiT2en entries now live in `set` variables
(`kait2en_common`, `kait2en_blacklist`, `kait2en_initrd`) so six entries cannot
drift apart. GRUB expands variables in `linux`/`initrd` arguments and word
splits the result; Fedora's own BLS `grub.cfg` relies on exactly this with
`$kernelopts`, so the mechanism is proven.

Two ordering rules that matter:

- The variables must be set *after* the `search --set=fedora` line, because
  GRUB expands `$fedora` and `$kait2en_efi` at assignment time, not at use
  time.
- `${name}` braces are used throughout so `${kait2en_blacklist},amdgpu` parses
  unambiguously.

Verified by rendering the template and textually expanding the variables: the
`linux` and `initrd` lines of both pre-existing entries come out byte-identical
to the previous version. That was the main regression risk, because a mistake
there would break the default boot for every user.

## Root cause of the reported failure: confirmed as graphics

The reporter found an external keyboard, edited the boot options by hand and
booted successfully with `nomodeset` and `quiet` removed. That confirms the
failure is in the graphics handover and nowhere else.

The symptom pattern had already pointed there: the Fedora logo means Plymouth
ran, so kernel and initramfs were fine, and the black screen begins at the
handover from the firmware framebuffer to the real graphics driver. On a 16,1
the Radeon Pro 5300M (Navi14) is the boot GPU and drives the internal panel
through the mux, so an `amdgpu` modeset hang fits. The fans ramping and the
power off are the T2 noticing an unresponsive host and cutting power.

Do not read the fans alone as a thermal signal: the live system blacklists
`applesmc`/`macsmc`, so nothing regulates the fans there under any
circumstance. Only the combination with the power off is meaningful.

What is *not* explained is why this machine and not the others. Dedicated GPUs
are common across our T2 Macs and they boot fine, so this is one case so far,
not a known 16,1 or Navi14 trait. Do not write it up as one until a second
report or console output says otherwise.

Still open: whether blacklisting only `amdgpu` (troubleshooting entry 2) is
enough. That is the better outcome, because i915 then keeps full hardware
acceleration instead of falling back to software rendering. Worth asking the
reporter, since it costs them one reboot and decides what we recommend first.

## Journal of the successful `nomodeset` boot (2026-08-07)

The reporter sent the full journal of the working `nomodeset` boot: one boot,
~21 minutes, desktop reached, installer never started. It cannot show the crash
itself, but it pins down the hardware situation.

Facts from the journal:

- `MacBookPro16,1`, BIOS `2103.100.6.0.0`, iBridge `23.16.15067.0.0`, Fedora 44
  live on kernel `6.19.10-300.fc44`.
- dGPU `0000:03:00.0 [1002:7340]` (Navi14, Radeon Pro 5300M) sits behind the
  AMD-internal PCIe switch `01:00.0` / `02:00.0` on root port `00:01.0`.
  iGPU is `0000:00:02.0 [8086:3e9b]` (UHD 630).
- `vgaarb: setting as boot VGA device (overriding previous)` picks `03:00.0`
  over `00:02.0`, and `Relocating firmware framebuffer to offset 0x600000
  within [mem 0xb0000000-0xbfffffff]` puts the EFI framebuffer inside the dGPU
  BAR 0. **The internal panel is muxed to the dGPU on this machine**, so the
  black screen is exactly the moment `amdgpu` takes that framebuffer over.
- An ACPI `VFCT` table is present (~60 KB), so `amdgpu` can read the VBIOS.
  The failure is not a missing-VBIOS probe error; the machine really dies
  inside amdgpu init/modeset.
- `amdgpu 0000:03:00.0: probe with driver amdgpu failed with error -22` is just
  the `nomodeset` refusal and carries no information.
- VT-d is on (`iommu: Default domain type: Translated`) with an RMRR at
  `0x7b800000-0x7fffffff`. Untested as a factor.
- Everything except graphics is healthy: t2bce VHCI brings up the internal
  keyboard/trackpad, `kait2en-live-wifi` installs the `brcmfmac4364b3-pcie`
  `apple,bali` firmware after the expected first-try load failures and
  `wlp5s0` gets a lease, Bluetooth is UART (no BCM4377), no oops, no PCIe
  errors. A `Dell Secure Link Receiver` keyboard on `00:14.0-4.2` is the
  external keyboard used to edit the boot options.

### Consequence for troubleshooting entry 2

Because the panel hangs off the dGPU and `apple_gmux` is blacklisted in the
live system, nothing can move the mux at runtime. Blacklisting only `amdgpu`
therefore leaves the panel on simpledrm on the AMD BAR — visually the same as
`nomodeset`, with i915 loaded but unable to scan out. The docs claim entry 2
"leaves the Intel GPU fully accelerated"; that only holds for a Mac whose mux
already sits on the iGPU.

### The real fix for this class of machine

Setting `gpu-power-prefs` to the iGPU from macOS
(`sudo nvram fa4ce28d-b62f-4c99-9cc3-6815686e30f9:gpu-power-prefs=%01%00%00%00`)
makes the firmware boot on the Intel GPU. The EFI framebuffer then lands in the
i915 aperture, `amdgpu` is no longer in the display path, and the normal
KaiT2en entry can boot with full acceleration. This needs no new ISO and is
reversible with an NVRAM reset. Same variable the T2 GPU Control helper writes
after installation.

### Resolved by an NVRAM reset (2026-08-07, same day)

The reporter reset NVRAM on their own initiative and the **normal** KaiT2en
entry now boots: `quiet rhgb`, no `nomodeset`, same blacklist. `amdgpu` comes up
completely — VBIOS from VFCT (`ATOM BIOS: 113-D32207P1-020`), BAR 0 released
from the 256 MB window at `0xb0000000` and re-assigned to
`0x4100000000-0x41ffffffff` (4 GB rebar above 4G), `VRAM: 4080M` GDDR6,
`Display Core v3.2.359 initialized on DCN 2.0`, `Initialized amdgpu 3.64.0 on
minor 1`, `fbcon: amdgpudrmfb (fb0) is primary device`. i915 loads alongside it.

Important: the reset did **not** move the mux. `Relocating firmware framebuffer
to offset 0x600000 within [mem 0xb0000000-0xbfffffff]` and boot VGA `03:00.0`
are byte-identical to the earlier boot, so the display still hangs off the
dGPU. The `gpu-power-prefs` route was never taken; it remains a valid
alternative but is not what fixed this.

What the reset actually changed is **not visible from Linux**. PCI topology,
host bridge windows (including the `0x4000000000-0x7fffffffff` 64-bit window),
ACPI tables, the 76 EFI device properties on `03:00.0` and the firmware
framebuffer are identical across both boots. This is correlation, not proven
causation, and there never was a dmesg from a failing boot to diff against —
the earlier journal was the working `nomodeset` boot, where amdgpu never ran.

Four non-fatal WARNINGs fire within 1.7 ms at 9.537 s, all on CPU#7 in
`plymouthd/713` and all on the same call chain:

```
__x64_sys_close -> __fput -> drm_release -> drm_minor_release
                -> drm_managed_release -> drm_mode_config_cleanup
```

The device is `platform simple-framebuffer.0`, not amdgpu:

- `drm_mode_config.c:544`, followed by
  `[drm:drm_mode_config_cleanup] *ERROR* connector Unknown-1 leaked!`
- `drm_mode_config.c:578` — objects still registered at teardown
- `drm_gem_shmem_helper.c:197` — `drm_WARN_ON(refcount_read(&shmem->vmap_use_count))`
- `drm_gem_shmem_helper.c:209` — `drm_WARN_ON(refcount_read(&shmem->pages_pin_count))`

Cause: amdgpu binds late (`vgaarb: deactivate vga console` at 7.25 s, BAR
resize, firmware load, `fbcon: amdgpudrmfb (fb0) is primary device` at 9.53 s),
which gives plymouth roughly nine seconds to open simpledrm and map a buffer.
When amdgpu takes the aperture, simpledrm is unregistered while plymouthd still
holds the fd; the deferred `drm_managed_release` then runs on `close()` and
finds outstanding references.

This is generic upstream drm behaviour, not Mac- or amdgpu-specific, and not
caused by anything in this repo — none of our modules appear in the trace, they
only show up in the `OE` taint. WARN_ON, not BUG: the kernel continues, the
taint goes `G OE` -> `G W OE`, and nothing afterwards misbehaves. Do not read a
later `W` taint on this machine as a new problem.

Not worth acting on. `plymouth.enable=0` (troubleshooting entry 1) avoids it but
costs the splash; pulling amdgpu into the initramfs so plymouth never sees
simpledrm would also avoid it, but that is a real risk for a cosmetic warning.

Two amdgpu notes that matter after installation:

- `amdgpu: Runtime PM not available` — the dGPU cannot runtime-suspend, so it
  draws power continuously until T2 GPU Control's dGPU-off is in place.
- `[drm] Skipping amdgpu DM backlight registration` — panel brightness comes
  from gmux, and the live system has `apple_gmux` blacklisted with no `t2gmux`
  shipped, so brightness control is expected to be dead in the live session.

Everything else is healthy: nvme `p1 p2 p3`, t2hid keyboard/trackpad/Touch Bar
backlight, t2magicmouse, Wi-Fi via `kait2en-live-wifi` (BCM4364/4, firmware
`9.30.514.0.32.5.94`, `wlp5s0`), Bluetooth up. `brcmf_p2p_set_firmware ... -52`
is the usual harmless P2P complaint.

Consequence for support: an NVRAM reset is cheap, reversible and now has one
data point as a first-line step for this symptom. Worth putting ahead of the
troubleshooting entries in the docs.

### How to capture the actual amdgpu failure

The live system is RAM-only and the machine powers off, so no journal survives.
Two usable paths: troubleshooting entry 1 (`plymouth.enable=0`, no
`nomodeset`) and a photo of the last console lines — which may be empty if the
console is already cleared for the modeset — or booting the working
`nomodeset` entry and then forcing the driver from a terminal with
`sudo modprobe amdgpu modeset=1`. The `modeset=1` module parameter overrides
the `nomodeset` refusal in `amdgpu`'s probe, so the failure can be reproduced
with a shell (and ideally an SSH session from a second machine) still attached.

## Notes for later

- `set timeout=5` was deliberately left alone. Five seconds is tight for a user
  who has to pick a troubleshooting entry, and raising it would change the boot
  experience for everyone. Revisit if reporters say they miss the window.
- The new invariant in `tests/installer/static-check.sh` asserts that every
  entry using `${kait2en_common}` also loads `${kait2en_initrd}`. Verified that
  it actually fails when an entry drops the overlays and when the
  troubleshooting entries are removed — a check that cannot fail is worthless.
- The EFI partition free space check in `prepare-fedora-installer.sh` has a 1
  MiB slack on top of the measured file sizes, so the grown `grub.cfg` (roughly
  1.5 KB to 4 KB) is irrelevant there.
