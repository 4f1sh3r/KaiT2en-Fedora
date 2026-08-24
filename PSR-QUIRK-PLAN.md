# Plan: enable eDP PSR by lifting the Apple `DP_DPCD_QUIRK_NO_PSR`

**Status: DONE 2026-08-23 - negative result, do not re-run.** Executed in
full. The quirk patch works and PSR saves 0.469 W, but the panel never
acknowledges PSR entry and the screen goes black. Six configurations were
ruled out live via MMIO/DPCD. The stock module has been restored. The full
write-up is in `c-states-learnings.md`, section *"PSR quirk lifted and
measured: works on the source, dead on the panel (2026-08-23)"* - read that
before touching this topic again. Two corrections to this plan are recorded
there: there is a second VBT gate beyond the quirk, and the GRUB escape hatch
in section 2.3 does not exist on T2 Macs.

**This file stays untracked**, like `c-states-learnings.md` and the
`*-LEARNINGS.md` documents. Never `git add` it.

---

## 0. What this is about

The internal eDP panel advertises PSR1, but Linux refuses to use it because of
a blanket quirk on the Apple OUI in the DRM core. Without PSR the display
engine scans out 2560x1600 XR30 from memory continuously, which pins package
C-state PC2 at ~40 % and keeps `DC5 -> DC6` at 0 since boot.

Full evidence is in `c-states-learnings.md`, section
*"PSR is blocked by a DRM quirk, not by the panel (2026-08-23)"*. Read that
section before starting — it has the DPCD dump and the decoded quirk table.

Short version:

| DPCD | value | meaning |
|---|---|---|
| `0x700` eDP_DPCD_REV | `0x02` | eDP 1.3 |
| `0x070` PSR_SUPPORT | `0x01` | `DP_PSR_IS_SUPPORTED` (PSR1) |
| `0x071` PSR_CAPS | `0x01` | `DP_PSR_NO_TRAIN_ON_EXIT` |
| `0x701` EDP_GENERAL_CAP_1 | `0x83` | bit 7 `DP_EDP_SET_POWER_CAP` set |
| `0x400` SINK_OUI | `00:10:fa` | Apple Inc. |

`_psr_init_dpcd()` in `intel_psr.c` has only two gates after `psr_dpcd[0] != 0`:
the `DP_DPCD_QUIRK_NO_PSR` quirk and the `DP_EDP_SET_POWER_CAP` bit. Bit 7 is
set, so the quirk is the sole cause.

**Goal of the experiment:** find out whether this specific panel actually
works with standard PSR1, and what it is worth in watts.

**Expected payoff:** unquantified, estimated ~0.7-1.0 W. That is an order of
magnitude more than the 0.02 W that PC6 -> PC8 turned out to be worth. It is
an estimate, not a measurement. PSR does not touch the backlight.

---

## 1. Preconditions (verified 2026-08-23 — re-verify cheaply, do not re-derive)

| fact | value | why it matters |
|---|---|---|
| `CONFIG_MODVERSIONS` | **not set** | no symbol-CRC matching, so a rebuilt module loads next to Fedora-built `i915` without ABI games. This is the single biggest simplification — do not build a whole kernel. |
| `CONFIG_MODULE_SIG_FORCE` | not set | |
| Secure Boot | **disabled** (`mokutil --sb-state`) | no signing needed |
| `CONFIG_DRM_DISPLAY_HELPER` | `m` | the quirk table lives in a replaceable module, not in vmlinux |
| `CONFIG_DRM_DISPLAY_DP_HELPER` | `y` | `drm_dp_helper.c` is compiled *into* `drm_display_helper.ko` |
| running kernel | `7.1.9-200.fc44.x86_64` | |
| vermagic | `7.1.9-200.fc44.x86_64 SMP preempt mod_unload` | the rebuilt module must match this exactly |
| module path | `/lib/modules/7.1.9-200.fc44.x86_64/kernel/drivers/gpu/drm/display/drm_display_helper.ko.xz` | |
| original md5 | `e2c266589cbe8c3bbd81c0431b8f2ae7` | back this file up before touching anything |
| **module is in the initramfs** | yes | `dracut -f` is mandatory, otherwise the old module loads early and wins |
| other kernels installed | `7.1.8-200`, `7.1.7-200` | **the rollback path** — leave their module trees pristine |
| toolchain | gcc 16.2.1 (kernel was built with 16.1.1) | only emits a compiler-mismatch warning; harmless here |
| build resources | 8 cores, 15 GB RAM, 127 GB free | plenty |

---

## 2. Safety and rollback — read before step 3

The failure mode is **a broken or black display on the only machine the user
has**, with Wi-Fi as the only link. Plan for that up front.

1. **Touch only the 7.1.9 module tree.** Kernels 7.1.8 and 7.1.7 stay
   untouched. If 7.1.9 boots to a broken display, the user picks 7.1.8 in the
   GRUB menu and everything is normal again. This is the primary rollback and
   it needs no typing in the dark.
