# Testing the Apple Thunderbolt device-link patch series on Linux 7.2

Three patches received 2026-08-17 from Atharva Tiwari
(`atharvatiwarilinuxdev@gmail.com`) via Discord, to be tested on this
MacBookPro16,2 before he sends them upstream. A second dev asked that
`t2thunderbolt` be blacklisted during the test.

```
0001 thunderbolt: Shift tb_apple_add_links to pci.c
0002 thunderbolt: Add device links for Apple machines with Titan Ridge
0003 thunderbolt: Add device links for Apple systems with Icelake thunderbolt
```

## What the series does

On suspend the Thunderbolt controller is reset and every PCIe tunnel is lost.
`tb_probe()` therefore creates device links from the tunneled PCIe downstream
ports to the NHI, so the device core resumes the NHI *first* and the tunnels can
be re-established. Mainline had this for the old discrete Apple controllers
only; patch 1 moves the helper into `pci.c` (Mika Westerberg's request), patch 2
adds Titan Ridge, patch 3 adds Ice Lake.

## Only patch 3 is exercised on this machine

MacBookPro16,2 is Ice Lake:

```
00:07.0-3  Ice Lake TB3 PCI Express Root Port #0-3  8086:8a1d/8a1f/8a21/8a23
00:0d.2    Ice Lake TB3 NHI #0                      8086:8a17
00:0d.3    Ice Lake TB3 NHI #1                      8086:8a0d
```

Patch 3's hardcoded root-port IDs match exactly, and the ICL topology
assumption holds here: NHIs and root ports all sit directly on bus 0 with no
upstream port, which is why the discrete path (which walks up to a
`PCI_EXP_TYPE_UPSTREAM` bridge) cannot work for Ice Lake.

Verified in the built binary — `tb_pci_add_links` contains `0x8a17` and
`0x8a0d`, and `tb_pci_add_links_icl` was inlined into it:

```
objdump -d --disassemble=tb_pci_add_links drivers/thunderbolt/thunderbolt.ko
```

## A full kernel build really is required, and here is the proof

The patches touch `drivers/thunderbolt/pci.c`, which **does not exist** before
7.2 — the `nhi.c` split landed in 7.2. Fedora 7.1.8 cannot be patched at all,
not even for an out-of-tree module build. Confirmed without downloading any
source, by reading the dynamic-debug `__FILE__` strings out of the shipped
module:

```
strings thunderbolt.ko | grep -oE "drivers/thunderbolt/[a-z_0-9]+\.c" | sort -u
```

That lists `nhi.c`, `tb.c`, `acpi.c`, … and no `pci.c`. Useful trick in general
for answering "which source files is this distro module built from".

## Test result (2026-08-17, booted `7.2.0-tblinks`): patch 3 works

All eight links are created — both NHIs to all four root ports — and confirmed in
both sysfs directions (`supplier:pci:0000:00:0d.{2,3}` under each `00:07.x`,
`consumer:pci:0000:00:07.{0,1,2,3}` under each NHI):

```
thunderbolt 0000:00:0d.2: using software connection manager
thunderbolt 0000:00:0d.2: created link from 0000:00:07.0 .. 07.3
thunderbolt 0000:00:0d.3: created link from 0000:00:07.0 .. 07.3
```

The `device links to tunneled native ports are missing!` warning is gone (0
occurrences). No `WARNING:`/`BUG:`/hung task, no stall — the Ice Lake path does
not hit the locking problem described below. Both domains enumerate normally
(`domain0`, `domain1`, TMU negotiates to uni-directional LowRes).

Full log in `/var/log/tb-links-test.txt` (copy plus a trimmed
`dmesg-excerpt.txt` and the write-up `report-for-dev.md` in
`/home/alex/kernel-tb-test/`).

**`t2thunderbolt` kept coming back — three times.** Removed once after
`make install`, put back by the `dracut --regenerate-all` that fired another DKMS
autoinstall; removed again before the test; and put back a *third* time simply by
**booting the test kernel**, because Fedora's `dkms.service` runs an autoinstall
for the running kernel at every boot.

Two consequences worth remembering:

- Never trust a one-time `dkms remove` for a kernel you are about to boot. Check
  `dkms status -k <kver>` *after* the boot, and derive teardown lists from
  `dkms status` rather than hardcoding them.
- The `module_blacklist=` entry (below) stopped being cosmetic the moment the
  autoinstall rebuilt the module: it is what actually guarantees the driver
  cannot load, regardless of what DKMS puts back on disk.

### The two 7.2 module fixes verified at runtime, not just as a compile check

Booting the test kernel doubled as the real-hardware proof for
`fix/kernel-7.2-build`:

```
[drm] Initialized t2bdrm 1.0.0 for 5-6:2.1 on minor 0   -> card0-USB-1 present
t2bce_audio: module initialized                          -> "Apple T2 Audio" card 1
```

## Review findings worth sending back upstream

### 1. Patch 1 adds a lock that can self-deadlock

The moved helper gained `pci_lock_rescan_remove()` / `pci_unlock_rescan_remove()`
around the `for_each_pci_bridge()` loop. The original `tb_apple_add_links()` had
no such lock, and patch 1 claims to be a pure move.

`pciehp_configure_device()` takes exactly that mutex and calls
`pci_bus_add_devices()` underneath it, which attaches drivers synchronously:

```
drivers/pci/hotplug/pciehp_pci.c:39   pci_lock_rescan_remove()
drivers/pci/hotplug/pciehp_pci.c:72   pci_bus_add_devices(parent)
drivers/pci/hotplug/pciehp_pci.c:80   pci_unlock_rescan_remove()
```

So for any NHI that appears via pciehp, `nhi_pci_probe()` → `tb_probe()` →
`tb_pci_add_links_discrete()` runs *already holding* the mutex and would block
on itself. `pci_rescan_remove_lock` is a plain mutex, not recursive. The
discrete Apple controllers in patches 1 and 2 are precisely the ones that power
up and appear on device attach, so this is not a theoretical path for them.

Ice Lake is unaffected: the NHI is fixed on bus 0 and probes at boot. Patch 3
copies the same locking into `tb_pci_add_links_icl()`, so it inherits the
pattern without inheriting the risk.

Separately: a patch whose subject is "Shift … to pci.c" should not change
behaviour. Upstream will ask for the lock to be split into its own patch with
its own justification regardless of whether the deadlock argument holds.

### 2. We have measurement data the author can use

Patch 3 makes **both** NHIs link to **all four** root ports — eight links —
which is exactly what this repo's own `t2thunderbolt` already does on Ice Lake.
That means `c-states-learnings.md` already answers the obvious power objection:
the eight links are energetically **neutral** (PC8 21.50 % → 21.64 %, inside
noise, measured 2026-08-06 with a link-only A/B while the generic driver stayed
bound). The large PC8 regression seen when unloading the whole Thunderbolt stack
came from the generic driver / active NHI state, *not* from the links.

Also relevant: the `device links to tunneled native ports are missing!` warning
that patch 3 removes has been in every boot journal on this machine since
2026-07-17.

### 3. checkpatch is clean

`./scripts/checkpatch.pl --strict` reports 0 errors, 0 warnings, 0 checks on all
three patches.

## Build recipe that worked

Shallow clone of mainline, patches applied with `git am` (patches 2 and 3 only
apply *after* 1 — checking them individually against a pristine tree makes them
look broken), then Fedora's config as the base:

```
cp /boot/config-7.1.8-200.fc44.x86_64 .config
scripts/config --set-str LOCALVERSION "-tblinks" --disable LOCALVERSION_AUTO \
               --disable RUST --disable MODULE_SIG_ALL \
               --set-str SYSTEM_TRUSTED_KEYS "" --set-str SYSTEM_REVOCATION_KEYS ""
scripts/config --disable DEBUG_INFO_DWARF5 --disable DEBUG_INFO_DWARF4 \
               --disable DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT \
               --enable DEBUG_INFO_NONE --disable DEBUG_INFO_BTF
make olddefconfig
```

Notes that cost time:

- **`--enable DEBUG_INFO_NONE` alone does nothing.** It is a choice member, and
  `olddefconfig` keeps Fedora's `DEBUG_INFO_DWARF5=y`, which re-selects
  `DEBUG_INFO`. The DWARF options have to be disabled explicitly. Worth doing:
  installed module tree drops to 567 MB.
- `RUST=y` in Fedora's config drags in rustc/bindgen; disabling it avoids the
  whole toolchain question.
- Secure Boot is off on this machine, so an unsigned kernel and unsigned
  out-of-tree modules just work. `MODULE_SIG=y` with `MODULE_SIG_ALL=n` needs no
  signing key.
- **`.scmversion` no longer exists in 7.2's `scripts/setlocalversion`.** With
  three commits on top of the `v7.2` tag (and no tags at all in a shallow
  clone), the release string became `7.2.0-tblinks+`, and with the patches left
  uncommitted `7.2.0-tblinks-dirty`. A `+` in a kernel release leaks into DKMS
  paths, so the fix was to move `.git` aside for the build — then
  `setlocalversion` finds no SCM and the release is a clean `7.2.0-tblinks`.

## Test method

`thunderbolt` is persistently disabled on this machine as a power optimisation
(`install thunderbolt /bin/false` plus dracut `omit_drivers`). Do **not** edit
that config to test — it is bypassed per-invocation instead:

```
modprobe --ignore-install thunderbolt dyndbg=+p
```

`--ignore-install` skips the `install … /bin/false` rule, and `blacklist` only
affects alias resolution, never an explicit module name. Verified with
`modprobe -n -v` before rebooting: plain modprobe resolves to `install /bin/false`,
with the flag it resolves to a real `insmod`.

### Make the t2thunderbolt exclusion *visible in the dmesg you hand over*

`/etc/modprobe.d` rules leave no trace in dmesg, so a reviewer cannot tell from
the log that the out-of-tree driver was kept out. The kernel's own
`module_blacklist=` does appear, in the `Kernel command line:` line that every
dmesg starts with. So `t2thunderbolt` was appended to `module_blacklist=` in the
test kernel's BLS entry only — functionally redundant (it isn't even installed
for that kernel), but it turns "trust me" into evidence. Worth doing for any
patch test where a reviewer needs to rule out interference from another driver.

