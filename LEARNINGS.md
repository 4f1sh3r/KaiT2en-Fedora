# t2bce / Touch Bar Suspend-Resume Learnings

Status: 2026-07-04. Branch `codex/t2bce-resume-port-state-gating`.

This file replaces `T2BCE-LEARNINGS.md` (now untracked/ignored, kept locally
for historical reference). It is the working notebook for the ongoing
`t2bce`/`t2bdrm`/`t2touchbar_kbd` suspend-resume and Touch Bar debugging
effort. Update it after every test cycle on `mac-t2` so we never have to
re-derive what was already tried.

## Current baseline

- Loaded module: `t2bce` srcversion `D975F4A2DA35CC283AC039A` (matches commit
  `d62bec5`, the "touchbar mode milestone"). Kernel `7.0.14-201.fc44.x86_64`.
- `t2bdrm` is **not** loaded in this test configuration; Touch Bar runs native
  Config 1 through `t2touchbar_kbd`.
- Working: Touch Bar visible and responsive, Fn switches graphics between
  F-keys and the standard view (brightness/media/volume), keyboard/trackpad,
  suspend entry.
- **Fixed 2026-07-04 (commit `0939182`)**: the ~30-55s real resume delay is
  gone. `PM: resume devices took 0.805 seconds` confirmed on hardware, Touch
  Bar visible immediately post-resume (no more manual Fn press needed to wake
  it), keyboard/trackpad normal. See the dated section below for the full
  root-cause writeup — this closes out essentially all of the "Open next
  steps" and open hypotheses from the sections below it, which are kept for
  historical context but are superseded.
- Still open, pre-existing, low-impact: Port 5 (Topcase/keyboard/trackpad)
  still needs one real usbcore-triggered reset + re-enumeration per resume
  cycle (`raw=0x40285`/`0x285` never clears via local `PORT_RESUME`). No
  longer cascades into a full-bus re-enumeration storm — costs ~800ms for
  that one port instead of ~30s+ across all ports. No functional impact
  observed. Root-causing *why* port 5 doesn't resume in place remains
  unsolved (see "Open next steps"), but is no longer the expensive part.

## What today's session established (2026-07-03)

Chronological experiment history (see `git log` on this branch for full
commit list/messages — search for `t2bce:`/`t2touchbar:`/`docs:` commits from
today):

1. **EP0 status=3 handling** went through several iterations before landing on
   the current approach: on `EP0 status=3`, complete the failed control URB
   *and* schedule an endpoint reset (`ENDPOINT_RESET` + `ENDPOINT_SET_STATE
   ACTIVE`) instead of just marking the local queue inactive/stalled. Earlier
   attempts left the firmware endpoint in a "zombie" state
   (`active=0 paused_by=0 stalled=0 state=2` i.e. `BCE_VHCI_ENDPOINT_STALLED`
   despite the local stalled flag being cleared).
2. **Stale internal pause after EP0 reset**: after the reset, some later EP0
   control URBs still landed on `active=0 paused_by=1 stalled=0 state=1`
   (`BCE_VHCI_PAUSE_INTERNAL_WQ` / `BCE_VHCI_ENDPOINT_PAUSED`). Fixed by
   detecting this state on URB enqueue and resuming EP0
   (`resume(INTERNAL_WQ)`) before linking the new URB. Doing this synchronously
   from the enqueue path caused a `BUG: scheduling while atomic` regression
   (enqueue can be called from `usb_hcd_giveback_urb()`/usbhid callbacks), so
   the recovery was moved to a workqueue (`tq async resume start/done`).
3. **Root cause of the Fn/mode-report path found**: when a Fn press cancels
   the in-flight control URB to send a new mode SET_REPORT, that cancel pauses
   EP0 internally. If the T2 sends its `TRANSFER_REQUEST` for the *new* URB in
   exactly that window, the event used to be dropped
   (`EP0 event drop-inactive`) because the queue was inactive — so the new URB
   waited forever for an event that had already been discarded. Fix: defer
   such events instead of dropping them
   (`EP0 event defer-inactive-internal-pause` → `deferred-deliver` →
   `deferred-consumed`) once the endpoint becomes active again.
4. **Result**: with all of the above, Touch Bar native mode now reliably shows
   the image and Fn reliably switches between F-key/standard view across
   repeated fast toggles, with no more `scheduling while atomic`. This is
   commit `d62bec5`, confirmed as today's "milestone".
5. **Internal reset for resume-stuck ports — tried and reverted**
   (`eda0582` → `f2795a4`): to attack the remaining long-resume problem, we
   tried having the t2bce port worker call `bce_vhci_reset_device()` itself,
   once per port/resume cycle, for ports still stuck in `needs_resume` after
   local retries — the idea being to refresh the BCE-internal device/endpoint
   state without letting usbcore trigger a root-hub reset.
   - Result: the internal reset formally succeeded (port raw value moved from
     `0x40285` to `0x40215`, i.e. ready + connection-change) but usbcore still
     treated the connection-change as reset-worthy and re-enumerated multiple
     BCE devices (5-1..5-4, 5-6, 5-7) anyway. Resume time was unchanged
     (~34.4s) and, worse, keyboard/trackpad/Touch Bar were left completely
     unresponsive after resume in that test, requiring an SSH reboot.
   - Conclusion: `bce_vhci_reset_device()` from the port worker is not a safe
     shortcut. It was cleanly reverted (net-zero diff on `vhci.c`/`vhci.h`
     versus `d62bec5`); current shipped behavior is back to "let usbcore do a
     normal external reset when local resume retries on a port are exhausted".
     Do not re-attempt this exact approach without first understanding why the
     BCE-internal reset produces a connection-change usbcore insists on
     honoring.

