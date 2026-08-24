# Live USB rescue console (feat/live-rescue)

Branch: `feat/live-rescue`, pushed to `origin` (4f1sh3r fork), commit `1de1fc3`.
Not merged into `local/rig` — needs the user's explicit go-ahead first.

## Why this exists

The failure class it targets: a kernel update where the DKMS build fails or the
initramfs loses the T2 input modules. After that there is no keyboard, not even
at the LUKS prompt, so the live stick is the only way back in. Nothing in the
repo could touch an installed system from the live session before this.

The Project Owner originally asked for auto-chroot *plus* a backup script hooked
into kernel updates *plus* restore-from-live. The backup half was dropped on
2026-08-21 ("should be a user responsibility thing actually"), so only the
chroot/rescue half was built. `RESCUE-BACKUP-PLAN.md` still holds the full
two-phase plan if the backup half is ever revived.

## What was built

- `packaging/installer/runtime/kait2en-rescue` — detection, LUKS unlock,
  fstab-driven mounting, chroot, three actions (shell, rebuild initramfs +
  verify input drivers, show/set the default boot entry).
- `packaging/installer/runtime/kait2en-rescue.service` — gated on
  `ConditionKernelCommandLine=kait2en.rescue`, so a normal live boot never
  starts it.
- 7th GRUB entry `Rescue: repair an installed KaiT2en system`, booting
  `systemd.unit=multi-user.target kait2en.rescue`.
- `scripts/tests/rescue.sh` plus static-check invariants.

## Learnings

- **The btrfs subvolume name cannot be assumed.** Fedora uses `root`, other
  layouts use `@` (see `INSTALLER-BLS-DEFAULT-LEARNINGS.md`). The tool mounts
  the file system read-only, checks for `etc/os-release` with `ID=fedora` at the
  top level, and only then walks `btrfs subvolume list -o` and probes each one.

- **Mount everything else from the target's own fstab**, never from a guess.
  `findmnt --fstab -F "$target/etc/fstab" -no TARGET,SOURCE,FSTYPE,OPTIONS |
  sort` handles both `/boot` layouts (own partition or a directory inside the
  root subvolume) without special cases. Sorting by target string is enough to
  get `/boot` before `/boot/efi`. Entries whose target is not absolute (swap
  uses `none`) and network file systems are skipped.

- **fstab options can fail in a live session** (systemd-specific or network
  options). Mounting with the fstab options and falling back to a plain mount on
  failure is two lines and avoids parsing them.

- **APFS must be skipped by file system type, not by partition number.** The
  skip list is `apfs hfsplus hfs ntfs vfat swap iso9660 squashfs`, so the macOS
  partition is never even probed. The test asserts it never reaches `mount`.

- **The live medium has to be excluded explicitly.** A KaiT2en stick carries a
  Fedora root itself and would otherwise show up as a repair target. The tool
  resolves `findmnt -no SOURCE /run/initramfs/live`, takes its `PKNAME`, and
  drops every device whose path starts with that disk.

- **`grubby --set-default-index=` only.** `--set-default <path>` fails on
  `/@/boot/...` layouts — same trap as in `kait2en-prepare`. static-check
  enforces this with a positive and a negative grep.

- **efivars must be bind-mounted rw**, otherwise `grubby`/`efibootmgr` inside
  the chroot silently cannot write.

- **The firmware guard in `90-kait2en-updates.sh` is not worth working around.**
  The live helpers are copied to `/run` only inside `[ -d "$firmware_source" ]`.
  That looks like it would leave a firmware-less stick without a rescue console,
  but such a stick cannot be built: `pick_one` in
  `scripts/macos/prepare-fedora-installer.sh` calls `die` unless exactly one of
  each Wi-Fi firmware file is found, so the build aborts long before writing the
  medium. The guard is always true. An earlier version of this branch hoisted
  both loops out of the guard for that non-existent case and was reverted; the
  helper and the unit are now simply two more names in the existing lists.

- **`KAIT2EN_TEST_MODE=1` bypasses the root check**, following the precedent in
  `kait2en-prepare`. Without it the test suite cannot run the script at all.

- Every external command is called by bare name, so the test can fake `lsblk`,
  `findmnt`, `mount`, `umount`, `btrfs`, `cryptsetup`, and `chroot` on `PATH`.
  The fake `mount` copies a prepared directory tree over the mount point, which
  is what makes the detection logic testable without root and without touching a
  real disk.

## Not verified yet

Everything below needs a freshly built stick and a real boot; it could not be
tested on the running machine:

1. The rescue GRUB entry comes up as a text console **and the internal keyboard
   works** — that is the real regression risk, and the proof that the input
   initramfs overlays are loaded in the new entry.
2. Detection on the real disk: the APFS partition must not appear.
3. LUKS unlock on the real encrypted root (the test only covers the
   "no terminal, so no unlock" path — an interactive unlock needs a tty).
4. `findmnt` on exit must show nothing left mounted.

## The rescue console does not belong on a text console

Tested on real hardware on 2026-08-21 with release `v1.11.0-live-rescue`. The
dedicated GRUB entry booted, the internal keyboard worked and the tool found,
unlocked and mounted the installation correctly — but the text console is
unusable on this MacBook Pro's Retina panel. The kernel console reports more
rows than the panel actually displays, so everything scrolls out of the visible
area: the menu was already on screen while nothing could be seen, and typed
input only became visible after five or six presses of Enter.

Fixing that would mean guessing at `video=`/`fbcon=` parameters, and the right
value differs per panel and per T2 model. Hardcoded into a GRUB entry it would
break other machines instead.

The entry and its `kait2en-rescue.service` were therefore removed. The
script itself is unchanged and is still copied to `/run/kait2en/` by
`90-kait2en-updates.sh`, so it runs from a terminal in the live desktop:

    sudo /run/kait2en/kait2en-rescue

That path is better anyway: readable text, scrollback, plus Wi-Fi and a browser
for troubleshooting. It is also the only approach that is identical across
GNOME, KDE and Cosmic — a `.desktop` launcher would have to know the terminal
emulator, which is `ptyxis`, `konsole` and `cosmic-term` respectively.

What is lost is the text-mode fallback for a machine where the live desktop
itself will not start. That is a small loss: the four Troubleshooting entries
already cover the graphics cases, and the fallback was demonstrably unusable.

## Putting the rescue tool on PATH

`/run/kait2en` is in no PATH, and `sudo` ignores the caller's PATH entirely — it
uses `secure_path` from `/etc/sudoers`. So `sudo kait2en-rescue` needed a
symlink in a directory that is in `secure_path` on every Fedora: `/usr/bin`.
`/usr/local/bin` was rejected because whether it is part of `secure_path` varies
between distributions and local edits.

The link is created by `90-kait2en-updates.sh`, the same dracut pre-pivot hook
that copies the helpers, pointing at `/run/kait2en/kait2en-rescue`. It
writes to `/sysroot/usr/bin`, i.e. the live overlay before the switch root, and
is guarded by `[ -d /sysroot/usr/bin ]` plus a `warn` so a failure can never
break the boot. Nothing of this reaches an installed system: the live root is an
overlay that is discarded on reboot, and the hook only ever writes to `/run` and
`/sysroot`.

Precedent that the live `/usr` is writable: `kait2en-live-wifi` calls
`install-wifi-firmware.sh --root /`, which installs into
`/usr/lib/firmware/brcm` of the running live session.

Still unverified on hardware: whether `/sysroot` is already mounted rw when the
pre-pivot hook runs. If it is not, the guard skips the link silently and the
full path `sudo /run/kait2en/kait2en-rescue` still works — which is why the
documentation names it as the fallback.