Note the asymmetry: `module_blacklist=` is enforced in the kernel and cannot be
bypassed by `modprobe --ignore-install`, so the driver under test must **never**
go in there — only the ones that must stay out.

`dyndbg=+p` matters: the success path logs `created link from %s` via `dev_dbg`,
which produces nothing without dynamic debug enabled. Only the *failure* path
uses `dev_warn`. Without `dyndbg` a working patch looks like a silent no-op.

## Keeping the daily system untouched

- `dkms install <mod>/<ver> -k 7.2.0-tblinks` only ever writes to
  `/var/lib/dkms/<mod>/<ver>/7.2.0-tblinks/`. `/usr/src` is never modified, so
  the experimental SG stack under 7.1.8 is unaffected.
- Do **not** run `scripts/fedora/install-dkms-modules.sh` for this. It removes
  every module for every kernel and re-copies `/usr/src` from the repo working
  tree, which would destroy whichever branch state is currently installed.
- **`make install` triggers a DKMS autoinstall** via `kernel-install`, which
  happily installed `t2thunderbolt` for the new kernel behind our back. It has
  to be removed again explicitly (`dkms remove t2thunderbolt/0.5 -k …`).
- **`make install` also rewrites `saved_entry`** to the new kernel. Reset it and
  use one-shot booting instead:
  `grub2-editenv - set next_entry=<machine-id>-7.2.0-tblinks`. Fedora's
  `grub.cfg` consumes and clears `next_entry`, so a failed test kernel needs
  nothing but a power cycle to get back to Fedora.