## Latest resume-cycle log analysis (post-revert, current baseline)

Analyzed two full suspend/resume cycles from the current boot
(suspend/resume at 17:45–17:53 and 17:57:03–17:57:52, both ~34.4s resume).
Detail below is from the second cycle (`journalctl -k -b 0`, filtered on
`bce-vhci|t2touchbar|t2bdrm|PM: |EP0|reset high-speed|port worker|
GetPortStatus`).

- **No warnings, errors, call traces, or "failed" strings anywhere in this
  cycle's log** — this run was clean relative to earlier regressions
  (no `scheduling while atomic`, no unexpected `urb_reject`).
- Ports 1, 2, 3, 4, 7 resume normally: at most one `PORT_RESUME` retry, then
  `ready`.
- **Port 6 (Touch Bar)**: starts at `raw=0x275`, one `PORT_RESUME` clears it to
  `raw=0x215` → `ready`. Later, during post-reset re-enumeration, a single
  `EP0 status=3 dev=2 port=6` occurs on a mode SET_REPORT
  (`reqtype=21 req=09 value=0202`). The full recovery chain fires and
  completes cleanly: `tq reset start` → `tq reset command` → `tq resume
  begin/done` → `tq reset done ... active=1 paused_by=0 stalled=0 state=0` →
  `port worker ready port=6 waiting=0`. This is the known-healthy signature
  from the milestone, working as intended — no dead end this time.
- **Port 5 (Topcase/keyboard/trackpad)**: stays at `raw=0x40285` across all 10
  local `PORT_RESUME` retries (guard countdown 9931ms → 9621ms, value never
  changes), then `resume retries exhausted port=5 raw=40285`. usbcore
  subsequently does a normal external reset (`SetPortFeature RESET` →
  `raw=0x215` → `usb 5-5: reset high-speed USB device number 9`). **Port 5 is
  still the only port that needs a real usbcore-level reset every time** — the
  other BCE devices (5-1..5-4, 5-6, 5-7) get reset too, but as a side effect of
  being on the same bus/root hub during the storm, not because they were
  individually stuck.
- Duration structure: the local port-worker retry phase for port 5 is short
  (well under a second of logged activity); the vast majority of the 34s
  resume window is the `reset high-speed USB device` re-enumeration storm
  across all 7 BCE child devices (GET_DESCRIPTOR/SET_CONFIGURATION/SET_IDLE
  floods per device). **The 34s cost is dominated by full re-enumeration of
  the whole BCE bus, triggered by port 5 alone failing to resume in place.**
- `t2touchbar_kbd` sends `mode` SET_REPORTs during the resume burst
  (`mode=3`, then `mode=2` — the one that hits the EP0 status=3 above) and
  again afterward from user Fn presses once steady state is reached. No sign
  of a stuck/duplicate mode report in this cycle.
- `t2bdrm`: confirmed absent from the log, consistent with it not being
  loaded.

**Takeaway**: the Touch Bar/EP0 fix from the milestone is holding up under
real suspend/resume, including a live EP0 status=3 recovery on port 6 during
the post-reset storm. The remaining 34s resume cost is now clearly and
specifically attributable to **port 5 never resuming in place** (`raw`
oscillating only within the `0x285`/`0x40285` family, never reaching
`0x215`/ready via local `PORT_RESUME`), which forces usbcore into a full
external reset/re-enumeration of the entire BCE bus. This matches the
pre-revert analysis in the old learnings file, and confirms the internal-reset
experiment (see above) is not the way to fix it.

## Portability finding: no hardcoded port numbers

Since this needs to run on Mac models other than the current test unit,
checked whether any of the resume-gating logic assumes a fixed port index.

