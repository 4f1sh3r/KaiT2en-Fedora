# t2bce_audio Troubleshooting Learnings

Status: 2026-07-10

This file collects findings from troubleshooting the `t2bce_audio` driver
(ALSA driver for the T2 internal speakers/headphones/mic exposed via
`t2bce`). Working notes: what was observed, what was tried, what worked,
what turned out to be a dead end.

## Context

- `t2bce_audio` is one of several `t2bce`/`apple-bce`-derived modules in this
  repo (`t2bce_core`/`t2bce_audio`/`t2bce_dma`/`t2bce_vhci`) and provides ALSA
  playback/capture for the T2 internal audio path.
- Prior work on this branch's predecessor commits (see `main` history, e.g.
  `t2bce_audio: finally fixed the headphone crackling xruns` and the timestamp
  trial-and-error commit before it) already resolved headphone crackling/xrun
  issues. This branch is for the *next* set of `t2bce_audio` problems.
- Related general `t2bce` learnings (clock reliability after resume, ftrace
  debugging technique) live in `local/learnings/t2bce/resume.md` — consult that file for
  cross-cutting `t2bce` infrastructure issues that also affect audio.

## Reproducible symptoms (MacBookPro16,2, reporter: user)

- Regression introduced between commit `6c4a367` (last known-good for this
  machine) and `ca01a06` (current `main`). The effective audio-relevant
  change in that range is commits `4bd459f` and `46a7b12` (`c4a96cb` was
  fully reverted by `d5a7c2d`, net zero).