- Every `dkms install`/`dkms remove` fires DKMS's `post_transaction`, i.e.
  `dracut --regenerate-all`, which rebuilds *every* initramfs on the box. Nine
  module operations means nine full runs. Suppress it with a temporary
  `post_transaction=""` drop-in under `/etc/dkms/framework.conf.d/`, and never
  kill a running dracut — a truncated initramfs is much worse than waiting.

## Undoing the teardown: snapper saved the test setup

The devs asked for follow-up tests *after* `restore.sh` had already run. The
setup was fully recoverable from a snapper timeline snapshot, without rebuilding
the kernel. Worth knowing for the next time a teardown turns out to be
premature.

**What snapper covers here and what it does not.** `/` and `/home` are btrfs
subvolumes with hourly timeline snapshots. `/boot` is a separate **ext4**
partition (`/dev/nvme0n1p3`) and is therefore *not* snapshotted at all. So a
snapshot restores the module tree, the DKMS state and the kernel build tree —
but never `vmlinuz`, `initramfs`, `System.map` or the BLS entry. Those have to
be reconstructed:

- `vmlinuz-<kver>` is byte-identical to `arch/x86/boot/bzImage` in the build tree
- `initramfs` is regenerated with `dracut --force /boot/initramfs-<kver>.img <kver>`
- the BLS entry comes from `kernel-tb-test/bls-entry.conf.bak`