- The internal VHCI port numbering comes straight from T2 firmware
  (`bce_vhci_cmd_controller_enable()`'s `portMask`), not from any VID/PID or
  device-class check in the driver. There is nothing in `vhci.c`/`transfer.c`
  that maps a port number to "this is the Touch Bar" or "this is the topcase"
  by USB descriptor — the driver only sees generic ports/devices/endpoints.
  Which physical device ends up on which port number is therefore entirely a
  per-model (and possibly per-boot firmware) decision, not something we can
  rely on being stable across MacBook Pro 13"/15"/16", MacBook Air, Mac mini,
  iMac/iMac Pro (some of which don't even have a topcase or Touch Bar at all).
- Found exactly one place that hardcoded a port index anyway:
  `bce_vhci_hub_control()` in `modules/t2bce/vhci/vhci.c` had
  `bool topcase_stuck = wIndex == 5 && needs_resume && tries >= MAX;` guarding
  whether usbcore's real `SetPortFeature RESET` is let through during the
  resume guard window, versus being suppressed with just an internal
  `PORT_RESUME`.
  - This was dangerous for portability: on a Mac where the topcase (or
    whatever ends up being the hard-stuck device) enumerates on a port other
    than 5, the real reset would never be let through for the actually-stuck
    port — resume would hang indefinitely on that port instead of falling
    back to a normal usbcore reset. Meanwhile port 5 on a different model
    (whatever device that happens to be) would get the "allow real reset"
    exception it doesn't need.
  - Fixed by dropping the `wIndex == 5` clause. The remaining condition
    (`needs_resume && port_resume_tries[wIndex] >= BCE_VHCI_PORT_RESUME_MAX_TRIES`)
    is already fully port-generic and firmware-state-driven — it's the exact
    same condition the port worker itself uses to declare "resume retries
    exhausted" for whichever port hits it
    (`bce-vhci: port worker resume retries exhausted port=%d ...`). The port
    index was never adding correctness, just artificially restricting an
    already-generic check to one port observed on this specific test unit.
  - Renamed `topcase_stuck` → `port_resume_exhausted` and updated the log
    line (was "stuck topcase", now "resume-exhausted port") so future readers
    don't assume a device identity that may not hold on other models.
  - No other hardcoded port numbers found in `modules/t2bce/vhci/*.c` or
    `modules/t2touchbar*/`; the rest of the state (`port_to_device[17]`,
    `port_resume_tries[17]`, etc.) is already indexed generically up to the
    firmware-reported `port_count`.
- Testing caveat: on `mac-t2` port 5 happens to be the stuck topcase port, so
  this change should be a no-op there (same behavior, same port index still
  satisfies the same condition). The real value of this fix only shows up on
  a different chassis/model — we have no second physical unit to verify that
  on right now, so treat this as a correctness fix by inspection, not yet
  empirically confirmed on hardware where port 5 differs.
- **Confirmed on hardware** (commit `911c7eb`, srcversion
  `FFE09A8ED94417237F6C082`): behavior on `mac-t2` is unchanged from before the
  fix — Touch Bar works, Fn switches reliably, resume still slow, Touch Bar
  still briefly black after resume until Fn wakes it. Exactly as predicted by
  the no-op caveat above (port 5 still satisfies the same condition without
  the port-index gate). No regression from removing the hardcode.

## Refined hypothesis: C_CONNECTION change bit, not just SUSPENDED

Re-examined the bit-level raw values with the `BCE_VHCI_PORT_STATUS_*` enum in
`vhci.c` to understand *why* port 5 differs from port 6 mechanically, not just
that it does:

- `0x275` (port 6, resolves fine) = `HIGH_SPEED(0x200) | SUSPENDED_LEGACY(0x60)
  | ENABLED(0x10) | CONNECTED(0x4) | 0x1`. No `C_CONNECTION(0x40000)` bit.
  `PORT_RESUME` clears the legacy-suspend bits, port becomes ready, done.
- `0x285`/`0x40285` (port 5, hangs) = `HIGH_SPEED | SUSPENDED(0x80) |
  CONNECTED(0x4) | 0x1`, and in the `0x40285` variant also
  `C_CONNECTION(0x40000)`. **`ENABLED` is never set** by any number of
  `PORT_RESUME` retries.
- The reverted internal-reset experiment (see above) moved the raw value from
  `0x40285` → `0x40215` — i.e. it *did* clear `SUSPENDED` and set `ENABLED`,
  but `C_CONNECTION(0x40000)` was still set both before and after. That bit is
  what `bce_vhci_hub_control()`'s `GetPortStatus` path (`vhci.c:382-383`)
  turns into `USB_PORT_STAT_C_CONNECTION` in `wPortChange` — and a
  connection-change is precisely what makes usbcore's hub thread treat the
  port as "newly connected" and drive a full reset + re-enumeration,
  independent of whether `ENABLED`/`SUSPENDED` already look fine. This is
  standard USB hub state-machine behavior, not a bug in our `GetPortStatus`
  handling.
- So the actual trigger for the reset/re-enumeration storm is specifically
  the **`C_CONNECTION` bit being stuck set on port 5**, which our internal
  reset attempt did not (and structurally cannot, since it's a separate
  firmware-reported change flag) clear. `bce_vhci_cmd_port_status()` does
  support a `clearFlags` argument (mask `0x560000`, includes bit 0x40000) via
  `ClearPortFeature(C_CONNECTION)` (`vhci.c:476-479`), but the port worker's
  own polling loop always calls it with `clearFlags=0`
  (`vhci.c:1150`/`vhci.c:403`), so it never clears this bit itself while
  gating usbcore.
- Open question this raises, not yet answered: is the `C_CONNECTION` bit on
  port 5 firmware-genuine (T2 really power-cycles/re-inits the topcase
  controller chip across sleep, unlike the Touch Bar's controller, so a real
  reconnect happened and re-enumeration is factually correct/needed) — or is
  it spurious/stale and could be safely cleared internally (mirroring what
  `ClearPortFeature(C_CONNECTION)` already does) before usbcore ever sees it,
  once local `PORT_RESUME` has already brought the rest of the port state
  back to a sane place? These have very different implications: the first
  means 34s re-enumeration may be structurally unavoidable for this port (in
  which case the win is speeding up/parallelizing the multi-device
  re-enumeration, not preventing it); the second means we could potentially
  suppress it, but doing so blind failed once already (unresponsive HID
  requiring reboot) — any attempt here needs a way to positively confirm
  "topcase controller is actually alive" before hiding the change bit from
  usbcore, not just clearing it optimistically.

## Timing instrumentation added (commit `fa4a57f`), hypothesis to confirm

Rather than guess further from bit values alone, added duration logging for
every single VHCI firmware command round-trip
(`bce_vhci_command_queue_execute()` in `queue.c`: logs `cmd`/`param1`/`param2`/
`elapsed_ms`/`res_status` on every completion, plus timeout/cancel paths) and
broke `bce_vhci_reset_device()` (`vhci.c`) down phase by phase
(`DEVICE_DESTROY`, `PORT_RESET`, `DEVICE_CREATE`, endpoint re-create loop),
each with its own elapsed time. Pure logging, no behavior change — safe to
deploy regardless of what it finds.

**Concrete hypothesis this is meant to confirm/refute**: `bce_vhci_cmd_device_destroy()`
uses `BCE_VHCI_CMD_TIMEOUT_LONG` (30000ms — see `command.h:156`), and
`bce_vhci_command_queue_execute()` holds `cq->mutex` for its entire duration
(`queue.c`), which serializes *every other port's* firmware commands behind
it. `bce_vhci_reset_device()` calls `bce_vhci_cmd_device_destroy()` first,
before `PORT_RESET`, whenever the port already had a device attached — true
for port 5 (topcase), since it was already enumerated pre-suspend. If the
firmware is slow to answer DEVICE_DESTROY specifically while port 5 is in its
stuck `0x40285`/`C_CONNECTION` state, a single ~30s command timeout there
would single-handedly explain the observed ~34.3s/34.4s/34.369s resume
durations (30s timeout + ~1s cancel round-trip + a few seconds for the
port_reset/device_create/endpoint_create phases and the other ports' queued-up
commands) — independent of, and possibly a more actionable target than, the
`C_CONNECTION` bit question above. This is falsifiable directly from the next
test's logs: look for a `DEVICE_DESTROY took ...ms` line near 30000ms for
port 5's devid.
- If confirmed: the fix target becomes *why does DEVICE_DESTROY hang for the
  stuck device* (maybe it needs a shorter timeout with its own fallback path,
  or the destroy needs to happen after/without waiting on a device that's
  already known-unresponsive) rather than the connection-change bit itself.
- If not confirmed (device_destroy is fast): the 30s must be coming from
  usbcore's own retry/timeout logic outside t2bce (e.g. repeated failed
  descriptor reads at 5s USB control-transfer timeout each), which we can only
  observe, not directly instrument from t2bce.

## DEVICE_DESTROY hypothesis REFUTED; "34.575s resume" figure is a clock artifact; new root cause found (backlight PM hooks missing)

Deployed the `fa4a57f` timing instrumentation and analyzed a full suspend/resume
cycle from `mac-t2` (lid closed 20:26:26, opened 20:27:35). Used
`journalctl -k -b 0 -o short-monotonic` to get real monotonic timestamps
(immune to the wall-clock bunching journald does around a suspend/resume
boundary, where every line gets stamped with the same display-second because
journald itself was frozen during the actual sleep).

**Hard findings, in order of how they were derived:**

1. **Every single instrumented firmware command in this resume cycle completed
   in 0–2ms** — including `DEVICE_DESTROY` for port 5 (the previously-stuck
   topcase port), every time it was called, across every port, both passes.
   The DEVICE_DESTROY/30s-mutex-hang hypothesis from the previous section is
   **REFUTED** by direct measurement. There is no 30-second (or any multi-ms)
   stall anywhere inside `bce_vhci_command_queue_execute()` or
   `bce_vhci_reset_device()` in this cycle.
2. Using `-o short-monotonic`, `bce_vhci: bus_resume entry` fires at
   `132.466460`, and the entire per-port reset/re-enumerate storm (all 7
   ports, both passes, including the `usb 5-N: reset high-speed USB device`
   events) finishes by `132.522`, i.e. **the whole t2bce/bce_vhci resume path
   is done in about 55ms**. `t2bce: complete: entry/exit` (the PM `.complete`
   callback, which by kernel PM design can only run *after every device in the
   system* has finished its `.resume()`) fires at `132.522402`, confirming
   `dpm_resume_devices()` for the *entire machine* had already finished by
   then — not 34 seconds later.
3. `PM: resume devices took 34.575 seconds` prints at `132.523127`, ~0.7ms
   after `t2bce: complete: exit`. Given point 2, this duration **cannot be
   real** — it directly contradicts the observed monotonic ordering (a
   `.complete()` callback cannot fire before `dpm_resume_devices()` for the
   whole system returns). It's a stale/bogus measurement, most likely because
   the same kernel-internal clock produced an equally bogus-looking "37.6s
   gap" between `Filesystems sync: 0.066 seconds` and the first
   post-wake log line — except that gap turned out to be **completely real
   and expected**: cross-checking with `journalctl -b 0` (all units, not just
   kernel) showed `systemd-logind: Lid closed.` at 20:26:26 and
   `systemd-logind: Lid opened.` at 20:27:35 — i.e. that "gap" is simply the
   real physical time the lid was closed, controlled entirely by the user, not
   a stall. The `34.575s` "resume devices took" figure is very likely this
   same clock domain producing a nonsensical value across the suspend/resume
   boundary on this hardware — **not a genuine measurement of resume time**.
4. Cross-checked against real wall-clock userspace logs after lid-open:
   `gdm-password]: gkr-pam: unlocked login keyring` at 20:27:40 (5s after lid
   open), user's own interactive shell command (`sudo dmesg -w`) at 20:27:45
   (10s after), first Fn press at 20:27:49 (14s after) — and that first Fn
   press's HID control transfer completes cleanly with **no retries, no
   stalls**. None of this is consistent with the system being unusable for
   34 seconds. The desktop is interactive within single-digit seconds of
   lid-open.
5. **So what actually causes the black Touch Bar?** Checked
   `t2touchbar_kbd.c`: it already has explicit `.suspend`/`.resume`/
   `.reset_resume` hooks (`appletb_kbd_suspend`/`appletb_kbd_resume`) that
   restore `kbd->saved_mode` via `appletb_kbd_set_mode()`. The log **confirms
   this fires correctly and immediately** during the resume burst — a
   `touchbar mode request sent: mode=2` line appears right in the reset storm
   at `132.5xx`, well before the user ever touched Fn. So the *mode*
   (F-keys vs. special-keys layout) is provably restored automatically on
   resume already.
   
   But `t2touchbar_bl.c` (the **backlight** driver, a separate HID interface/
   device from the mode-switching one) had **no `.suspend`/`.resume`/
   `.reset_resume` hooks at all**. It only relies on the generic Linux
   backlight-class `BL_CORE_SUSPENDRESUME` mechanism (via
   `appletb_bl_backlight_ops.options`), which triggers `update_status()`
   through the backlight class's own PM notifier — a code path with no
   ordering guarantee relative to the underlying HID/USB transport actually
   being ready again after `bce_vhci_reset_device()` tore down and recreated
   the whole device. This is a highly plausible root cause for "content is
   correct (Fn/mode switching works) but the panel stays visually dark until
   nudged": the automatic brightness-restore HID `SET_REPORT` may fire before
   the transport is ready and silently get lost, leaving the backlight
   logically at brightness 0 (off) even though the KBD mode driver already
   fully recovered.

**Fix applied (commit pending)**: added explicit `appletb_bl_suspend`/
`appletb_bl_resume` hid_driver callbacks to `t2touchbar_bl.c`, mirroring the
already-proven-working pattern in `t2touchbar_kbd.c`. `.resume`/
`.reset_resume` now call `appletb_bl_update_status(bl->bdev)` directly at the
HID-level resume callback point (same point where kbd's mode-restore is
proven to run correctly), instead of solely depending on the implicit
backlight-class mechanism. Added liberal `pr_info` logging of
`last_requested_brightness`/`full_on`/current backlight-class brightness and
blank state on both suspend and resume entry, plus a log line after the
`hid_hw_request()` SET_REPORT call, so the next test's log will show directly
whether this path fires, in what order relative to the reset storm, and
whether the class-level automatic path was *also* firing redundantly (which
would suggest the class mechanism does work but was simply racing/losing, not
never firing).

**What to check in the next test's logs**: search for `appletb_bl_resume` and
`appletb_bl_set_brightness`/`appletb_bl_update_status` lines around the resume
timestamp, and compare their timing against `bce_vhci_reset_device port=7`
(Touch Bar Backlight's port) — if `appletb_bl_resume` fires *before* port 7's
`DEVICE_CREATE`/endpoint re-create finishes, that would confirm a genuine race
even with the explicit hook, pointing to a next fix of deferring the backlight
restore until the port is confirmed ready (mirroring how `bce_vhci_port_ready_for_hub`
already gates other logic).

## USER CORRECTION: the ~30s resume delay is REAL wall-clock time, not a clock artifact — and t2bce_suspend()/resume() were never even called

The previous section's conclusion that "34.575s resume" was a bogus clock
artifact was **only half right**. The user explicitly corrected this after
testing the `3737104` (backlight PM hooks) build: suspend is fine, but real
elapsed wall-clock time between opening the lid and the system actually being
usable is genuinely **~30 seconds** — confirmed by the user directly watching
the clock, not inferred from a kernel print.

Re-analyzed the next test cycle (`Lid closed.` 20:57:37.869, `Lid opened.`
20:58:33.716, all timestamps `journalctl -o short-precise` real wall clock,
not monotonic) with much more scrutiny:

1. **The "34.109 seconds" print is still bogus** — cross-checked again, this
   time against real wall-clock instead of monotonic time: `Lid opened.` fires
   at `20:58:33.716595`, and `PM: resume devices took 34.109 seconds` prints
   at `20:58:33.747980` — only **31ms** later. Whatever this duration counter
   is measuring, it is not measuring real elapsed time across this suspend/
   resume boundary. This has now been confirmed twice, by two independent
   methods (monotonic-vs-monotonic in the previous cycle, wall-clock-vs-wall-
   clock in this one). Do not trust this print on this hardware/kernel combo.
2. **But there is still no ~30s gap anywhere in the software logs.** A
   Python pass over every journal line in the 40s window after `Lid opened.`
   found the largest gap between consecutive log lines was ~9s (an unrelated
   `realmd` timeout during normal desktop idle, nothing to do with resume).
   `gdm-password]: gkr-pam: unlocked login keyring` fires at 20:58:38.236
   (4.5s after lid-open), `t2touchbar_kbd` Fn-driven mode toggles start at
   20:58:40.124 (6.4s after), wifi/DHCP/NetworkManager fully reconnected by
   20:58:38.6. **Software-visible resume completes in single-digit seconds.**
   Grepped for `drm|i915|panel|backlight|mutter|gnome-shell|monitor|connector`
   in the whole window: essentially nothing logged there either, so the main
   display path is a blind spot, not a proven-fast path — but nothing points
   to a stall there specifically.
3. **The real finding: `t2bce: suspend: entry` / `t2bce: resume: entry` never
   appear anywhere in this boot's log at all** (`grep -c` returns 0 for both,
   across the *entire* boot, not just this window). Only `t2bce: complete:
   entry`/`exit` fired (at 20:58:33.747657, with `no_state_fallback=0
   no_state_resume=0` — i.e. `t2bce_complete()` ran believing no suspend/
   resume had even happened, which is exactly consistent with them never
   being called). No "printk messages dropped" indicator anywhere in the log,
   ruling out simple ring-buffer loss as the explanation.
4. **Root cause (best-supported hypothesis): `t2bce_pci_driver_pm` never
   defined a `.prepare` callback.** Per the PCI PM core contract, when a
   driver's `dev_pm_ops` has no `.prepare`, `pci_pm_prepare()` falls through
   to `pci_dev_keep_suspended()`, which can decide the device is "already
   runtime suspended" and set `direct_complete`, causing the PM core to skip
   `.suspend()`/`.resume()` entirely and call only `.complete()` — which is
   *exactly* what the log shows happened. `t2bce_main.c` never calls
   `pm_runtime_enable()`/`pm_runtime_get/put` itself, so this would have to be
   triggered by the generic PCI/ACPI power-management defaults treating the
   function as suspendable, not by anything our driver deliberately requests.
5. **Why this would explain the ~30s real delay**: if `t2bce_suspend()` is
   skipped, the `SAVE_STATE_AND_SLEEP`/`SLEEP_NO_STATE` mailbox handshake
   (`bce_pm_suspend_prepare()` → `bce_pm_suspend_try_state()`/
   `bce_pm_suspend_fallback_no_state()`) never reaches the T2 coprocessor
   before the PCIe link goes down. The T2 then has to go to sleep
   *uncoordinated*, with no warning and no chance to persist fast-wake state.
   On wake, it plausibly has to do a slow, autonomous cold-boot-style
   recovery of its own bridgeOS side — entirely before the PCIe link/mailbox
   comes back up, hence completely invisible to any Linux-side log — which
   would show up to the user as exactly what they described: suspend is
   fine, but a long real delay before *anything* happens, followed by
   everything (USB re-enum, keyboard, network, backlight) cascading back in
   one synchronized burst within tens of milliseconds once the T2 finally
   signals readiness. This also plausibly ties into the black-Touch-Bar
   symptom: an uncoordinated T2 sleep/wake would explain why its panel/
   backlight state isn't preserved cleanly, on top of the missing HID PM
   hooks fixed in the previous session.
   Note for context: at boot, `t2bce: capturing our device` → `t2bce: handshake
   done` takes only ~42ms (20:53:53.630 → 20:53:53.672), so the T2's fast
   path is normally very quick — supporting the idea that ~30s is specifically
   the cost of an *unprepared* wake, not an inherent T2 boot cost.

**Fix applied (commit pending)**: added an explicit `t2bce_prepare()` callback
to `t2bce_pci_driver_pm.prepare` that unconditionally returns `0` and logs
`pm_runtime_status_suspended(dev)` and the device's PM usage count. Per the
PCI PM core contract, a `.prepare` callback returning `0` (as opposed to
returning nothing/no callback at all) forces `pci_pm_prepare()` to return `0`
too, which bypasses the `pci_dev_keep_suspended()` skip path unconditionally —
guaranteeing `t2bce_suspend()`/`t2bce_resume()` are always called on every
cycle from now on, regardless of any runtime-PM state PCI core thinks the
device is in.

**What to check in the next test's logs**: search for `t2bce: prepare:`,
`t2bce: suspend: entry`, and `t2bce: resume: entry` — they should now appear
on every cycle where previously the resume one was completely absent. Compare
their timestamps against `Lid opened.`/`Lid closed.` and re-measure the real
wall-clock delay the user experiences. If the delay collapses to something
much shorter, this confirms the uncoordinated-T2-sleep hypothesis. If the
delay persists even with `.suspend()`/`.resume()` now provably running, the
next place to look is inside `bce_pm_suspend_try_state()`/
`bce_pm_resume_stateful()`'s mailbox round trip itself (already has
`ktime`-based timing instrumentation from `fa4a57f` — check `bce-vhci: cmd
done`/`cmd timed out` lines for the `SAVE_STATE_AND_SLEEP`/
`RESTORE_STATE_AND_WAKE` commands specifically), or genuinely on the T2's own
side (unfixable from Linux, in which case the mitigation would be to fall
back to a faster no-state path deliberately).