2. **Back up the original module** before overwriting:
   `cp -a <path>.ko.xz /home/alex/psr-quirk-backup/` (also back up the
   initramfs image).
3. **Escape hatch on the boot line:** `i915.enable_psr=0` forces PSR off even
   with the quirk lifted. Do **not** put it into the permanent kernel args —
   it is what the user types in GRUB if needed. Tell the user this string
   before the reboot.
4. **Do not** modify `scripts/fedora/install-kernel-args.sh` or the permanent
   cmdline as part of this experiment.
5. Warn the user that a `dnf update` reinstalling `kernel-7.1.9` would silently
   restore the stock module.

Brief the user on 1 + 3 in plain German **before** asking them to reboot.

---

## 3. Implementation

### 3.1 Get the matching source

Only `drivers/gpu/drm/display/` is needed. Two routes, cheapest first:

**Route A (preferred, minutes):** upstream tarball for the matching version.
Fedora's `7.1.9-200.fc44` is upstream 7.1.9 plus Fedora patches; the DP helper
is very unlikely to be patched. Fetch `linux-7.1.9.tar.xz` from kernel.org,
extract only `linux-7.1.9/drivers/gpu/drm/display/`.

**Verify the assumption instead of trusting it:** build the *unmodified* source
first (step 3.3) and compare the resulting `.ko` against the shipped one —
same exported symbols, same section sizes, similar text size. If it diverges
noticeably, fall back to Route B.

**Route B (authoritative, slower):** the Fedora SRPM.

```
dnf download --source kernel --enablerepo=fedora-source,updates-source
rpm -i kernel-7.1.9-200.fc44.src.rpm
rpmbuild -bp ~/rpmbuild/SPECS/kernel.spec --target=x86_64
```

Then use `~/rpmbuild/BUILD/kernel-*/linux-7.1.9-200.fc44.x86_64/drivers/gpu/drm/display/`.

Work in the scratchpad or under `/home/alex/psr-quirk-build/`, **not** inside
the repo working tree.

### 3.2 The change

File: `drivers/gpu/drm/display/drm_dp_helper.c`, table `dpcd_quirk_list[]`.

```c
	/* Apple panels need some additional handling to support PSR */
	{ OUI(0x00, 0x10, 0xfa), DEVICE_ID_ANY, false, BIT(DP_DPCD_QUIRK_NO_PSR) },
```

Make it the most surgical edit possible — set the quirk mask to `0`:

```c
	{ OUI(0x00, 0x10, 0xfa), DEVICE_ID_ANY, false, 0 },
```

Leave the entry in place so the table layout and every other entry stay
identical. Do not touch the second Apple entry (device id `65 44 15 65 62 61`,
`CAN_DO_MAX_LINK_RATE_3_24_GBPS`) — this panel's device id is
`65 44 13 67 64 61` and does not match it anyway.

Keep a `.patch` of the diff next to the build dir so it can be reapplied to the
next kernel.

### 3.3 Build

Out-of-tree against the installed kernel-devel tree, the same pattern the repo
already uses (see `KERNEL-7.2-LEARNINGS.md`):

```
make -C /usr/src/kernels/7.1.9-200.fc44.x86_64 M=<display-dir> modules
```

`drivers/` is not shipped in `kernel-devel`, which is why the source subtree is
copied in. Replicate the in-tree `Makefile`/`Kbuild` from the source tree; the
public headers come from `kernel-devel`'s `include/`. Expect
`warning: the compiler differs from the one used to build the kernel` —
harmless.

**Check before installing:**

```
modinfo <built>.ko | grep vermagic     # must equal the table in section 1
```

If vermagic differs, stop and fix it — do not install.

### 3.4 Install

```
xz -9 -c <built>.ko > drm_display_helper.ko.xz
# back up first, then replace the file at the path in section 1
depmod -a 7.1.9-200.fc44.x86_64
dracut -f --kver 7.1.9-200.fc44.x86_64      # MANDATORY, module is in initramfs
lsinitrd /boot/initramfs-7.1.9-200.fc44.x86_64.img | grep drm_display_helper
```

The repo has `scripts/fedora/rebuild-initramfs.sh` — check whether it fits
before hand-rolling `dracut`.

Confirm the new module is really the one in the initramfs (compare sizes or
extract and `modinfo` it). A stale initramfs is the most likely reason for a
"nothing changed" result.

### 3.5 Reboot

Reboot is required — the quirk is evaluated once at connector init.

Before asking the user to reboot, tell them:
- what to look for (does the desktop come up at all, any flicker or artefacts
  on static content),
- that `i915.enable_psr=0` typed into the GRUB boot line disables it,
- that picking kernel 7.1.8 in the GRUB menu is the full rollback.

---

## 4. Verification after reboot (do this before asking the user to judge)