**Only one snapshot had everything.** The build tree is deleted by the cleanup
step, and the module tree only exists between `modules_install` and the
teardown, so the useful window is narrow — here a single snapshot (14:00) held
both. Check `/home/.snapshots/<n>/snapshot/...` and `/.snapshots/<n>/snapshot/...`
separately; they are independent configs with independent numbering that happens
to line up.

**Freeze the snapshot before doing anything else.** `TIMELINE_LIMIT_HOURLY="6"`
means the useful snapshot is minutes away from being pruned by
`snapper-cleanup.timer`. `snapper -c <config> modify -c "" <n>` drops its cleanup
algorithm and takes a second.

**Copy with `--reflink`.** Snapshots and the live subvolume are on the same btrfs
filesystem, so `cp -a --reflink=auto` is a metadata-only copy: 5.7 GB of kernel
build tree in 14 seconds and no additional disk usage. `cp -a` also carries the
SELinux context; `restorecon` afterwards is still worth it for `/boot`.

**Restoring `/lib/modules` needs a `depmod -a <kver>` afterwards** — the copied
`modules.dep` refers to the right paths, but regenerating is cheap insurance.

**Verify you restored the *patched* kernel, not just any build.** `thunderbolt`
is built as a module here, so the patch strings are in
`kernel/drivers/thunderbolt/thunderbolt.ko`, not in `vmlinux`:
`strings thunderbolt.ko | grep "created link from"` must return two hits (the
discrete and the Ice Lake path). Note the patches rename the function to
`tb_pci_add_links` — grepping the source for the old `tb_apple_add_links` from
the patch subject line finds nothing and looks alarming for no reason.

**DKMS reports `built`, not `installed`, after such a restore.** The `.ko` files
are in `extra/` and `depmod` sees them, which is all that matters for booting.
`t2thunderbolt` comes back with the restored DKMS state — that is faithful to
how the test actually ran, and `module_blacklist=…,t2thunderbolt` is what keeps
it out of the kernel either way.

## Scenario 2: thunderbolt loaded at boot, then suspend/resume

The patch authors followed up with a second, narrower request: boot with
`thunderbolt` loaded **during boot** (not modprobed afterwards) and with debug
output on, do one suspend/resume cycle, hand over the dmesg. Nothing else.

**The blocking config is global, the test kernel is not.** Both
`/etc/modprobe.d/kait2en-disable-thunderbolt.conf` and
`/etc/dracut.conf.d/90-kait2en-disable-thunderbolt.conf` apply to every kernel on
the box, so simply lifting the block would also un-disable Thunderbolt for the
daily 7.1.8 system and wreck its PC8 residency. The way out is to swap which
mechanism does the blocking, per kernel:

- lift the block in `modprobe.d` for `thunderbolt` (keep `t2thunderbolt`), so
  udev can autoload it during boot on the test kernel, and
- add `thunderbolt,t2thunderbolt` to `module_blacklist=` on the **Fedora** BLS
  entries, which is enforced in-kernel and cannot be bypassed.

Net effect: the daily kernels are protected more strongly than before, and only
`7.2.0-tblinks` can load the driver. Order matters — add the kernel-level block
to the Fedora entries *before* lifting the modprobe block, never the other way
round.

**`install … /bin/false` cannot be lifted per kernel, `module_blacklist=` cannot
be lifted at all.** That asymmetry is exactly what makes the split above work,
and it is also why the driver under test must never appear in
`module_blacklist=`.

**Debug output needs `thunderbolt.dyndbg=+p` on the cmdline**, not a modprobe
parameter, because the module is loaded by udev/initramfs and not by hand. Also
worth adding `log_buf_len=8M`: a full boot plus suspend/resume with dyndbg on is
verbose enough to risk losing early lines from the ring buffer.

**Regenerating only one initramfs**: move the dracut drop-in aside, put a
temporary one in its place, run `dracut --force /boot/initramfs-<kver>.img
<kver>`, and leave the temporary file until teardown. The Fedora images are only
rewritten when something regenerates *them*, so they keep the old omit list.
Watch out for DKMS's `post_transaction` (`dracut --regenerate-all`) — after the
next boot into the test kernel it would strip `thunderbolt` back out of the test
image, so the test boot has to be the one right after the rebuild.

**Prove the load really happened at boot.** The test script rejects the run if
the first thunderbolt line in dmesg is later than 120 s, which is what a manual
modprobe would look like (scenario 1 produced its first line at 222 s and the
timestamp jump immediately drew questions from the devs).

## Parking the test material with an expiry date

snapper 0.13 has no native per-snapshot TTL. Its only cleanup algorithms are
`number` (count), `timeline` (hourly/daily/weekly buckets) and
`empty-pre-post`; the sole time-related config keys are `NUMBER_MIN_AGE`,
`TIMELINE_MIN_AGE` and `EMPTY_PRE_POST_MIN_AGE`, which set a *minimum* age
before a snapshot becomes eligible for deletion -- the opposite of an expiry.
So "delete this snapshot on date X" has to come from outside snapper.

What was set up on 2026-08-18:

- `root/299` (frozen, `cleanup` cleared) holds
  `/usr/lib/modules/7.2.0-tblinks` with all 14 `extra` modules.
- `home/307` (frozen) holds `kernel-tb-test/` including the 5.7 GB build tree,
  the `k72`/`sg` worktrees and the scenario-2 scripts.
- Both carry userdata `expires=2026-09-18,purpose=thunderbolt-devlinks-test`.
- `kernel-tb-test-expire.timer` (`OnCalendar=2026-09-18 11:00`,
  `Persistent=true`) runs a script that deletes every snapshot carrying that
  userdata marker and then removes its own units and itself.

Two things worth keeping from that:

- Match snapshots by **userdata marker, not by number**. Numbers are stable in
  snapper, but a marker survives a re-creation of the snapshot and documents
  itself in `snapper list`.
- Make the self-removal **conditional**. A first version removed its units
  unconditionally; if `snapper delete` had failed, the frozen snapshots would
  have silently outlived their janitor. The script now re-lists both configs
  and only removes itself when nothing with the marker is left, otherwise it
  exits non-zero so the unit shows up in `systemctl --failed`.

The mechanism was verified end-to-end, not assumed: a `sed`-substituted copy of
the script with marker `purpose=expire-selftest`, its own throwaway units and
two throwaway snapshots. Firing it deleted both snapshots and left no unit
file, no `timers.target.wants` symlink and no failed unit behind.

Note that snapshotting does not free space at cleanup time -- the 6 GB stay
allocated until `home/307` is deleted. Removing the live copy only tidies the
directory.