## 2026-07-03/04: The ~30-55s resume delay found and fixed

This closes out the "34.575s"/"34.109s resume devices took" mystery from the
two sections above. Both of those sections correctly proved the print itself
was bogus on this hardware/kernel, correctly proved the delay was nonetheless
real (user-observed wall clock), and correctly fixed a real bug
(`t2bce_prepare()`, commit `16613e0`) that forces `t2bce_suspend()`/
`t2bce_resume()` to actually run every cycle instead of being silently
skipped via PCI `direct_complete`. None of that turned out to be the
dominant cost, though — the `.prepare` fix was necessary (skipping
suspend/resume entirely is clearly wrong) but the real ~30s was still there
after it, coming from inside `bce_vhci`'s own resume path once it was
actually running every time.

### False lead 1: GetPortStatus gating on the whole port bitmask instead of per-port

`bce_vhci_hub_control()`'s `GetPortStatus` busy-wait checked
`port_change_waiting` as a whole instead of just the queried port's bit — a
real bug (one port's `GetPortStatus` could busy-wait on *other* ports being
ready), fixed in commit `eea8858`. Test result: **no change** in the ~35-38s
delay. The bug was real but not the cost driver.

### The trap that stalled diagnosis: `ktime_get()` and printk timestamps are unreliable for several real seconds right after this hardware's ACPI resume