- On a MacBookPro15,1 (maintainer André's machine) these same commits *fixed*
  post-resume headphone crackling/xruns/stutter. On this MacBookPro16,2 they
  *introduced* a very similar-sounding but not identical symptom, so a plain
  revert is not an option — the fix must work on both.
- Repro: suspend, resume, immediately start audio playback (e.g. YouTube in
  Firefox). Audio plays normally for a few seconds, then the stream audibly
  "breaks" (crackle, then silence); video playback stutters/hangs too. Over
  the following ~5 minutes the problem gradually subsides; after that,
  playback is completely normal again until the next suspend/resume cycle.

## Root-cause hypothesis: T2/host clock-domain mismatch after resume

Commit `46a7b12` changed `t2audio_pcm_pointer()`'s playback-pointer estimate
from being anchored on a **host-captured** timestamp to being anchored on
the **raw device-reported** timestamp:

- Before: `stream->remote_timestamp` was set to `os_timestamp`, which is
  `ktime_get_boottime()` read on the host at the moment the driver processed
  the `T2AUDIO_MSG_UPDATE_TIMESTAMP` message
  (`t2audio_handle_cmd_timestamp()` in `audio.c`). This message is dispatched
  through `schedule_work()` (see `t2audio_bce_in_queue_handle_msg()` /
  `t2audio_deferred_msg_work()` in `protocol_bce.c`), i.e. **not** IRQ
  context — it runs whenever the kernel's system workqueue gets around to
  it. Right after resume, system-wide workqueue congestion (every driver
  reinitializing at once) can delay this significantly, making the captured
  `os_timestamp` stale/jittery. This class of problem matches the general
  [[t2bce_clock_unreliable_after_resume]] finding from the suspend/resume
  work: clock-ish reads can be unreliable for seconds after a real ACPI
  resume, apparently more so on some models than others.
- After `46a7b12`: `stream->remote_timestamp` is set to `dev_timestamp`, the
  **raw `u64` value read straight off the wire** from bridgeOS
  (`t2audio_msg_read_update_timestamp()` in `protocol.c` — no unit/epoch
  conversion happens anywhere in this driver or in `t2bce_core`/mailbox/
  transport; grepped for "clock sync"/"mach_absolute"/similar and found
  nothing). `t2audio_pcm_pointer()` then computes
  `ktime_get_boottime() - stream->remote_timestamp`, i.e. it subtracts a
  **T2-device-clock-domain value from a host-boottime-domain value** and
  treats the result as an elapsed-time-on-host duration. This is only
  correct if the T2's own clock and the host's `CLOCK_BOOTTIME` stay in
  lockstep (same epoch, same rate, including across suspend). This removes
  the host-side workqueue-jitter problem (good, explains the win on 15,1),
  but introduces a new assumption that could plausibly fail differently per
  Mac model/T2 firmware revision (explains the regression on 16,2): if
  bridgeOS's internal clock needs some time after a real resume to
  re-converge with real elapsed time (independent of anything this driver
  controls), the pointer estimate would be off by a growing/shrinking amount
  right after resume, matching the observed multi-second breakage that
  gradually clears up over ~5 minutes.
- This can't be fixed by reading up on it — it's specific to this stateful/
  resume-capable `t2bce` fork (see CLAUDE.md), so no external docs exist.
  Needs to be diagnosed from logs.

## Instrumentation added (branch `t2bce-audio-troubleshoot`)

To test the above hypothesis without reverting the 15,1 fix, added `pr_debug`
markers (enable via Dynamic Debug on the Mac, not `pr_info`, to avoid noise
outside of active investigation):

- `pcm.c` `t2audio_handle_stream_timestamp()`: on every timestamp update,
  logs `t2_delta` (raw device-clock delta since the previous update) next to
  `host_delta` (host-boottime delta since the previous update, in us). If the
  clock-domain-drift hypothesis is right, these two should diverge sharply
  right after resume and converge back towards tracking 1:1 over the
  following minutes.
- `pcm.c` `t2audio_pcm_pointer()`: logs a `ptr jump` whenever the computed
  ALSA pointer jumps forward by more than half the ring buffer between two
  consecutive polls — this is the kind of event that should correlate with
  an audible crackle/dropout.
- All new debug lines stamp themselves with an explicit
  `ktime_get_boottime()`-based `boottime_ms=` field (see
  `t2audio_dbg_boottime_ms()` in `audio.h`), because dmesg's own `[...]`
  timestamp prefix is known to be unreliable for seconds after a real ACPI
  resume ([[t2bce_clock_unreliable_after_resume]]) and can't be trusted to
  correlate events across the resume boundary.
- The existing `pr_info` resume-exit lines in `audio.c`
  (`t2audio_resume()`/`t2audio_resume_work()`) were also given the same
  `boottime_ms=` stamp so the resume-completion instant can be lined up
  against the new debug data.

To capture a repro, enable just these call sites via Dynamic Debug on the
Mac (avoid a blanket `module t2bce_audio +p`, which also lights up
high-frequency unrelated `pr_debug`s):

```sh
echo 'func t2audio_handle_stream_timestamp +p' > /sys/kernel/debug/dynamic_debug/control
echo 'func t2audio_pcm_pointer +p' > /sys/kernel/debug/dynamic_debug/control
```

Then: suspend, resume, start playback immediately, let it run through the
~5 minute degraded window, and capture `dmesg`/`journalctl -k`.

## First captured repro (2026-07-10, MacBookPro16,2)

Test: suspend → resume ~09:33:30 → started YouTube in Firefox immediately →
crackle/dropout/stutter until ~09:35 → then fine. Captured `dmesg` (our new
`pr_debug`s, correlated via `boottime_ms=`) and `journalctl --user -u
pipewire -u wireplumber -u pipewire-pulse` for the same window.

**The raw clock-domain-mismatch hypothesis above is NOT the direct
explanation** — once corrected for units (both `dev_timestamp` and
`os_timestamp` turned out to already be ns-since-boot, closely aligned, e.g.
`t2=177109255247 host=177114014492` — under 5ms apart even in the very first
post-resume timestamp) and once a genuine "have_prev" pair of consecutive
`t2audio_handle_stream_timestamp()` calls on the *same* stream lifetime was
captured, `t2_delta` vs. `host_delta` only ever diverged by low single-digit
milliseconds, converging further within a few seconds. That part of the
theory is essentially debunked as originally framed (there's no large
multi-second clock skew that slowly re-converges).

**What is actually happening: a resume-triggered XRUN/restart storm**,
confirmed independently in two places:

- Kernel side: counted `t2audio_handle_stream_timestamp()` "first ts" events
  (each one = one `t2audio_pcm_start()`, i.e. one ALSA `TRIGGER_START`) per
  10s bucket of `boottime_ms`:
  ```
  150-160s   0 restarts  (steady state, pre-resume)
  190-200s  22 restarts  (storm starts — right when playback was started)
  200-210s  83 restarts  (peak: ~8.5 restarts/sec)
  210-220s  86 restarts  (peak continues)
  220-260s  53-59/10s    (storm continues, slower)
  260s+      0 restarts  (back to steady state)
  ```
  Storm ran from `boottime_ms≈194720` to `≈259699`, i.e. **exactly a 65
  second window**, starting ~18s after the resume-complete marker
  (`boottime_ms=176363`, i.e. ~09:33:39) — the 18s gap is just human reaction
  time to open Firefox and hit play.
- Userspace side: `journalctl` for the same wall-clock window shows PipeWire
  logging repeated `spa.alsa: hw:Audio,0p: (N suppressed) snd_pcm_avail
  after recover: Broken pipe` from **09:33:58 to 09:35:01** — i.e. PipeWire
  is genuinely getting `-EPIPE` (XRUN) from `snd_pcm_avail()` on our ALSA
  device and auto-recovering (stop+prepare+start) over and over, with each
  logged line's "(N suppressed)" indicating several more recoveries happened
  in between. This end time (09:35:01) matches the kernel-side storm end
  almost to the second, and matches the user's "bis 9:35" report closely.
- The `ptr jump` diagnostic mostly just re-detects the same restarts (a
  fresh stream's pointer legitimately starts back down near
  `stream->latency`, ~320-325 frames out of a 16640-frame ring) rather than
  showing an independent mid-stream glitch — most restarted streams die
  before even a second pointer poll, so this metric didn't add information
  beyond the "first ts" restart count.

**Conclusion so far:** something makes `t2audio_pcm_pointer()`'s hw_ptr
estimate look invalid to ALSA's own xrun detection almost immediately after
each fresh `t2audio_pcm_start()`, for about a minute after a real resume,
even though the underlying `dev_timestamp`/`os_timestamp` values we can see
are not wildly off. Two components remain unexplained and are the next
things to instrument:
1. What exactly trips `-EPIPE` in `snd_pcm_avail()` — need the ALSA-visible
   hw_ptr sequence, not just our raw timestamps. Candidate: the
   `waiting_for_first_ts` fallback window (`start_io_time`-anchored
   estimate, used until the *first* post-restart timestamp arrives) might be
   producing an implausible jump before the real `remote_timestamp` kicks
   in, especially if that window is longer/more variable right after resume.
2. Why does the underlying condition need up to ~65s (not just a few
   seconds) to clear, when our directly observed clock deltas look fine
   within seconds of resume completing? This suggests the real bottleneck is
   elsewhere (BCE mailbox/transport congestion from other clients
   re-initializing post-resume?) and is not something visible in the audio
   module's own timestamp math.

## Dead ends

- Chasing a large, slowly-reconverging T2-clock-vs-host-boottime skew as the
  *direct* cause of the crackle: the measured deltas are too small (single-
  digit ms) and converge too fast (seconds) to explain a 65s-long symptom
  window. The 46a7b12 clock-domain change may still matter as a contributing
  factor (removing host-capture jitter helped 15,1), but it is not, by
  itself, what's producing this specific 16,2 regression. Don't re-chase
  this same framing without new evidence.

## Open questions

- Confirmed: this is a resume-triggered ALSA XRUN/restart storm lasting
  ~65s, not a slow clock drift. See captured-repro section above.
- Also worth checking: does the *very first* `t2audio_pcm_start()` after
  resume (the one at `boottime_ms=177114`, well before the storm at
  `194720`) succeed and play cleanly for those ~18s the user was clicking
  around, or does the storm actually start on the very first playback
  attempt? (Can't tell from current data — no independent signal for "audio
  was audibly fine" vs. "no audio was playing yet" in that gap.)

## Instrumentation round 2: where does the `waiting_for_first_ts` window go?

Added to `t2audio_handle_stream_timestamp()`'s existing "first ts" log
(`pcm.c`): a `fallback_us=` field measuring how long
`t2audio_pcm_pointer()` had to run on the `start_io_time`-anchored,
host-clock-only fallback estimate before the real bridgeOS timestamp
notification actually arrived for that stream generation.

Also enabling two more existing (previously dormant) `pr_debug` call sites
that were already in the code before this investigation started, to
correlate against the fallback duration:

- `t2audio_pcm_start()` — logs `start_io %s %lld us`, the time for the
  *synchronous* `t2audio_cmd_start_io()` mailbox round-trip
  (`t2audio_send_cmd_sync`, 500ms timeout) to complete. If this stays fast
  during the storm while `fallback_us` balloons, the bottleneck is
  specifically in delivering the async timestamp notification afterwards
  (dispatched via `schedule_work()` in `protocol_bce.c`, see
  [[t2bce_clock_unreliable_after_resume]] for the general post-resume
  workqueue-congestion pattern) rather than in the command round-trip
  itself. If `start_io` itself is slow, the bottleneck is earlier, in the
  mailbox/command path shared with the rest of `t2bce_core`.
- `t2audio_pcm_trigger()` — logs `TRIGGER START %s` / `TRIGGER STOP %s`, to
  see restart cadence directly instead of only inferring it from "first ts"
  events.

Enable via:
```sh
echo 'func t2audio_handle_stream_timestamp +p' > /sys/kernel/debug/dynamic_debug/control
echo 'func t2audio_pcm_pointer +p' > /sys/kernel/debug/dynamic_debug/control
echo 'func t2audio_pcm_start +p' > /sys/kernel/debug/dynamic_debug/control
echo 'func t2audio_pcm_trigger +p' > /sys/kernel/debug/dynamic_debug/control
```

## Second captured repro (2026-07-10, MacBookPro16,2) — both prior hypotheses ruled out

Test: two suspend/resume cycles back to back. First resume (`boottime_ms=144771`,
~09:55:04): only brief stutter, and the log shows **zero restarts** in the
following ~55s (steady `ts update` cadence throughout, `FIRST=0` in every
10s bucket up to the second resume) — so whatever the user perceived as a
brief stutter there didn't produce a kernel-visible restart/xrun at all
(possibly just normal stream-open latency, unrelated to the bug under
investigation). Second resume (`boottime_ms=208743`, ~09:55:56): full storm,
starting at `boottime_ms≈250000` (~41s after resume-complete) and ending at
`boottime_ms≈290000` (~81s after resume-complete) — matching "beruhigte sich
während 9:57" almost exactly.

**Cross-checked against round 1**: storm-end-minus-resume-complete was
**83.3s** in round 1 (`259699-176363`) and **81.3s** in round 2
(`290000-208743`) — two independent resume events landing within ~2 seconds
of each other on a ~80s total duration, despite the storm *starting* at very
different offsets after resume (18s vs. 41s) and the two resumes happening
minutes apart with different amounts of the storm's own internal state
carried over. This is a real signal, not coincidence: something clears at a
roughly fixed ~80s mark after resume, largely independent of when this
audio module's own activity (stream restarts) happens to begin.

**This round's new instrumentation ruled out both standing hypotheses** —
every per-restart metric we can observe from inside `t2bce_audio` looked
completely normal throughout the entire storm:
- `start_io` (the synchronous `t2audio_cmd_start_io()` mailbox round-trip,
  `t2audio_send_cmd_sync`, 500ms timeout): a **rock-steady ~38-40ms**
  throughout the storm, identical in magnitude to steady-state / pre-storm
  values (`38933us` right after resume #1, no storm; `38147us` right after
  resume #2, into the storm; `~38000-41000us` sustained *through* the whole
  storm). Not congested, not degraded, not correlated with storm intensity.
- `fallback_us` (time spent on the `start_io_time` host-clock fallback
  before the real bridgeOS timestamp notification arrives): **near zero
  (-216 to +920us)** throughout the storm — actually *smaller* than the
  pre-storm baseline (~920us). So the async notification dispatch is not
  delayed either; ruling out the [[t2bce_clock_unreliable_after_resume]]-
  style workqueue-congestion theory as the direct mechanism here.
- `t2` vs. `host` absolute values at each "first ts" stayed within ~5-7ms of
  each other throughout the storm — no sign of a growing clock-domain gap.

So the bottleneck is **not** host-capture jitter, **not** delayed async
notification delivery, and **not** an obviously growing clock skew. Every
number we can see from inside this module looks healthy at every single
restart, and yet PipeWire (`journalctl`, confirmed independently again)
still gets genuine `-EPIPE` from `snd_pcm_avail()` and restarts every
~125-140ms for the ~80s window.

**New working theory:** ~125-140ms per restart cycle is suspiciously close
in magnitude to the ~38-40ms `t2audio_pcm_trigger(START)` blocking mailbox
round-trip (plus a STOP round-trip, plus PipeWire's own recovery overhead).
In steady state this ~40ms one-time cost at stream-open is invisible — it
only happens once. But *if* something external causes one genuine xrun
shortly after resume (still unexplained — no local metric shows it), our
`.trigger()` implementation's synchronous, blocking, ~40ms-per-command
mailbox round-trip may itself be too slow for PipeWire's real-time
scheduling budget on *every recovery attempt*, making the storm
self-perpetuating independent of whatever originally triggered it — which
would explain why component-level metrics look fine (they are fine) while
the storm still grinds on. This is a hypothesis, not yet confirmed.

## Open questions

- Confirmed: this is a resume-triggered ALSA XRUN/restart storm lasting
  ~65-80s, not a slow clock drift, and not explained by host-capture jitter
  or delayed async notification delivery (both directly measured and ruled
  out in round 2).
- The consistent ~80s storm-clears-after-resume figure (83.3s and 81.3s in
  two independent tests) suggests a fixed-duration process elsewhere in the
  system (kernel timekeeping recalibration, another `t2bce_*` client's
  post-resume re-init competing for the shared BCE mailbox, or a
  bridgeOS-internal stabilization timer) is the real root cause — not
  something local to `t2bce_audio`'s own clock/pointer math, which has now
  been checked in detail and looks fine throughout.
- Next step: capture **full system `dmesg`** (not just `t2bce_audio`) for
  the next repro, to look for what else is active system-wide in the first
  ~80s after resume (`t2bce_vhci` USB re-enumeration, other `t2bce_*`
  clients, ACPI, NetworkManager, etc.) — pivot the search outward from this
  module. Also worth pulling ALSA's own authoritative xrun view
  (`/proc/asound/card*/pcm*/sub0/status`) live during a storm rather than
  only inferring recovery timing from PipeWire's rate-limited log lines.
- The self-perpetuating-restart theory (blocking `.trigger()` mailbox
  round-trip too slow for PipeWire's RT budget once *any* xrun has
  occurred) is untested — would need to check whether making `.trigger()`
  faster/non-blocking changes the storm's behavior, but that's a real
  driver change with real risk and shouldn't be attempted before the
  external-cause investigation above rules in or out a simpler root cause.

## Third captured repro (2026-07-10) — root cause found via ALSA's own status

Added a second background capture alongside the existing `t2bce_audio`
`pr_debug`s: full `journalctl -k -f` (not filtered to this module) and a
25ms poll of `/proc/asound/card0/pcm0p/sub0/status` (the Speaker playback
substream) into `/tmp/alsa_status_poll.log` on the Mac, running for the
whole test so we don't need to be present at the exact moment.

**System-wide log shows nothing else running for the storm's duration.**
All non-`t2bce_audio` kernel activity around this resume (CPU onlining,
`t2bce_vhci`/`t2bce_core` resume, USB re-enum of the internal
keyboard/trackpad, ~40 `efivarfs: removing variable` lines) completes within
the first ~250ms after resume and does not recur. This argues against a
shared-mailbox-congestion-from-another-client theory. Storm duration this
run was **~102s** (vs. 83.3s and 81.3s in the first two runs) — same order
of magnitude but not as precisely fixed as first thought; not a hard timer.

**ALSA's own `/proc/.../status` gives the actual mechanism, unambiguously:**
of 795 sampled `state: XRUN` snapshots during the storm, **791 (99.5%) have
`avail` greater than the 16640-frame ring buffer size** — a value that is
physically impossible for a healthy stream and is exactly what trips ALSA's
own xrun detection. Concretely, right after almost every restart:
```
RUNNING  hw=0    appl=2048  avail=14592   (healthy: buffer_size - (appl-hw))
XRUN     hw=15529 appl=2048 avail=30121   (hw raced 13481 frames past appl!)
```
`hw_ptr` (built from our `.pointer()` return values) races **past**
`appl_ptr` — i.e. our driver is reporting to ALSA that hardware has already
consumed more audio than the application has even written yet. That is not
a clock-domain-alignment nuance; it's `t2audio_pcm_pointer()`'s fundamental
design: it estimates hw position purely from
`ktime_get_boottime() - stream->{start_io_time,remote_timestamp}`, i.e. pure
wall-clock interpolation assuming uninterrupted playback since start,
**with no check against how much data `appl_ptr` has actually provided.**
Real hardware with a physical DMA-position register cannot outrun the data
it was given; this software/timer-based stand-in can, and does, whenever
the app doesn't refill fast enough after a fresh `TRIGGER_START`
(`appl_ptr` sits at a consistent `2048` right after restart across every
sample here — one PipeWire quantum's worth of pre-buffered data).

**Working theory tying it together:** post-resume, something makes the
system momentarily less responsive in a way that isn't visible as any
single logged subsystem (general scheduling/CPU pressure from many
processes doing their own post-resume reconciliation work — compositor,
NetworkManager, systemd, etc. — is a plausible diffuse cause, consistent
with the general post-resume-unreliability pattern in
[[t2bce_clock_unreliable_after_resume]], just broader than clock reads).
PipeWire's real-time thread occasionally can't refill the ~2048-frame
pre-buffer before our pointer estimate (which has zero safety margin,
being pure elapsed-wall-clock-time math) reports that data as already
consumed → ALSA sees `avail > buffer_size` → XRUN → PipeWire recovers via
STOP+`prepare`+START, which repeats the exact same tight-margin startup
sequence → same failure mode → storm, until by chance a cycle lands
favorably enough (or system load eases) to break out.

**Fix implemented** (branch `t2bce-audio-troubleshoot`, `pcm.c`/`audio.h`):
`t2audio_pcm_pointer()` now clamps its playback pointer estimate so the
forward delta reported since the last poll can never exceed how many new
frames `appl_ptr` actually made available since that same poll:
- New per-stream fields `dbg_last_ptr` (last raw pointer returned) and
  `last_appl_ptr` (appl_ptr snapshot at that same poll), seeded in
  `t2audio_pcm_start()` for playback right when the DMA is (re)started —
  `dbg_last_ptr = 0`, `last_appl_ptr = appl_ptr` at that moment — so even
  the very first poll after a restart is clamped, not just later ones.
- In `t2audio_pcm_pointer()`, after computing the existing wall-clock
  estimate: `hw_delta` (forward delta since last poll, wraparound-corrected)
  is capped to `appl_delta` (frames appl_ptr advanced since last poll).
  Logged via `pr_debug_ratelimited("pointer throttled ...")` whenever the
  clamp actually engages.
- Removed the old `dbg_last_ptr_valid = false` reset in the "first ts"
  handler (`t2audio_handle_stream_timestamp()`) — it would have thrown away
  the seeded baseline right at the fallback→real-timestamp transition, which
  is exactly the moment a discontinuity is most likely and most needs the
  clamp.
- Capture direction is untouched (clamp is `SNDRV_PCM_STREAM_PLAYBACK`-only;
  the observed failure was playback-only and the invariant doesn't apply the
  same way to capture).

This doesn't fix *why* the system is momentarily slower for up to ~100s
after a real resume — it makes that slowness degrade to harmless throttling
(driver holds at the edge of available data) instead of a cascading
XRUN/restart storm. Verification pending: re-run the same repro with the
`ptr jump` and `pointer throttled` debug lines enabled and confirm the
storm (`FIRST`/restart event rate) is gone or much reduced, while
`pointer throttled` lines appear during the window that used to be the
storm.

**Fix reverted — it deadlocked audio entirely, no suspend/resume needed.**
On the very next boot with this fix loaded, audio broke immediately: the
GNOME volume-change "plop" looped for ~30s right after login, and the first
real playback attempt hung before producing any sound — with no
suspend/resume cycle involved at all. Root cause of the clamp itself: it's
circular. `hw_delta` (how far the reported hw pointer may advance this
poll) was capped to `appl_delta` (how far `appl_ptr` advanced since the
last poll). But ALSA's `avail` is computed from the *gap* between `appl_ptr`
and `hw_ptr` — a hw pointer that's artificially held back makes `avail`
look smaller than it really is, so PipeWire (reading a falsely-low `avail`)
writes less or stalls, which shrinks `appl_delta` on the next poll, which
clamps `hw_delta` even further. This is a self-reinforcing feedback loop
with a stable fixed point at zero throughput, and nothing in the design
breaks it — it isn't specific to the post-resume window, it can trigger any
time the driver's `.pointer()` poll cadence and PipeWire's write cadence
interleave unfavorably, which is apparently very likely right after a fresh
`TRIGGER_START` even outside of resume.

Reverted in full: removed the `hw_delta`/`appl_delta` clamp block from
`t2audio_pcm_pointer()`, the seeding block from `t2audio_pcm_start()`, and
the now-unused `last_appl_ptr` field from `struct t2audio_stream`
(`audio.h`). Kept the harmless diagnostics-only `dbg_last_ptr`/
`dbg_last_ptr_valid` fields and the `ptr jump` log line, which only read
state and don't affect the reported pointer value. Verified working again
on the Mac (audio confirmed fine without suspend/resume; suspend/resume not
yet re-tested after this revert).

**Lesson for any future fix attempt:** do not make the reported hw pointer
depend on `appl_ptr`/how much userspace has written. The pointer's whole
job in this driver is to tell userspace how much room is free — feeding
that decision back into itself is inherently circular. A safe fix needs to
bound the wall-clock estimate using only information that cannot be
influenced by the consumer's own behavior in response to that same
estimate — e.g. clamping against `dbg_last_ptr` plus a hard maximum
per-poll advance derived purely from elapsed time and rate (already
implicit in the existing formula), or detecting the `avail > buffer_size`
impossible state directly and holding the pointer flat for that one poll
only (a one-shot correction, not a standing dependency on `appl_ptr`).

**Second fix attempt (2026-07-10) — absolute ceiling instead of per-poll
delta throttle.** Re-implemented in `t2audio_pcm_pointer()` only (no changes
to `t2audio_pcm_start()` or `audio.h` needed this time), playback-only:

```c
if (substream->stream == SNDRV_PCM_STREAM_PLAYBACK) {
    ktime_t since_start = ktime_get_boottime() - stream->start_io_time;
    u64 nominal_frames = (u64) ktime_to_ns(since_start) * substream->runtime->rate / NSEC_PER_SEC;
    snd_pcm_uframes_t appl_now = substream->runtime->control->appl_ptr;

    if (nominal_frames > appl_now)
        frames = (snd_pcm_sframes_t) (appl_now % substream->runtime->buffer_size);
}
```

Key design difference from the reverted attempt: this compares two
**absolute, monotonic, from-the-same-zero-point** quantities recomputed
fresh on every single poll — "wall-clock ns since `start_io_time` (fixed at
`TRIGGER_START`, never moves) times the nominal rate" vs. "`appl_ptr` right
now" (already a from-zero running counter since the last prepare/reset,
per ALSA core semantics: `SNDRV_PCM_IOCTL_PREPARE` resets both `hw_ptr` and
`appl_ptr` to 0, same zero point our DMA restart uses). There is no
per-poll delta and no dependency on "how much did `appl_ptr` grow *since
the last poll call*" — that specific framing is what made the reverted
attempt circular. Here, any already-written backlog (e.g. the ~2048-frame
PipeWire pre-buffer at a fresh restart) is immediately available to drain
at full nominal rate with zero throttling, since `appl_now` already
includes it; the clamp only ever engages at the genuine edge of available
data, and un-clamps the instant more is written, with no lag or ratchet
effect carried between polls. Reasoned through the monotonicity on paper:
both operands are non-decreasing over time, so their min (what the clamp
effectively computes) is also non-decreasing — no backward pointer jumps.

Deliberately did **not** try to correct the separate, pre-existing
fallback→real-timestamp epoch-swap discontinuity (`time_from_start`'s
reference switches from `start_io_time` to `remote_timestamp` once the
first bridgeOS timestamp arrives, which can itself cause a jump — this is
what the "ptr jump" diagnostic already watches for). This ceiling is
anchored to `start_io_time` only, which never changes for the life of a
`TRIGGER_START`, so it's unaffected by that swap either way; fixing the
swap itself is out of scope here and remains an open question.

Built and loaded cleanly (DKMS build had no warnings/errors besides the
known-benign `pahole` notice), rebooted (required — hot-reload of
`t2bce_audio` is blocked by `t2bce_core`'s PM-client reference), verified
installed/loaded `srcversion` match (`CF8A89E2A95D8B3E087D492`) before
handing off for testing. New debug line to watch for during a repro:
`pointer capped %s nominal=%llu appl=%lu boottime_ms=%lld` (only fires
while the clamp is actually engaged).

Verification pending: (1) basic playback/GNOME volume-plop sanity check
(the thing that broke immediately last time, before suspend/resume was
even needed), (2) the original suspend/resume repro (standby → resume →
playback, watch for the XRUN/restart storm over the ~65-102s window that
this whole investigation is trying to fix).

**Second fix attempt — result: failed, same storm.** Basic playback sanity
check passed this time (no immediate deadlock like the first attempt), but
the suspend/resume repro still showed the full crackle/stutter storm
("Test ist durch, der Fehler ist weiterhin vorhanden. Es stottert und
ruckelt weiterhin."). Root cause of the failure, understood in hindsight
once the third attempt's fresh data came in: this ceiling reused
`time_from_start` (via `nominal_frames`) for its "wall-clock since start"
side, but *only* the `start_io_time`-anchored value — it does not, on its
own, explain the failure. The real problem turned out to be one level
deeper (see [[t2bce_clock_unreliable_after_resume]]-adjacent finding in
the fourth attempt below): the underlying wall-clock/device-clock estimate
itself can be so far off after a real resume that even a same-zero-point
absolute ceiling isn't a tight enough bound — see third attempt for the
next refinement.

## Third fix attempt (2026-07-10) — monotonic incremental accumulator

Replaced the absolute-ceiling approach with a monotonic accumulator
(`hw_frames_since_start`, new field in `audio.h`) fed by small incremental
deltas instead of a recomputed-from-scratch absolute estimate:

- Each poll computes `raw_delta` = the wrapped delta of the existing
  wall-clock `frames` estimate since the previous poll
  (`hw_accum_last_raw`), then adds `min(raw_delta, backlog)` to the
  accumulator, where `backlog = appl_ptr - hw_frames_since_start` (host-side
  only, not clock-dependent). The reported pointer becomes
  `hw_frames_since_start % buffer_size`.
- By construction the accumulator can never exceed `appl_ptr` — not
  "usually doesn't", provably cannot, regardless of how wrong the raw
  estimate is, since even a garbage `raw_delta` is capped by `backlog`
  every single poll. It also never needs a large corrective jump (unlike
  the first and second attempts), since it only ever advances by small
  incremental steps.
- New fields in `audio.h`: `hw_frames_since_start`, `hw_accum_last_raw`,
  `hw_accum_valid` (the latter two reset at the start of every
  `t2audio_pcm_start()`).

Built, loaded, verified `srcversion` match, rebooted. **Result: failed —
same storm, confirmed by user** ("Test ist durch, der Fehler ist weiterhin
vorhanden. Es stottert und ruckelt weiterhin.").

**Root cause of the failure, found via full log correlation:** captured a
suspend/resume repro with both the existing `t2bce_audio` `pr_debug`s and
a full-system `journalctl -k -f` + 25ms `/proc/.../status` poll running
throughout. Cross-referencing every "pointer capped" accumulator-safety-net
log line against whether it happened before or after the "first ts on" log
line (i.e. before or after `time_from_start`'s anchor switched from
`stream->start_io_time` to `stream->remote_timestamp`, which happens once
the first post-restart bridgeOS timestamp notification arrives) showed
**148 out of 148** capping events occurred strictly *after* that switch;
**zero** occurred while still on the `start_io_time`-only fallback.
Separately, correlating the pre-existing `ts update` diagnostic
(`t2_delta` vs. `host_delta`) against wall-clock time since resume showed
the *device* clock (`remote_timestamp`/`dev_timestamp`) runs up to **~25%
fast/slow** for several seconds immediately after a real ACPI resume
(ratio up to 1.25139), settling back to within ~0.1% within a few hundred
ms to ~1s. So the accumulator itself was working exactly as designed
(mathematically bounded), but the raw estimate it was bounding was
severely wrong during this window because of the anchor switch to a
device clock that is itself unreliable right after resume — the same
class of unreliability as [[t2bce_clock_unreliable_after_resume]], now
shown to also apply to bridgeOS's own device-clock timestamps, not just
host-side `ktime_get()`/printk. Clamping alone wasn't enough to prevent
audible stuttering, since the clamping frequency/magnitude still
corresponded to real, audible pointer-position errors and their
correction cycles.

## Fourth fix attempt (2026-07-10) — host-clock-only anchor, never switch to device clock

Given the above, removed the anchor-switching behavior entirely in
`t2audio_pcm_pointer()`: `time_from_start` is now **always** computed from
`stream->start_io_time` (host `ktime_get_boottime()`), never from
`stream->remote_timestamp` (device clock), regardless of
`waiting_for_first_ts`. Rationale: host-oscillator drift over one buffer
period (a few hundred ms) is negligible, so there's no accuracy upside to
the device-clock anchor that's worth the confirmed overshoot risk. The
third attempt's accumulator stays in place underneath as an additional
safety net, unchanged.

Built, loaded, verified `srcversion` match, rebooted, dynamic debug
re-enabled. Basic playback sanity check passed ("Läuft sauber").

**Result on the original storm: fixed, confirmed across two independent
suspend/resume repros.** Both test runs (`alsa_status_poll_v4.log`/
`journal_resume_test_v4.log` and the `_v4b` pair) show **0** `state: XRUN`,
**0** "pointer capped" accumulator events, and **0**/2 "ptr jump"
diagnostic lines (the 2 in test 1 were not accompanied by any XRUN or
capping and are unexplained but harmless-looking). Added a third logger
this round, `pw-top -b -n 1` polled every 0.5s into `pw_top_poll.log`, to
get PipeWire's own independent xrun/error view (its `ERR` column per
node) rather than relying solely on the kernel-side ALSA status file —
confirmed **0** PipeWire-side errors on the `Speaker` sink across the
`_v4b` test too. The crackle/restart storm this whole investigation was
chasing does appear to be gone.

**But: a second, previously-masked bug surfaced once the storm stopped
hiding it — silent audio dropout with zero errors anywhere.** Both `_v4`
test runs independently reproduced total audio silence (video keeps
playing, no crackle, no error) that only cleared after the user paused
and restarted playback (or, in test 1, after PipeWire itself closed and
reopened the stream). Timing was inconsistent between the two occurrences
(~3.5 minutes into playback in test 1 vs. ~13 seconds after resume in
test 2), so it isn't tightly resume-bound the way the storm was.

Test 2 (`_v4b`) is the cleaner case to reason from: the ALSA substream
stayed in `state: RUNNING` for the *entire* ~2m41s silent window (no
`TRIGGER STOP`/close until the user paused the video). Extracting
`hw_ptr` from the 25ms status poll across that exact window
(1783676835.0–1783676996.0) shows it advancing at a **rock-steady
47998–48002 Hz** in every one of eight consecutive 20-second sub-windows
— i.e. textbook-perfect, uninterrupted progress reported by the driver
for the full 2m41s during which the user heard nothing. The driver had no
way to notice anything was wrong.

**Why: this isn't a v4 regression, it's a structural limitation of the
whole `.pointer()` design that predates this entire investigation.**
Checked the wire protocol directly (`protocol.h`,
`T2AUDIO_MSG_UPDATE_TIMESTAMP` / `t2audio_msg_read_update_timestamp()`):
bridgeOS only ever sends a `timestamp` (device clock reading) and
`update_seed` — there is no field anywhere in this protocol carrying
"frames actually consumed/played". `t2audio_pcm_pointer()` has never had
real DMA-position readback in any of the four fix attempts above; it has
always been pure elapsed-time interpolation (`ktime_get_boottime()` minus
some anchor, times nominal rate), just choosing between two different
*clocks* to interpolate against — never a real progress signal. So
switching the anchor (as attempts 1-4 all did, in different ways) can
only ever change how badly the estimate diverges from reality when
something upstream goes wrong; it can never let the driver detect that
divergence, because no available signal reflects real playback progress.
The storm (v1-v3) and this silent dropout are therefore best understood
as **two different failure modes of the same open-loop design**: one
where the estimate visibly overshoots `appl_ptr` (audible, XRUN-detected,
now fixed by v4's anchor choice), and one where the estimate stays
plausible while the real hardware output has stopped for an unrelated
reason (inaudible to any current instrumentation, unaffected by anchor
choice).

**What actually silences the speaker for those minutes is still
unknown** — it's downstream of everything `t2bce_audio`'s ring-buffer
protocol can see. Investigated one specific theory (relayed secondhand
from maintainer André: "some old apple-bce patch that resets the camera
or something" disrupting audio as a side effect) and could not find a
target for it in this repo: there is no camera/FaceTime/UVC driver or
module anywhere under `modules/` (checked: `hid_t2magicmouse`,
`t2bce_audio(-alsa-ucm-conf)`, `t2bce_core`, `t2bce_dma`, `t2bce_vhci`,
`t2bdrm`, `t2gmux`, `t2mfi_fastcharge`, `t2smc`, `t2touchbar` — none of
these touch a camera), and neither `_v4`/`_v4b` test's full kernel
journal shows any camera-related activity at all during or around the
dropout. The specific theory as stated doesn't apply to this codebase,
though the broader idea — some other resume-time activity disrupting
real audio output outside this module's visibility — remains plausible
and would need different instrumentation (something with visibility into
actual codec/mute/routing state, not clock or pointer logs) to chase
further.

**Status:** kept the v4 anchor change as-is (real, confirmed fix for the
storm; not reverted). Not yet committed — the silent-dropout bug means
audio is still not reliably stable end-to-end, which is the actual
top-level goal, so this isn't a shippable state yet. The silent-dropout
investigation is a separate, open thread from the storm investigation
above.

**Correction to the paragraph above:** the "structural limitation,
predates this investigation" framing is wrong and is superseded by the
control test below — the dropout is caused by v4's missing re-anchor,
not by the protocol's lack of a frames-consumed feedback field. That
protocol fact is real (checked directly), but it doesn't by itself
explain the dropout, since the pre-regression baseline has the exact
same protocol and does not drop out.

## Control test: commit 6c4a367 (pre-regression baseline), full repro on this 16,2

At the user's request, built and installed commit `6c4a367` (the
documented last-known-good commit for this machine, from *before*
`c4a96cb`/`4bd459f`/`d5a7c2d`/`46a7b12`, i.e. before any of the regression
range or any of this session's four fix attempts) in a separate git
worktree, alongside a re-run of `install-alsa-ucm.sh` (`6c4a367` also
carries the "let UCM handle jack routing" change, `audio.c`/UCM config
content is byte-identical between `6c4a367` and current `main`, so this
was precautionary, not expected to matter).

Build/install surfaced an unrelated infra landmine, fixed along the way:
`dkms.service` runs `dkms autoinstall` on every boot and had silently
re-installed **stale, orphaned** `t2bce_core/0.07` and `t2bce_vhci/0.02`
builds on disk after our correct `0.06`/`0.01` install + reboot — neither
version number exists in the repo at `6c4a367` or `HEAD`, they were
leftover registrations from an earlier session, never cleaned up. The
*loaded* kernel modules were still correct, but the *on-disk* `.ko` files
(what the next reboot would load) were not — purged the stale versions
from DKMS so this can't silently bite on a future reboot. Worth
remembering: DKMS bookkeeping on this Mac can carry orphaned versions
across sessions; `dkms status` occasionally deserves a glance, and
loaded-vs-installed `srcversion` should be checked with `modinfo` (on-disk)
*and* `/sys/module/.../srcversion` (loaded) both, not just one.

**Result: completely clean.** Same suspend/resume repro as the `_v4`/
`_v4b` tests (suspend → resume → immediate playback → run several
minutes), confirmed by the user: "Die Version läuft perfekt... der Ton
läuft die ganze Zeit super sauber weiter." No storm, no silent dropout —
on this same 16,2, same test methodology, same session.

**Why this matters:** it disproves the "structural, unfixable" framing
above. `6c4a367`'s `.pointer()` is *also* pure open-loop wall-clock
interpolation with *also* no frames-consumed feedback from bridgeOS — the
protocol limitation is real and identical in both versions. The
difference that matters is in `t2audio_handle_stream_timestamp()`: at
`6c4a367`, `stream->remote_timestamp` is set **unconditionally, on every
single timestamp update**, from `os_timestamp` (host-captured, at that
point always). `t2audio_pcm_pointer()` re-derives `time_from_start` from
this freshly-updated anchor on every poll. This means the interpolation
window between corrections is bounded to roughly one buffer period
(~346ms on this hardware) — accumulated error can never grow beyond what
one period's worth of nominal-vs-real playback-rate mismatch can produce,
because every ~346ms the anchor snaps back to "now" (`ktime_get_boottime()`
captured essentially at the moment of that update). v4 broke exactly this
property: it anchors once at `t2audio_pcm_start()` and never updates
`time_from_start`'s reference again for the rest of the stream's life
(`stream->remote_timestamp` is set correctly and updated on the standard
cadence by the timestamp handler as always, but v4's `pointer()` simply
stopped reading it) — so any per-period rate mismatch between the host
oscillator and bridgeOS's actual playback rate accumulates without bound
for as long as the stream runs, instead of being corrected away every
~346ms. Over a few minutes this is enough for the host's belief about ring
position to diverge from bridgeOS's *real* internal read position by more
than PipeWire's write cadence can tolerate — plausible mechanism: PipeWire,
trusting our (increasingly wrong) `avail`, overwrites ring memory bridgeOS
hasn't actually read yet (or fails to refill memory it has), and
bridgeOS's own DMA engine — which we have zero visibility into — hits a
real underrun/desync and mutes, with nothing in this protocol to report
that back to the host. This is a hypothesis for the *specific hardware
mechanism*; what's now directly confirmed by the control test is that
*something* about running open-loop for the full stream lifetime (v4),
rather than re-anchoring every period (6c4a367 and all versions before
v4), is what causes it.

**Why not just revert to 6c4a367's pcm.c wholesale:** `46a7b12`
("finally fixed the headphone crackling xruns") is what changed the
anchor's clock source from `os_timestamp` to `dev_timestamp` in the first
place, and per the user (relaying community experience) this machine is
"eher die Ausnahme als die Regel" — the newer patches fixed real problems
for other T2 Macs, most plausibly by removing host-capture jitter: the
timestamp handler runs via `schedule_work()` on the shared system
workqueue (`t2audio_bce_in_queue_handle_msg()` →
`t2audio_deferred_msg_work()` → `t2audio_handle_cmd_timestamp()`, see
`protocol_bce.c`/`audio.c`), not IRQ context, so `os_timestamp` can carry
scheduling jitter on machines/loads where that queue is busier than it
is on this one. A wholesale revert would hand that jitter back to
whichever machines `46a7b12` helped. Can't verify that class of hardware
from this Mac — the fix has to avoid regressing it by construction, not
by hope.

## Fifth fix attempt (2026-07-10) — periodic re-anchor + windowed device-clock distrust

Combines both findings into a minimal, surgical change (`pcm.c` only):

1. **Restored periodic re-anchoring** in `t2audio_pcm_pointer()`: back to
   `stream->start_io_time` while `waiting_for_first_ts`, else
   `stream->remote_timestamp` (updated every message) — i.e. exactly
   `6c4a367`'s structure, undoing only v4's single-anchor regression. This
   alone is expected to fix the silent dropout.
2. **Added a per-message device-clock trust check** in
   `t2audio_handle_stream_timestamp()`, gating what `remote_timestamp` gets
   set to *without* touching the historical unconditional-`dev_timestamp`
   default: `dev_timestamp` and `os_timestamp` are both ns-since-boot in
   the same nominal epoch (confirmed in the first captured repro — under
   5ms apart even in the very first post-resume timestamp, under normal
   conditions). Computing `dev_host_gap_ns = dev_timestamp -
   ktime_to_ns(os_timestamp)` directly (no history needed, works even on
   the very first post-restart message) and comparing against a new
   `T2AUDIO_DEV_CLOCK_MAX_GAP_NS` (30ms — well above observed steady-state
   noise, well below one ~346ms buffer period so real drift is caught
   early): if the gap is implausible, `remote_timestamp` falls back to
   `os_timestamp` for *that update only*; every other update keeps using
   `dev_timestamp` exactly as `46a7b12` intended. Logs
   `pr_debug_ratelimited("dev clock distrusted ...")` when the fallback
   engages, so a future capture can directly see how often/when this
   triggers relative to resume.
3. Left the third attempt's monotonic accumulator
   (`hw_frames_since_start`) in place as a defense-in-depth safety net —
   still provably bounds the reported pointer to `appl_ptr`, now expected
   to engage rarely instead of on every restart.

This should, in principle, fix both bugs at once while touching nothing
on the steady-state path other machines rely on: the *only* behavioral
difference from current `main`/`46a7b12` is (a) re-anchoring every period
instead of once, and (b) occasionally distrusting `dev_timestamp` for a
single update when its gap to `os_timestamp` is implausible — both
changes are inert (never trigger, byte-for-byte same resulting values) on
a machine whose device clock never drifts and whose stream never runs
long enough to matter, which is the expected case for hardware `46a7b12`
already fixed.

Verification pending: basic playback sanity check, then the full
suspend/resume repro (5-6+ minutes) watching for both failure modes —
XRUN/pointer-capped/PipeWire-ERR counts for the storm, and
silence-with-healthy-`hw_ptr` for the dropout.

**Fifth fix attempt — result: real but partial improvement, storm not
eliminated.** Basic playback fine. Suspend/resume repro: user confirmed
"Hänger / Stotterer sind wieder da" — audible storm returned, clearing
~97s after resume-complete (close in magnitude to the original
65-102s range). Severity dropped a lot in absolute counts (24 XRUN /
23 "pointer capped" / 20 "dev clock distrusted" vs. the pre-v4 storm's
795 XRUN / 148 capped), and the isolated slow-drift episode captured
early in this test (boottime_ms 196005-199162, gap -32ms → -51ms, device
clock genuinely running slow) was fully absorbed with **zero** storm
activity — direct confirmation the distrust gate does what it was
designed to do for a clean, isolated drift event. But the count drop is
not the same as fixing the actual complaint (audibly back) — don't credit
this as a win the way v4 wrongly was before the dropout surfaced.

**Root cause of the miss, found by correlating cadence:** during the
storm's sustained phase (boottime_ms 268265-279953), every single
`first ts on` log shows `t2`/`host` within ~0.1-1ms of each other (e.g.
`t2=268323600034 host=268323851449`, 0.25ms) — the device clock was
genuinely fine, zero `dev clock distrusted` events fired in this whole
window, yet the storm continued regardless. Cross-checking cadence:
`TRIGGER STOP`/`START` cycled roughly every **~85ms** during the storm,
while bridgeOS timestamp messages arrive roughly every **~346ms**
(matching `buffer_time_length`). Restarts outpace messages about 4:1, so
during a storm most restarts complete entirely on the
`waiting_for_first_ts` → `start_io_time` fallback path and never receive
a fresh `remote_timestamp` at all — the per-message trust gate (or any
gate on which clock feeds `remote_timestamp`) structurally cannot reach
most of the storm's own restarts once it's underway. Every restart shows
the same signature immediately after `first ts on`: `pointer capped` with
`raw_delta` a few thousand frames against a ~2048-frame `backlog`, plus a
`ptr jump` back down near that backlog value — i.e. the accumulator's
safety net is firing on nearly every single restart, which is exactly the
"third captured repro" self-perpetuating-restart mechanism from before
the clock-anchor investigation even started: fresh restart → thin
~2048-frame pre-buffer → estimate overshoots it almost immediately → XRUN
→ PipeWire recovers via STOP+prepare+START → same thin margin → repeat.

**Lesson:** stacking a clock-source heuristic (gate) on top of the
existing accumulator was the wrong move — it protects a narrow case
(isolated slow drift) while leaving the storm's actual sustained-phase
mechanism (the fallback-path handoff overshoot) completely untouched.
Should have kept bisecting against the one known-clean reference point
(`6c4a367`) instead of layering a new heuristic on `main`'s anchor choice.

## Sixth fix attempt (2026-07-10) — isolate the clock variable against the known-clean baseline

`6c4a367` differs from `HEAD` in exactly two places in this code path:
(1) the steady-state anchor clock (`os_timestamp` vs. `dev_timestamp`),
and (2) the `waiting_for_first_ts` fallback body (6c4a367:
`return appl_ptr % buffer_size`, i.e. commit `4bd459f`'s version; current:
elapsed-time-since-`start_io_time`, from `46a7b12`). The fifth attempt
changed neither cleanly — it kept `dev_timestamp` (gated) and kept the
elapsed-time fallback. Since `6c4a367` is confirmed clean on this machine
for *both* bugs, some combination of reverting these two is guaranteed
clean; the fix should be found by bisecting toward it one variable at a
time, not by stacking more heuristics onto `HEAD`'s shape.

**Changed just the first variable**, in `t2audio_handle_stream_timestamp()`:
`stream->remote_timestamp = ktime_to_ns(os_timestamp);` unconditionally
— removed the fifth attempt's gate entirely, dropped
`T2AUDIO_DEV_CLOCK_MAX_GAP_NS`, kept the `delta_dev`/`delta_os_us`
diagnostics (still useful for monitoring, no longer drive any decision).
Left the `waiting_for_first_ts` → `start_io_time` fallback in
`t2audio_pcm_pointer()` untouched for now — testing one variable first.

**Why this should also fix the storm's sustained phase, not just the
isolated-drift episode:** the per-restart overshoot happens at the
handoff from the `start_io_time` fallback to the fresh `remote_timestamp`
anchor (`time_from_start` jumps from `ktime_get_boottime() -
start_io_time` to `ktime_get_boottime() - remote_timestamp` the instant
`waiting_for_first_ts` flips). With `os_timestamp`, both operands are
`ktime_get_boottime()`-domain, so this jump is always just "how long the
fallback ran for" — small and self-consistent by construction, matching
6c4a367's behavior. With `dev_timestamp`, the jump additionally carries
whatever cross-domain epoch/rate error exists between bridgeOS's clock
and the host's at that exact instant — which is exactly the kind of error
already shown to spike post-resume, and which a per-message gate can't
catch when most restarts don't get a fresh message before the next
restart. Anchoring on `os_timestamp` removes this failure mode at its
source rather than trying to detect it after the fact.

Not yet a full revert of `46a7b12`/`4bd459f`'s net effect: the fallback
body is unchanged, and all diagnostic logging/the accumulator safety net
stay in place. If this alone doesn't fully clear the storm, the fallback
body is the next thing to revert toward `6c4a367`, per the bisection
above — not another clock heuristic.

Verification pending: basic playback sanity check, then the full
suspend/resume repro (5-6+ minutes), same acceptance criterion as before
— clean audio *and* no silent-`hw_ptr`-healthy dropout.

**Sixth fix attempt — result: clean run, storm and dropout both absent.**
User report: brief crackle for a few seconds right after resume, then
completely clean for the rest of the test ("Nach Suspend/Resume gab es noch
ein kurzes Rauschen, aber das ging nur über wenige Sekunden... Das mag ein
Einzeleffekt gewesen sein"). Full log analysis (`alsa_status_poll_v6.log`,
`journal_resume_test_v6.log`, `pw_top_poll_v6.log`) confirms this
kernel-side:

- **0** `state: XRUN` in the ALSA status poll for the whole test.
- **0** "pointer capped" accumulator-safety-net events (vs. 24 in the fifth
  attempt, 148/795 in the pre-v4 storms).
- **2** total "ptr jump" lines in the entire log, both immediately after a
  `TRIGGER START` (one before suspend, one at the post-resume restart at
  `boottime_ms=106964`) — this is the expected, harmless pointer reset to
  near-zero on a fresh start, not a storm signature. Only **one** restart
  cycle happened after resume (`TRIGGER START` at 12:50:20, matching
  `t2bce_audio: resume deferred path complete boottime_ms=106218`); no
  further restarts until the user's own manual stop at 12:52:56, i.e.
  continuous, uninterrupted playback for ~2m36s post-resume.
- PipeWire's own `pw-top` `ERR` column stayed at **0** for the `Speaker`
  sink across every sample of the whole test — independent confirmation of
  no userspace-visible error either.

The single restart right at resume with a brief audible crackle is the
normal cost of `TRIGGER_START`'s own startup transient (matches `6c4a367`'s
baseline behavior too, which also briefly restarts the stream on first
post-resume playback) — not the multi-restart storm this investigation has
been chasing.

**Caveat — one clean run is not yet proof, especially for the dropout.**
The fourth attempt's silent dropout showed inconsistent timing across
occurrences (~13s vs. ~3.5min after resume), and the fifth attempt's storm
also looked absent in earlier basic checks before reappearing on the full
repro. A single ~2m36s post-resume window matches or exceeds the fastest
observed dropout onset (13s) but not the slowest (3.5min), and is far
short of the ~65-102s storm-clearing window observed in the original
storm-only investigation (that window doesn't directly apply here since
this test never entered a storm at all, but longer/repeated runs are still
warranted before treating this as confirmed). Debug logging has been
disabled on the Mac and the machine is idle pending the next test session.

**Next steps (not yet started):** run several more independent
suspend/resume repros, at longer post-resume durations (10+ minutes), to
build confidence against both failure modes before considering a commit.
The bisection plan (revert the `waiting_for_first_ts` fallback body toward
`6c4a367`'s `return appl_ptr % buffer_size` next, if needed) remains the
documented fallback if a future repro reopens the storm.

## Verification session v7 (2026-07-10, prepared, awaiting user test)

Long-duration verification runs for the sixth fix attempt. Preconditions
re-verified before handing off: Mac tree byte-identical to local working
tree (md5 over `pcm.c`/`audio.c`/`audio.h`), loaded == installed
`srcversion` (`63C4870BCAA0A605CC20E8E`), Mac on branch
`t2bce-audio-troubleshoot`, no reboot since the v6 test (so the loaded
module is still the sixth-attempt build), zero further suspend/resume
cycles between the v6 test and this session (checked `journalctl -k`), and
0 `state: XRUN` in the leftover still-running ALSA poll covering that whole
idle window.

Setup on the Mac (all survive suspend/resume and SSH disconnect via
`setsid nohup`):
- Dynamic debug re-enabled for `t2audio_handle_stream_timestamp`,
  `t2audio_pcm_pointer`, `t2audio_pcm_start`, `t2audio_pcm_trigger`.
- 25ms poll of `/proc/asound/card0/pcm0p/sub0/status` →
  `/tmp/alsa_status_poll_v7.log` (previous session's still-running poller
  killed; its log archived as `/tmp/alsa_status_poll_pre_v7_archive.log`).
- `journalctl -k -f` → `/tmp/journal_resume_test_v7.log`.
- `pw-top -b -n 1` every 0.5s → `/tmp/pw_top_poll_v7.log` (Speaker sink
  node visible, capture confirmed working over SSH).

Acceptance criteria (same as v6, but longer and repeated): across 2-3
independent suspend/resume cycles with 10+ minutes of continuous playback
each, (a) no XRUN/restart storm (0 `state: XRUN`, no `pointer capped`
bursts, `pw-top` ERR stays 0), and (b) no silent dropout (no
audible-silence window while `hw_ptr` keeps advancing healthily — the
longest observed dropout onset was ~3.5min into playback, hence the 10+
minute requirement).

## v7 test result (2026-07-10): storm gone, new residual symptom — and its root cause

User test: two suspend/resume cycles (14:20:11→:32 and 14:21:18→:32).
Report: crackling from **14:21:43 to 14:21:52** (9 seconds), "fängt sich
schneller und besser, aber noch nicht 100%ig weg".

**Storm and dropout signatures fully absent, kernel-side:** after the
second resume there was exactly **one** `TRIGGER START` (14:21:32) and no
further restart until the user's own stop at 14:22:56; **0** `state: XRUN`
in the whole 25ms ALSA status poll; **0** `pointer capped`; only the two
benign at-start `ptr jump` lines. The sixth fix attempt's target failure
modes did not occur — the residual 9s crackle is a **different, third
failure mode** that the storm previously masked.

**Root cause, measured (this is the key finding):** during exactly the
crackle window, the bridgeOS timestamp-message cadence changed from its
steady ~346.7ms to a steady **~375.5ms — in both clock domains at once**
(`t2_delta` ≈ `host_delta` ≈ 375.5ms per message). Since bridgeOS sends
one timestamp message per ring cycle (16640 frames), this means the
device was *genuinely consuming audio ~8.3% slower than nominal*
(16640 frames / 375.5ms = **44313 Hz effective instead of 48000**) for
those ~9 seconds, then snapped back. Additionally the device timestamp
showed a one-shot **+96ms** artifact at the window start and **-97ms** at
the end (t2_delta spikes to 471ms/249ms while host_delta stayed smooth) —
consistent with bridgeOS temporarily switching its output path to
something with ~97ms more internal latency and a slightly different
consumption rate, then switching back. What bridgeOS does internally is
unknowable from here; what matters is that **the real consumption rate
can deviate several percent from nominal for seconds around a resume.**

**How the current pointer code behaves under that (verified by exact
simulation, matching the captured data instant for instant):** the
pre-existing `frame_min` ratchet in `t2audio_pcm_pointer()` (the
`elapsed < 2*buffer_time_length → frame_min = frames` branch) makes late
messages *seamless* — when a message is late, the estimate keeps sweeping
at nominal 48k past the mod-BTL wrap, and when the late message finally
re-anchors, the floor holds the pointer flat instead of letting it move
backward. Net effect: ALSA's hw_ptr advanced at a smooth ~44.3kHz average
(measured: 42.0-45.2k per 0.5s bucket) with no jumps, no XRUN, no capping
— which is why every existing diagnostic stayed silent. **But within each
375.5ms cycle the estimate sweeps at 48k while the device only consumes
at 44.3k, so the reported pointer transiently runs up to ~1380 frames
ahead of where the sweep should be — and, after the steady-state
reporting lag (~1060 frames) is used up, up to ~300 frames ahead of the
device's true read position.** The erase head
(`t2audio_pcm_erase_played_frames()`) then zeroes those up-to-~300
not-yet-played frames, once per cycle — ~6ms of forced silence every
~375ms, for the whole deviation window = **exactly the rhythmic crackling
the user hears.** (Secondary suspect, same root: PipeWire's rate DLL sees
the sawtooth 48k-sweep-then-flat-hold pattern instead of a smooth 44.3k
and chases it with its resampler.)

## Seventh fix attempt (2026-07-10) — interpolate over the measured message cadence + erase-head safety margin

Two changes (`pcm.c` + fields in `audio.h`), both validated in simulation
against the captured 44313Hz window before implementing:

1. **Adaptive interpolation cycle:** new per-stream `msg_interval_ns` — a
   clamped EWMA (alpha 1/4) of the host-domain (`os_timestamp`) interval
   between consecutive bridgeOS timestamp messages, fed in
   `t2audio_handle_stream_timestamp()` (intervals outside
   (nominal/2, nominal*2) are dropped as missed-message/restart artifacts,
   not averaged in). `t2audio_pcm_pointer()` now sweeps over this measured
   cycle (clamped to [3/4, 3/2]×nominal at use) instead of the nominal
   `buffer_time_length`; the `frame_min` ratchet conditions use the same
   measured cycle. Host-domain intervals (not `dev_timestamp` deltas) are
   used deliberately: the anchor is host-domain, so the sweep must be
   measured in the same domain — and the host-domain cadence signal was
   smooth through the window edges while the device timestamps carried
   the ±97ms artifacts. The estimate persists across restarts and
   suspend/resume on purpose (a restart mid-window inherits the measured
   rate; staleness errs toward lagging = safe). Simulation: rate tracking
   improves from sawtooth 43.7-45.2k to smooth 44.0-44.6k, and the
   worst-case overshoot vs. the device's true position drops from ~+300
   frames *every cycle* to ~+180 frames *once* (EWMA convergence
   transient at window start, time constant ~4 messages ≈ 1.5s).
2. **Erase-head margin:** `t2audio_pcm_erase_played_frames()` now trails
   the reported pointer by `T2AUDIO_ERASE_MARGIN_FRAMES` (1024, ~21ms)
   before zeroing, with a skip-instead-of-wrap rule when the lagged
   target is still behind the erase head (span > half ring). This
   removes the remaining transient corruption path entirely: even while
   the EWMA is still converging, the margin (1024) comfortably covers
   the residual overshoot (~180-300). Delaying erasure by ~21ms is
   immaterial for the erase head's purpose (stale-sample hygiene).

Both changes are inert on machines whose cadence never deviates: the EWMA
then sits at nominal ±message-arrival-jitter (sub-percent) and the sweep
is the same as before within noise; the erase margin only shifts *when*
consumed samples get zeroed, never whether.

Also renamed `dbg_last_os_timestamp` → `last_os_timestamp` (now
load-bearing for the cadence estimate, no longer diagnostics-only) and
extended the `ts update` debug line with `cyc_est=%lldus` so the next
capture can watch the estimate converge during a deviation window.

Built via targeted DKMS rebuild (installed srcversion
`B2FEF8FE2505A503630A905`; t2bce_audio not in initramfs, so no initramfs
rebuild needed). Reboot required to load (hot-reload blocked by
`t2bce_core` PM-client reference).

Verification pending: same acceptance criteria as v7 (storm + dropout),
plus specifically: during any future device-rate-deviation window, expect
`cyc_est` in the `ts update` lines to converge to ~375000us within ~1.5s
and audio to stay clean (or at most one brief artifact at window start),
with 0 XRUN / 0 capped throughout.

Post-reboot remote sanity check (user still at GDM, so via `sudo aplay`
directly on `plughw:Audio,0`, bypassing PipeWire): driver path fully
healthy — `cyc_est` converges to ~346.3ms as designed, 0 XRUN, and the
`pointer capped` events that fire right before each `TRIGGER STOP` are
the accumulator correctly holding at the written edge during aplay's
end-of-file drain (appl_ptr stops advancing → backlog shrinks → cap; this
is the designed behavior at drain, not a defect; PipeWire keeps writing
silence so it won't fire there).

Side finding, deliberately not chased: `aplay` playing **two or more
files back-to-back** on this card aborts at exit with glibc
`corrupted size vs. prev_size` (reproducible; single file is clean; gdb
shows the abort in a `free()` in aplay's `main()`, i.e. late detection of
an earlier userspace heap overrun). This cannot be caused by the kernel
driver: aplay uses WRITEI (no ring mmap into its address space), and the
hw params that drive aplay's own buffer sizing are untouched by this
branch. Most likely a pre-existing aplay/alsa-lib quirk with this card's
unusual fixed-period hw params, never noticed because nothing plays
multi-file aplay sessions on it. PipeWire showed 0 errors across all
captures. Revisit only if a userspace-visible corruption ever shows up in
PipeWire itself.

Fresh v8 captures started after the reboot (same three loggers, `*_v8.log`
in `/tmp`; note `/tmp` is tmpfs on this Mac — **v7 raw logs were lost to
the reboot**, analysis results preserved here and in the scratchpad; from
now on, copy any logs worth keeping out of `/tmp` before rebooting).
Dynamic debug re-enabled (it also resets on reboot).

## v8 test result (2026-07-10): FIX VERIFIED — issue closed

User ran multiple suspend/resume cycles with extended playback: **audio
is fully stable, the post-resume crackling is gone.** The v8 captures
(preserved in the session scratchpad; `/tmp` on the Mac is tmpfs) show
the mechanism doing exactly what it was designed to do:

- Real cadence-deviation windows *did* occur during the tests, in both
  directions (`cyc_est` tracked 312ms…373ms vs. the nominal ~346.7ms),
  and were inaudible — previously an 8% slow window meant ~9s of
  rhythmic crackling.
- 0 XRUNs across all captures.
- All `pointer capped` events sat at stream-drain edges (accumulator
  holding at the written edge — designed behavior) and all `ptr jump`
  lines were the known `X -> 0` restart artifacts of the diagnostic
  itself (dbg_last_ptr not reset across restarts), not real glitches.

## Final cleanup (2026-07-10)

The working tree was reset to HEAD and the surviving mechanisms were
re-applied as four self-contained commits (each compile-tested on the
Mac against the real kernel/symvers):

1. `t2bce_audio: anchor pointer interpolation in the host clock domain`
   — sixth fix attempt (kills the post-resume restart storm).
2. `t2bce_audio: never report more consumed frames than the app wrote`
   — third fix attempt's monotonic accumulator (safety net; also what
   makes drains behave).
3. `t2bce_audio: interpolate the pointer over the measured message
   cadence` — seventh fix attempt part A (kills the crackle root cause).
4. `t2bce_audio: keep the erase head a safety margin behind the pointer`
   — seventh fix attempt part B (covers the EWMA convergence transient).

Dropped in the cleanup (diagnostics only, all verified byte-identical
logic otherwise via comment-stripped diff against the tested build):
`t2audio_dbg_boottime_ms()` helper and all `boottime_ms=` log suffixes
(incl. the audio.c resume pr_info decorations), the `ptr jump` detector
(`dbg_last_ptr`/`dbg_last_ptr_valid`), `dbg_last_dev_timestamp` and the
`ts update` t2_delta/host_delta line (replaced by a slim
`ts interval ... cyc_est_us=` pr_debug feeding off load-bearing state
only), and the `fallback_us` suffix on `first ts on`. Code comments were
rewritten to be self-contained (no references to fix-attempt numbers or
this untracked file). Kept pr_debug observability: `ts interval` (cadence
estimate) and `pointer capped` (accumulator engagement).

Cleaned build srcversion: `D137B7AD540028B84AF346C`. Note: the Mac keeps
*running* the tested superset build `B2FEF8FE2505A503630A905` until its
next reboot (hot-reload of t2bce_audio is blocked by the t2bce_core
PM-client reference; the cleaned build is behaviorally identical, so no
retest is required — the next regular reboot picks it up). Dynamic debug
was switched off and the v8 logger processes were stopped.