Let `D=/sys/kernel/debug/dri/0000:00:02.0` (needs sudo).

1. `cat $D/eDP-1/i915_psr_status`
   - before: `Sink support: PSR = no`
   - **success: `Sink support: PSR = yes`** plus PSR mode/state lines
2. `cat $D/i915_edp_psr_status` — previously `ENODEV`, should now report state.
3. `cat $D/i915_dmc_info` — `DC5 -> DC6 count` should start incrementing on an
   idle desktop. It has been 0 since boot for the entire investigation.
4. `sudo dmesg | grep -i psr` — look for
   `eDP panel supports PSR version 1` and any PSR error/timeout warnings.
5. Check that `drm_display_helper` is the new build:
   `md5sum` of the installed file vs. the backup, and `lsmod | grep drm_display_helper`.

If (1) still says `no`, the module did not take effect — suspect the initramfs
(step 3.4) before suspecting the patch.

---

## 5. Measurement

**The good news: this does not need two boots.** Once the quirk is lifted, PSR
can be toggled at runtime through `$D/i915_edp_psr_debug`:

```
echo 1 | sudo tee $D/i915_edp_psr_debug   # I915_PSR_DEBUG_DISABLE
echo 0 | sudo tee $D/i915_edp_psr_debug   # I915_PSR_DEBUG_DEFAULT (re-enable)
```

Verify the toggle actually moves `i915_psr_status` before trusting it.

Use the established methodology from `c-states-learnings.md` — do not invent a
new one:

- **battery gauge is the primary metric**
  (`/sys/class/power_supply/BAT0/{current_now,voltage_now}`, W = I x V),
  not package RAPL. RAPL misses most of what changes here.
- **pin the backlight** (rewrite `/sys/class/backlight/intel_backlight/brightness`
  every 2 s). GNOME's auto-dim invalidated a whole measurement round once.
- **interleaved A/B/A/B/A/B**, 90 s windows, never a single pair.
- record `turbostat` PC2/PC6/PC8, `Any%C0`, IRQ counts and `DC5 -> DC6` per
  window, so a CPU-load confound is visible.
- run it detached and let the machine sit idle; the user leaves it alone.
- **restore everything** on EXIT/INT/TERM traps.

Existing scripts to crib from: `c-states-measure.sh` in the repo, and the
round-3/round-6 scripts under the previous session's scratchpad if still
present.

Note the confound that ruined the naive reading last time: the display-off
window also had the backlight off, so 7.588 -> 4.640 W is **not** a display
engine number. PSR keeps the backlight on, so this measurement is the clean one.

Also worth capturing: whether PSR actually stays *entered* on an idle GNOME
desktop, or whether the compositor's frontbuffer activity keeps kicking it out.
`i915_psr_status` shows the current state; sample it over a window rather than
reading it once.

---

## 6. Failure modes

| symptom | meaning | action |
|---|---|---|
| `Sink support: PSR = no` after reboot | old module still active | check initramfs, `modinfo` the installed file |
| flicker / artefacts on static content | the quirk was justified for this panel | this is a **valid negative result** — record it, revert |
| black screen at boot | i915 broken or PSR fatal | user boots 7.1.8 from GRUB |
| PSR enters but never stays | compositor frontbuffer activity | not a failure, but the payoff will be small — measure it anyway |
| module refuses to load | vermagic mismatch | rebuild; do not force |

A negative result here is genuinely useful. Do not massage the outcome — if
the panel misbehaves, that is the answer, and the quirk was right.

---

## 7. When done

- Report to the user in German: does PSR engage, what does it cost/save in
  watts, and any visual artefacts observed.
- Write the outcome into `c-states-learnings.md` in **English**, in the same
  style as the existing sections — including a negative result.
- If it works and is worth keeping: **ask the user** before adding anything to
  `local/rig`. Nothing goes into the rig automatically (`CLAUDE.md`). A kernel
  module override is not a DKMS module and does not belong in
  `install-dkms-modules.sh` without a discussion about how it survives kernel
  updates.
- If it is worth upstreaming, the honest framing for a patch would be narrowing
  the wildcard to the device ids that actually misbehave — but that needs data
  from more than one machine, so it is not a same-day PR.

---

## 8. Hard constraints (from `CLAUDE.md` and prior feedback)

- Never `git add` this file or any `*-LEARNINGS.md` / `c-states-learnings.md`.
- Never commit to `main`. This work does not belong on `main` anyway.
- Nothing enters `local/rig` without explicitly asking the user first.
- Commit messages: subject line only, no body, never AI-sounding, no
  `Co-Authored-By`.
- Learnings/documentation in English; talking to the user in German.
- Do not "fix" Thunderbolt — it is disabled on purpose.
- Wi-Fi is this machine's only link.
- The user does not know hardware internals and only reports back observed
  behaviour. Tell them concretely what to look at.