All existing `elapsed_ms` instrumentation (VHCI firmware commands in
`queue.c`, `bce_vhci_reset_device()` phase timings) used `ktime_get()`
(CLOCK_MONOTONIC). Comparing it against the jiffies-based
`resume_reset_guard_left_ms` countdown and against `PM: resume devices
took ...` in the *same* log window showed `ktime_get()` deltas reading back
near-zero (0-12ms) for everything, while jiffies and the core resume-timing
line showed several real seconds elapsing in that identical window. The raw
printk timestamps themselves looked similarly compressed right after a
genuine resume.

Fix (commit `a4b6119`): switched all of this instrumentation to
`ktime_get_boottime()` and added an explicit `real_elapsed_ms` marker
(boottime delta since `bus_resume` entry) at the key port-worker/gate/
RESET-guard log points. **This step was a prerequisite, not a diagnosis by
itself** — without it, every subsequent timing conclusion drawn from the
logs up to this point would have been wrong, exactly as the two sections
above already found for the top-level `PM:` print (they just hadn't yet
connected it to t2bce's *own* command-timing logs having the identical
problem).

### The real root cause, found via `real_elapsed_ms` + ftrace

With trustworthy timestamps, a very consistent pattern emerged: a real gap
of ~5.2-5.5s before almost every port's `hub SetPortFeature RESET port=N`
(6 gaps × ~5.3s ≈ the entire `PM: resume devices took ~34s`).

A targeted ftrace capture (`function_graph`, entry points `hub_event` and
`usb_port_resume`, `tracing_thresh=200000` so only >200ms branches print)
pinned the exact call chain in one shot:

```
usb_port_resume -> finish_port_resume -> usb_reset_and_verify_device
  -> hub_port_init -> usb_get_status -> usb_control_msg
  -> usb_start_wait_urb -> wait_for_completion_timeout   (~5.01s, 6x)
```

`usb_get_status()` — usbcore's standard `GET_STATUS(device)` control
request, issued as part of `hub_port_init()`'s post-reset validation for
every device — blocks for almost exactly `USB_CTRL_GET_TIMEOUT` (5000ms)
before usbcore gives up, cancels the URB, and retries; the retry then
completes instantly. This request goes through our EP0 transfer queue, not
`hub_control()`, and the one function that traces per-stage EP0 control
transfers (`bce_vhci_urb_log_control()`) logged at `pr_debug` — invisible at
default log level — so this dominant cost had never shown up anywhere.
Promoted to `pr_info` in commit `bf3c100`.

With that visibility, the next test showed the precise sequence for an
affected port: the `GET_STATUS` URB is `enqueue`d, sits in
`init-wait-setup` for ~5s, gets `cancel-request`ed by usbcore
(`urb_reject=1`) — then an identical retry URB `enqueue`s and completes
cleanly (`setup-request` → ... → `complete`) essentially immediately. The
log also showed, right at system suspend for the same device/port, an
`EP0 event remove-pending dev=1 port=1 evq=1` line — i.e. a `TRANSFER_
REQUEST` the T2 firmware had *already sent* for that EP0 queue, but that
had no URB yet to hand it to (deferred), got wiped when the endpoint was
paused for suspend.

**Root cause**: `bce_vhci_transfer_queue_suspend_pause()` in
`modules/t2bce/vhci/transfer.c` — the suspend-specific pause path used by
`bce_vhci_bus_suspend()`, distinct from both the generic src-based pause/
resume and from the device-teardown/reset path — unconditionally called
`bce_vhci_transfer_queue_remove_pending()`, discarding any not-yet-delivered
EP0 event. Unlike `bce_vhci_reset_device()`'s DEVICE_DESTROY+CREATE cycle,
suspend-pause never tears down or recreates the queue's SQ/CQ, so the
discarded event was still perfectly valid after resume. Without it, the
first post-resume EP0 request on that endpoint had nothing to consume and
had to wait for the T2 firmware to notice independently and resend — which
took ~5 real seconds, once per affected port.

**Fix** (commit `0939182`): removed the `remove_pending()` call from
`bce_vhci_transfer_queue_suspend_pause()` specifically. Left the other two
call sites (`bce_vhci_destroy_transfer_queue()`'s device teardown, and the
generic `bce_vhci_transfer_queue_do_pause()` used for INTERNAL_WQ/FIRMWARE
pauses) untouched, since those do correspond to real queue regeneration
where stale events are genuinely meaningless.

**Confirmed on hardware**: `PM: resume devices took 0.805 seconds` (was
34+s), Touch Bar visible immediately post-resume with no Fn press needed,
keyboard/trackpad normal. Full unfiltered log re-checked afterward: no
warnings/errors/`EP0 status=3`/timeouts anywhere in the cycle; only Port 5's
already-known real-reset (see below) and an unrelated benign
`brcmf_p2p_create_p2pdev: set p2p_disc error` from the WiFi driver.

**Transferable lesson**: when timing numbers from logs spanning a real ACPI
resume don't agree with each other (e.g. `ktime_get()` near-zero but jiffies
or `PM: resume devices took` show real seconds in the same window),
question the measurement method itself before trusting a diagnosis built on
it. `ktime_get_boottime()` is the clock that held up here. When a
boottime-confirmed real delay shows *nothing* in t2bce's own logs no matter
how much logging is added, the time is likely being spent inside generic
usbcore between calls into the HCD — ftrace's `function_graph` tracer with
a `tracing_thresh` (~200ms) on `hub_event`/`usb_port_resume` found the exact
function in one capture, without needing kernel source on hand.

## Open next steps

1. **Port 5 resume-in-place, without `bce_vhci_reset_device()`.** Understand
   why `raw=0x285`/`0x40285` on port 5 does not clear via repeated
   `PORT_RESUME` the way port 6/others do at `0x275`. Candidates to
   investigate: whether port 5 carries additional endpoints (keyboard +
   trackpad + Force Touch + Caps Lock LED share this port per T2linux
   community notes) that need their own readiness signal before the port bit
   clears; whether the firmware expects a different resume command/sequence
   for this port; whether waiting longer (vs. resetting) would eventually
   clear it. No longer urgent for resume *speed* (now ~800ms for this one
   port), but still open for correctness/robustness.
2. Keep the EP0 status=3 / deferred-event / EP0 control per-stage logging as
   pr_info for now — it was exactly what solved the resume-delay
   investigation above. Only reduce verbosity once port 5's resume behavior
   is understood and fixed, or if log volume becomes a problem.
3. Continue treating `0x275` and `0x285`/`0x40285` as distinct cases, per the
   milestone finding: `0x275` = endpoint-not-ready-yet (wait it out), `0x285`
   family = harder stuck state (currently: let usbcore reset, now cheap).

## 2026-07-07: Kernel 7.1.3 upgrade broke keyboard/trackpad/mouse (unrelated to t2bce resume work)

Not part of the resume investigation above — a separate DKMS build-compat
issue hit when the user updated `mac-t2` from kernel 7.0.14 to 7.1.3.

**Symptom**: after the 7.1.3 boot, keyboard/trackpad/Touch Bar were gone
entirely (not just slow/broken resume — no input devices at all). System
fell back to 7.0.14 on the next boot (GRUB default was actually still 7.1.3;
the fallback was the user manually rebooting, not automatic boot counting —
this GRUB 2.12 setup has boot counting disabled).

**Root cause**: kernel 7.1 reworked the HID core's battery-reporting API.
`struct hid_device` used to carry single `battery`/`battery_report_type`/
`battery_report_id`/`battery_capacity`/`battery_max` fields directly; 7.1
replaces these with a `batteries` list of `struct hid_battery` plus a
`hid_get_battery(hdev)` helper (multi-battery-per-device support). Both
`modules/t2touchbar/t2hid.c` (`apple_fetch_battery()`) and
`modules/hid_t2magicmouse/hid_t2magicmouse.c`
(`magicmouse_fetch_battery()`) used the old fields directly and failed to
compile under 7.1.3 (`dkms.service`'s post-kernel-install autobuild failed
with exit status 2 for exactly these two DKMS packages — every other
module, including `t2bce`/`t2audio` core, built fine). Without `t2hid`
bound, the Apple keyboard/trackpad HID interface had no functional driver
(`hid_apple` is deliberately blacklisted), which is why input devices
disappeared completely rather than just losing battery reporting.

**Fix** (commit `7a26428`): added a `LINUX_VERSION_CODE < KERNEL_VERSION(7,
1, 0)` guard in both `apple_fetch_battery()` and
`magicmouse_fetch_battery()`, following the existing
`LINUX_VERSION_CODE`-guard convention already used elsewhere in this repo
(`modules/t2bce/*.c`). Old branch keeps the direct-field access for
pre-7.1 kernels; new branch uses `hid_get_battery()` + `struct hid_battery`
fields (`report_type`, `report_id`, `capacity`, `max`).

**Verification method**: cross-compiled both modules directly (plain
`make KDIR=...`, no DKMS registration) against both
`/usr/src/kernels/7.0.14-201.fc44.x86_64` and
`/usr/src/kernels/7.1.3-200.fc44.x86_64` headers on `mac-t2` before
touching DKMS/reboot at all — confirmed both compile clean on both. Then
did the full cycle: pushed, `grub2-reboot` one-shot into the 7.1.3 entry
(GRUB has no boot-counting here, so a one-shot is the safe way to test a
non-default-for-long-term kernel without permanently changing the
default), ran `install-dkms-modules.sh` + `rebuild-initramfs.sh` under
the running 7.1.3, rebooted again for a clean load, then confirmed via
`dmesg`/`/dev/input/by-id` that `t2hid`/`t2touchbar_kbd`/`t2touchbar_bl`/
`t2magicmouse` all bind and the keyboard/trackpad/Touch Bar input devices
are present again — identical device set to the working 7.0.14 boot.

**Transferable lesson**: `dkms status` showing "installed" for a given
kernel only means DKMS successfully registered/copied the source — check
`journalctl -b <N> | grep dkms` for the actual per-kernel build outcome
when something regresses right after a kernel bump; a failed autobuild for
one or two packages can silently coexist with every other DKMS package
building fine, so `dkms status` alone can look reassuring while a specific
driver is actually missing for the new kernel. Also: don't assume
`hid_device` (or any actively-developed upstream subsystem) field layout
is stable across kernel minor bumps in this "7.x" series — grep the target
kernel's own `/usr/src/kernels/<ver>/include/linux/hid.h` directly instead
of guessing from memory when a similar break happens again.

## Test checklist (carried over, still applicable)

After every test cycle, check:

- Is the patched module actually loaded? `modinfo t2bce | grep srcversion` and
  `/sys/module/t2bce/srcversion` on `mac-t2`.
- `journalctl -k -b 0 --no-pager | grep -iE 'PM: suspend entry|PM: resume
  devices took|suspend_test'` for cycle timestamps.
- Any `EP0 status=3 observed/deferred`? Does the reset/recovery chain reach
  `active=1 paused_by=0 stalled=0 state=0`?
- Any `scheduling while atomic`, `urb_reject=1` outside of an expected
  `EP0 control cancel-request` pairing, or `usb_submit_urb(ctrl) failed`?
- Port 5 raw value trend across `PORT_RESUME` tries — does it ever reach
  `0x215`/ready without a usbcore reset?
- Does `t2bdrm` appear unexpectedly (it should stay unloaded for this test
  setup)?
- Touch Bar: Config 1 or 2? `t2touchbar_kbd` bound, `mode` attribute value?
  Does the graphic actually change or only the logical F-key function?
- `PM: resume devices took ...` duration, and whether the accompanying
  `suspend_test.c` warning is just a consequence of resume length.
