# PROCHOT / fan control learnings

Context: the fan daemon (`apps/t2-fan-control/`) regulates only on the CPU
package temperature. The plan was to add a "chassis guard" that also considers
the ~36 t2smc sensors, on the hypothesis that PROCHOT throttling is caused by
the chassis as a whole overheating. This documents the calibration run that was
supposed to produce the guard's thresholds.

## Setup

- MacBookPro16,2 (Mac-5F9802EFE386AA28), Ice Lake, TjMax 100, TCC offset 0.
- Load: `stress-ng --cpu 8` for 5 min, then `stress-ng --cpu 8` +
  `glmark2 --size 1600x1000 --run-forever` (iGPU, Iris Plus G7).
- Fan curve during the run: the installed `Custom` preset
  `20:0,63:24,74:76,84:100` — i.e. the fans were free to reach 100 %.
- Logger: `prochot-calibration-2026-08-10/thermal-log.sh` (1 Hz: all t2smc
  channels, coretemp, fans, MSR 0x1B1 / 0x64F / 0x6B0 / 0x611, throttle counts).
  Raw data: `run1.csv` (332 samples), `power.csv` (53 samples of SMC adapter and
  battery telemetry). Analysis: `analyze.py`.

## Result: the throttle is NOT thermal

30 PROCHOT episodes were captured. The chassis-guard hypothesis is falsified on
this machine:

| Observation | Value |
|---|---|
| Throttle reasons seen | `PKG_PROCHOT` + `CORE_PROCHOT` + `GFX_PROCHOT`, 30/30 |
| RAPL / VR_THERM / EDP / thermal status | **0/30** — never once |
| Die temperature at assertion | dTj 21-30, i.e. **70-79 °C** (TjMax 100) |
| Samples at pkg >= 95 °C | 65, of which throttled: **0** |
| Fan speed at assertion | 6182-6373 / 6712-6935 RPM = **hardware maximum, 30/30** |
| Clock at assertion | **199-200 MHz** (from 3300) |
| Ambient TA0V at assertion | 27.0-27.75 °C, *below* the 28.50 °C idle baseline |

Two facts kill the idea outright:

1. **The fans were already at 100 % during every single assertion.** A guard can
   only make the fans spin up *earlier*, never harder. There was no headroom
   left to give.
2. **65 samples ran at >= 95 °C package with no throttle at all**, while the
   throttle fired repeatedly at 70-79 °C. Die temperature and the throttle are
   uncorrelated, and so is every chassis channel.

The per-channel plateau/follower ranking (`analyze.py`) found no causal
candidate: every channel that rose was still climbing linearly at the moment of
assertion (follower), and every channel with a low spread across the 30
episodes (`TF0S`, `TB0T`, `TB2T`, `TA0V`, …) had simply not moved at all.
Low spread there means "inert", not "causal" — worth remembering when reading
that table.

## Actual cause: adapter power ceiling

`power.csv` correlates PROCHOT with the SMC power telemetry
(`smc_adapter_power_uw`, `smc_battery_current_ua`). The pattern repeats ~12
times without exception:

```
adapter 52.4 W  pkg 37.4 W  prochot=0
adapter 52.1 W  pkg 36.3 W  prochot=0
adapter 17.5 W  pkg  7.8 W  prochot=1   <- assertion, clock drops to 200 MHz
adapter 27.5 W  pkg 21.5 W  prochot=0   <- recovers, ramps back to 52 W, repeats
```

- Adapter output tops out at **52.1-52.5 W** immediately before every assertion —
  the same ceiling every time, +/- 0.4 W.
- `smc_battery_current_ua` is **exactly 0 in all 53 samples**. The battery
  contributes nothing: `battery_charge_limit` = 80 and capacity = 80, so it is
  neither charging nor discharging.
- RAPL is not the limiter either: MSR 0x610 reads **PL1 = 100 W, PL2 = 125 W** on
  a 28 W part. Apple deliberately leaves the power limits wide open and does the
  real limiting through the SMC and BD_PROCHOT (MSR 0x1FC bit 0 = 1, enabled).

So the system draws until it hits the adapter ceiling, the SMC asserts
BD_PROCHOT, the clock collapses to 200 MHz, draw falls, PROCHOT releases, and it
ramps straight back up. The "laptop becomes unusable" symptom is this
oscillation, not a slow thermal soak.

## What is proven and what is not

Proven by this run:

- The throttle is **not** caused by any temperature. 65 samples at >= 95 °C with
  zero assertions, 30 assertions at 70-79 °C, and no thermal/RAPL/VR/EDP status
  bit ever set. This holds regardless of what the mechanism turns out to be.
- The fans were at hardware maximum during all 30 assertions.
- `smc_battery_current_ua` read exactly 0 throughout, while the adapter sat at a
  repeatable 52.1-52.5 W ceiling before each assertion.
- The battery current channel **works**: unplugged it reads -736 mA / 8.8 W
  discharge. The 0 A under load on AC was a real measurement, not a dead sensor.

**Not** proven — do not state these as fact:

- That the 80 % `battery_charge_limit` is *why* the battery contributed nothing.
  Competing explanations that fit the same data equally well:
  1. SMC policy refuses to cycle a battery reported as `Full` for load peaks,
     whatever the limit value is set to.
  2. The 52 W is an SMC-enforced *input current limit*, not an adapter shortfall.
     Then BD_PROCHOT is the enforcement mechanism and the battery is simply never
     asked to help.
  3. Supplementation happens in bursts shorter than the 1 Hz sampling interval.
- That a higher-wattage adapter would move the ceiling.

Discriminating tests (designed, not yet run):

- **Unplugged under the same load.** No adapter ceiling and the battery must
  deliver. If PROCHOT vanishes -> the ceiling is power-delivery related. If it
  persists -> the mechanism is something else entirely. Cheapest and most
  decisive; needs only `prochot-monitor.sh` plus the power logger.
- **`battery_charge_limit` = 100, charged to 100 %, then the same load.** Battery
  full but limit not clamping. If `battery_current` goes negative under load
  here but not at 80/80, explanation (1) is out and the limit really is the
  cause.
- **Different adapter.** If the ceiling moves off 52 W it is the adapter; if it
  stays, it is SMC policy.

## The stress test reproduced the wrong failure mode

Reported real-world trigger (user, not yet instrumented): long MS Teams calls,
**especially with screen sharing**, machine normally **docked**. CPU load only
15-50 %, iGPU doing video decode plus compositing. Onset after **15-20 minutes**.

That profile does not fit the mechanism measured above:

- At 15-50 % CPU with video decode, total draw is far below the 52 W ceiling, so
  an adapter-power limit is an unlikely explanation.
- A 15-20 minute onset is a **thermal soak time constant**, not a power event.
  The stress test asserted within ~4 minutes because it slammed straight into
  the ceiling.
- Docked adds an entirely different power and heat path: Thunderbolt controllers
  under sustained load (`TTRD`/`TTTD`/`TTWD`/`TTXD`), external display, and PD
  passthrough. None of that was present in this run.

So `stress-ng` + `glmark2` found *a* real PROCHOT mechanism quickly, but very
likely **not the one that actually bothers the user**. The chassis-guard idea is
therefore **not dead** — the slow-onset docked case is exactly the signature it
was designed for. It is only unsupported *for the high-draw stress case*.

## Revised test protocol (for the Teams/docked case)

1. Docked exactly as in normal use, external display attached, on AC.
2. Log from the start: all t2smc temps **plus** `smc_adapter_power_uw`,
   `smc_battery_current_ua`, `smc_battery_power_uw` — the thermal channels alone
   would have produced a wrong conclusion last time.
3. Load must mimic the real thing, not `stress-ng`: a real or synthetic video
   call with screen share, i.e. sustained iGPU decode + encode + compositing at
   moderate CPU. `intel_gpu_top` to confirm the engines are actually busy.
4. Run **at least 30 minutes** — the failure needs 15-20 min to appear.
5. Discriminator at the moment of assertion:
   - adapter power near 52 W -> same power mechanism as this run;
   - adapter power well below it, with several chassis channels still climbing
     and only now plateauing -> genuine thermal soak, and the plateauing channel
     is the chassis-guard candidate that this run failed to find.
6. Note whether the fans are at maximum. If they are not, a guard has headroom
   to work with; if they are, it cannot help in that case either.

## Consequences

- **Do not implement the chassis guard on the strength of this run** — but do not
  discard it either. It needs the docked/Teams calibration above before the
  decision is made. The implementation plan itself is unaffected; only the
  threshold table depends on the measurement.
- If the power mechanism is confirmed as a second, independent cause, the useful
  mitigation is capping sustained CPU power (PL1 via MSR, or
  `energy_performance_preference` / max frequency) so draw stays just under the
  ceiling. That trades a modest steady clock for eliminating the 3300 -> 200 MHz
  oscillation, which is almost certainly the better user experience. This is
  outside the fan daemon's scope.
- Assume **multiple independent PROCHOT causes** on these machines. Any future
  fix should be attributed to a specific measured mechanism, not to "PROCHOT" in
  general.
- Useful side effect: `scripts/fedora/prochot-monitor.sh`'s "BD_PROCHOT
  (inferred)" heuristic was confirmed correct by direct MSR logging.

## Method notes

- Sample the SMC power telemetry (`smc_adapter_power_uw`,
  `smc_battery_current_ua`) from the start next time. The thermal logger alone
  would have led to a wrong conclusion; the power channels are what identified
  the cause.
- MSR 0x1B1 (IA32_PACKAGE_THERM_STATUS) is the cheapest package-level PROCHOT
  source: bit 2 = PROCHOT event, bits 16-22 = dTj. MSR 0x6B0 gives the same for
  the iGPU.
- CPU-only load already reproduces the throttle on this machine; the iGPU is not
  required, it only makes the ceiling arrive sooner.

## Platform / heat-soak bias in t2-fancontrol (2026-08-19)

Implemented a configurable heat-soak bias in `apps/t2-fan-control` so the curve
reacts to platform soak, not just to the CPU/GPU die temperature (the observed
PROCHOT happened at only ~65-70 C CPU).

- `base = max(cpu, gpu)` is unchanged, including its 10-sample smoothing. The
  bias is added **after** smoothing, so existing behaviour is bit-identical when
  no platform sensor is present or the bias is disabled.
- `skin = max(Ts0S, Ts1S)`, `battery = max(TB0T, TB1T, TB2T)`;
  `skin_bias = clamp((skin - 40.5) * 6.0, 0, 25)`,
  `battery_bias = clamp((battery - 38.0) * 2.0, 0, 6)`.
- The two biases are combined with `max()`, not summed — both are proxies for
  the *same* heat soak, so summing would double-count it.
- `effective = clamp(base + bias, 0, 100)` and that value, not `base`, is what
  feeds the fan curve.
- All thresholds, factors, caps and the sensor label lists are config keys
  (`skin_threshold_c`, `skin_factor`, `skin_max_bias_c`, `battery_*`,
  `max_effective_temp_c`, `skin_sensors`, `battery_sensors`, `log_sensors`,
  `platform_bias_enabled`) in `/etc/t2-fancontrol/config.txt`.

Sensor plumbing:

- Platform sensors are read in **millidegrees** (`read_temperature_mc`). The old
  `read_temperature()` truncated to whole degrees via `raw / 1000`, which is
  useless here: the entire PROCHOT signal lives between Ts1S 44.2 and 45.2 C.
- `TemperatureSnapshot.sensors` carries the raw label/value pairs; missing
  labels simply never appear, and `max_sensor()` returns `None` for a group
  whose labels are all absent, which yields bias 0.0. That is the "SMC sensor
  missing" path — no special casing needed anywhere else.
- `TaLC, TaRC, TH0X, TTRD, TTTD, TTWD, TTXD` are sampled and logged every 30 s
  (journal, `sensors: ...` line) but deliberately excluded from the bias, for
  later correlation work.
- This machine (MacBookPro16,x, t2smc hwmon) exposes no `TG0P`/`TGDD`, so the
  GPU source is absent and `base` is CPU-only. Not a regression, it was already
  the case.

First live calibration (2026-08-19): the spec'd 40.5 / 6.0 / 25 saturated the
skin bias in normal desktop use. Ts0S/Ts1S idle around 43-46 C on this machine,
so the bias sat at its +25 cap and pinned both fans at max (6368/6865 rpm) at
only 61 C CPU. Retuned via config to **41.0 / 4.0 / 20**, which drops the same
state to 2770/3018 rpm and gives +12.8 C at the 44.2 C skin of the reference
measurement; the cap is only reached from 46.0 C upward. The code defaults are
still the original 40.5 / 6.0 / 25.

Note when verifying a config change: the `sensors:` journal line is only emitted
every 30 s, so a reading taken a few seconds after `SIGHUP` still shows the old
bias. That is a stale log line, not a failed reload -- wait for a fresh line
before concluding anything.

## Field result and comparison with upstream v0.05 (2026-08-20)

Field result: two long video calls, no PROCHOT event. The bias could even be
relaxed afterwards; the live config now runs skin threshold **42.0** / factor 4
/ max 20 with curve `20:0,63:16,79:52,87:100`. So the approach works on this
machine, with the caveat that it is a single machine and a single day.

Upstream (`3935b19` + `80cc852`, fan-control v0.05) solves the same problem a
different way and is currently NOT merged into `local/rig`. Its model:

- The fan curve follows `max(CPU, GPU)` only, with the 10-sample moving average
  **removed** -- the curve now sees the instantaneous 2 s sample.
- A second, independent quantity `system_temp_c` is the arithmetic mean of
  *every* readable positive temperature (CPU + GPU + all SMC labels).
- One threshold `system_temp_limit_c` (default 45) latches **both fans to 100%**
  when the mean reaches it, releasing at limit - 5 C.
- The same number is also the right end of the fan curve's x-axis.
- `sensor_label()` adds a ~45-entry SMC-key-to-human-name table (Palmrest skin,
  PCH Die, Thunderbolt Left/Right, ...), used for the UI and for GPU role
  detection.
- Any config without `config_version=2` is discarded and replaced by defaults.

Measured on this machine (idle, CPU package 60 C, read-only sampling of
hwmon2/t2smc + coretemp):

- 36 sensors enter the mean; the mean is already **42.4 C** at idle against a
  default wall of 45 C.
- **12 of those 36 sensors are CPU sensors** (`TC0E TC0F TC0P TC1C TC2C TC3C
  TC4C TCBC TCGC TCSA TCXC` + CPU package). Raising only the CPU sensors by
  25 K lifts the mean from 42.4 to **50.7 C**, i.e. well past the wall.

Consequences worth knowing before arguing either way:

1. The README claim that a high CPU temperature "cannot independently engage the
   system-limit latch" does not hold here. A third of the averaged sensors are
   CPU sensors, so the latch is a CPU trigger with a ~3x dilution factor -- and
   the dilution factor is itself model-dependent (how many `TC*C` cores a model
   exposes), which is exactly the portability problem it was meant to avoid.
2. The absolute value of a mean over an arbitrary sensor inventory is not
   comparable between models. Two Macs at identical thermal state produce
   different means purely from which sensors exist, so `system_temp_limit_c`
   still has to be calibrated per model -- only now the number no longer
   corresponds to any physical temperature.
3. Because the curve's last point is pinned to the same value, the default
   config saturates the curve at 18% above 45 C CPU. Everything above that
   relies on the mean latch (and on the CPU's own throttling).
4. Response is bang-bang: 18% -> 100% -> 18%, with a 5 C hysteresis on a value
   quantised to whole degrees. The removed smoothing makes the curve part react
   to instantaneous Intel package spikes as well.
5. Still `raw / 1000` truncation everywhere, so sub-degree drift is invisible.
6. Legacy configs are reset, not migrated.
7. Minor: `read_from()` computes `hottest_temp_c` / `overall_hottest_temp_c`
   inline and then `record_temperature()` computes both again; the `temp == 0`
   early return there is unreachable because the caller already filters.

What v0.05 gets right and we should not dismiss: heat soak really is a separate
quantity from component temperature, and it deserves its own path (we agree on
that); the SMC label table is genuinely useful; and a hard safety wall is a
reasonable last line of defence.

### Idea for the portability problem (both approaches need this)

The disagreement is not "named sensors vs. average" -- both need per-model
knowledge. The difference is whether that knowledge is explicit and testable.
Two mechanisms would make the named-sensor approach portable:

- **Roles with fallback chains instead of fixed labels.** Upstream's
  `sensor_label()` table already classifies keys; keep the classification
  instead of discarding it. Define roles (skin, battery, board, storage,
  ambient, cpu, gpu) with an ordered candidate list per role, take the hottest
  present member, and degrade gracefully when a role is empty -- which is what
  `max_sensor()` returning `None` already does today.
- **Bias on the rise above a learned baseline, not on an absolute threshold.**
  Track a long-window low percentile of the skin role while the CPU is cool and
  bias on `skin - baseline`. A delta threshold of ~4-6 K transfers between a
  machine whose skin idles at 34 C and one that idles at 40 C; an absolute
  41/42 C threshold does not. This also fixes the same weakness in the mean
  approach -- the *rise* of the mean is meaningful across models even though its
  absolute value is not.

Empirical route out of the guesswork: the `sensors:` journal line already logs
the candidate keys. Collecting it from a handful of different models around real
PROCHOT events would let the role tables be derived from data instead of being
argued about.

## System temperature as a third, portable bias source (2026-08-21)

Taking up the one good idea from upstream v0.05 without its hard 100% wall: the
daemon now computes a *system temperature* and feeds it into the same bias path
as skin and battery.

- System temperature is the arithmetic mean of every sensor that is **not** a CPU
  or GPU sensor. That is the part v0.05 got wrong: it averages the CPU sensors in
  too, and on this machine 12 of 36 sensors are CPU sensors, so its "system"
  value is really a diluted CPU trigger.
- Sensor discovery is no longer a hardcoded label list. Every labelled sensor the
  SMC exposes is sampled and classified by role: `TC*` -> Cpu, `TG*` -> Gpu,
  `Ts*` -> Skin, `TB*` -> Battery, everything else -> System. Descriptive labels
  are matched by substring first (cpu/package/core, gpu, batt, skin/palm), so
  machines that expose friendly names instead of 4-character SMC keys still
  classify. An unknown label counts as a system sensor, which is the safe default.
- `skin_sensors` / `battery_sensors` accept `auto` (or an empty value), which is
  the new default: pick the hottest sensor of that role at runtime. An explicit
  list still overrides it. So nothing is hardcoded per machine, but a user who
  knows their platform can still pin it.
- Plausibility filter: readings outside 10..125 C are dropped from both the mean
  and the group maxima. `TF0S` reports 3.5 C here; averaging it in shifts the
  system mean by -1.4 K.
- The three contributions are still combined with `max()`, not summed.
- `max_effective_temp_c` is gone as a setting; the effective temperature is simply
  clamped to 100 C, the upper end of the curve's own axis. It never did anything
  else.
- Defaults: system 40.0 C / factor 4.0 / max 25 C, skin 42.0 C / 4.0 / 20 C,
  battery 38.0 C / 2.0 / 6 C. Measured baseline for the system default: with the
  machine near idle the mean over 24 non-CPU/GPU sensors was 36.4 C, so 40 C
  leaves real headroom before the bias starts.
- Unknown config keys are still ignored rather than rejected, so an existing
  config file keeps working and nobody's fan curve gets invalidated. That is
  deliberately the opposite of what v0.05 does with `config_version=2`.

Why keep skin and battery next to the system mean: the mean is the portable
source that exists on every machine, but it is slow and diluted. On this machine
the skin sensors are the ones that actually led the PROCHOT events, so they stay
as a faster, sharper input. `max()` means whichever notices first wins.

## Yo-yo from the system temperature, and the low-pass that fixes it (2026-08-21)

First field impression of the system-temperature bias: it works, but it introduces
a slow oscillation. The cause is structural, not a tuning mistake. Unlike CPU
temperature, the system average is a quantity the fan itself directly moves:

    fan up -> chassis cools -> bias drops -> fan down -> chassis warms -> bias returns

That is a closed proportional loop, and it hunts.

Fix: a first-order low-pass on the system average with **separate rise and fall
time constants**, `system_rise_time_s` (default 30 s) and `system_fall_time_s`
(default 180 s). Fast attack, slow release. The asymmetry is what actually breaks
the oscillation: the fan still reacts promptly when the chassis heats up, but it
no longer chases the cooling it just caused itself. It also errs on the safe side,
since the bias lingers rather than disappearing early.

A symmetric filter would have been the obvious choice and is the wrong one here.
Adding equal lag in both directions mostly adds phase lag to the loop, which can
make hunting worse rather than better. Only the asymmetry removes energy from the
cycle.

Notes:
- A time constant of 0 disables the filter for that direction, so the old
  behaviour is still reachable from the config.
- Losing the sensors for a tick must keep the last average rather than reset it to
  None. Resetting would drop the bias to 0 instantly, which is exactly the jump
  the filter exists to prevent. A unit test caught this in the first draft.
- The journal line now logs both values: `system=<smoothed> (raw=<unfiltered>, n=)`,
  so the filter's effect is visible in the field.
- The skin and battery groups are deliberately *not* filtered. They are the fast
  path; the system average is the slow one.

If the yo-yo survives this, the next lever is loop gain, not more filtering:
lower `system_factor` from 4.0. Filtering changes when the loop responds, gain
changes how hard it responds, and hunting is usually a gain problem.

## Fewer wakeups and a gliding fan note (2026-08-21)

Two complaints at once: the fan speed changed audibly every couple of seconds, and
the daemon was supposed to be as cheap as possible so the package can stay in a deep
C-state. Measured on the running daemon via `/proc/<pid>/status`
(`voluntary_ctxt_switches` delta over 10 s), the v0.04 fork woke up **86 times per
second**. Three separate causes, all fixed:

1. **The daemon polled.** The main loop slept 100 ms, drained a non-blocking
   `accept()` and slept again. Replaced by a `poll(2)` on the listener fd with a
   timeout of the time left until the next control tick, so the daemon now blocks
   until a client connects or the tick is due. This alone is the difference between
   10 forced wakeups a second and none. `libc` became a direct dependency for it; it
   was already in the lock file through `signal-hook`.
2. **Every SMC key costs a mailbox round trip.** Reading a single `tempN_input` on
   `t2smc` shows up as ~4 voluntary context switches, and the controller was reading
   all 36 of them plus the fan registers every 2 s. The control interval is now 5 s,
   and the full sensor set is sampled only every 30 s (`PLATFORM_SAMPLE_INTERVAL`) --
   the heat-soak signals are low-passed over minutes, so sampling them faster buys
   nothing. In between, only CPU and GPU are read (`TemperatureSnapshot::read_cpu_gpu`)
   and the cached platform sensors are carried over.
   Because the sampling step is no longer the control interval, `smooth_system_temp()`
   takes the elapsed time as an argument instead of assuming `CONTROL_INTERVAL`.
3. **The GUI polled the daemon 5.5x faster than the daemon recomputed anything.**
   Its refresh went from 900 ms to 2 s, and it skips the poll entirely while the
   window is not visible.

Result under identical conditions (GUI window open): **86/s -> ~10/s**, and with no
GUI running the daemon is down to a handful of wakeups per second, nearly all of them
the 30 s sensor sweep.

Do not move the fan refresh (`refresh_state`) out of the tick and into the `GET_STATE`
handler. It looks like a saving -- the registers are only display data -- but the GUI
polls faster than the control loop, so it made the daemon *worse* (23/s measured).
Bounded work on a slow timer beats on-demand work on a fast one.

### The acoustic half: a slew limit, not more filtering

The temperature side was already filtered (10-sample average, now spanning 50 s at
5 s ticks, plus the asymmetric system low-pass). What was still audible was the
*output* jumping to whatever the curve said. `next_percent()` now moves the commanded
speed toward the curve target by at most `fan_rise_rate_pct_per_s` / 
`fan_fall_rate_pct_per_s` per second (defaults 1.0 and 0.35, both config keys, so
tuning needs a SIGHUP and no rebuild). Rising faster than falling is the same
asymmetry as the temperature low-pass and for the same reason: err toward more
cooling. On this machine 1 percentage point is roughly 40 rpm, so a fall step is
~60 rpm per 5 s tick -- below the threshold where the ear notices a change at all.

Two guards keep the limit from becoming a hazard:
- the 3 pp deadband still applies first, so the fan holds still instead of creeping,
- above `EMERGENCY_TEMP_C` (92 C effective) the limit is bypassed and the target
  applied in one step.

The journal line now ends in `target=<curve> fan=<commanded>`, which makes the ramp
directly visible: `target=96 fan=96` -> `target=56 fan=82` -> `target=42 fan=68` ->
`target=52 fan=52` is the curve dropping 54 points while the fan glides down over
~90 s.

## The sluggishness was the temperature filter, not the slew limit (2026-08-21)

After the rate limit went in, `stress-ng --cpu 4` still ran the machine into thermal
throttling. Sampling `fan1_target` at 5 Hz through a 25 s stress burst showed the fan
climbing 2572 -> 3945 rpm over 30 s and still creeping a minute later. The rate limit
was not what held it back: the *target* itself was crawling, because the CPU/GPU
temperature was smoothed with a flat 10-sample moving average -- 20 s at the old 2 s
tick, and 50 s after the tick moved to 5 s. `base=` in the journal went 63 -> 78 C
across the whole run while the silicon was long past 95 C.

A flat moving average is the wrong filter here. It is symmetric, so buying quiet on
the way down costs exactly as much lag on the way up, which is the one direction where
lag turns into throttling. Replaced by the same asymmetric first-order low-pass the
system temperature already uses (`smooth_temp`, `base_rise_time_s` 3 s /
`base_fall_time_s` 45 s). At a 5 s tick that is alpha 0.63 rising and 0.1 falling: a
load step is essentially answered within two ticks, and a burst that ends still takes
a minute or two to release the fan.

### Ramping at 10 Hz costs almost nothing

The remaining problem was that a fast target with a slow rate limit still means audible
steps, and a fast rate limit means an audible jump. Both disappear if the fan is moved
*often* instead of *far*: the daemon now runs a ramp step every 100 ms
(`RAMP_INTERVAL`) whenever the commanded speed is more than the 3 pp deadband away
from the target, and the tick that reads sensors stays at 5 s. The ramp step reads
nothing -- it only writes `fanN_target` -- which is exactly why this is affordable:
the expensive part of a tick is the SMC *reads*, not the writes.

Measured: ~21 wakeups/s while a ramp is running, back to ~10/s (GUI open) or ~6/s
(idle) the moment it settles. The old polling daemon sat at 86/s permanently.

Two more pieces fell out of this:
- The commanded speed has to be kept as `f32` (`commanded_percent`). With a `u8` a
  slow rate rounds every step away to nothing and the fan never moves at all -- the
  first attempt papered over this with a forced +-1 step, which then made 0.35 pp/s
  behave like 10 pp/s at 10 Hz.
- Each ramp step advances by at most `RAMP_INTERVAL` worth of movement, however long
  the daemon actually slept. Otherwise a target that jumps while the daemon is idle
  gets 5 s of accumulated movement applied in a single write, i.e. the jump again.

### The rate is gap-proportional, and only upward

`ramp_rate()` returns `max(base_rate, gap / fan_ramp_time_s)` when speeding up and the
plain base rate when slowing down. Small corrections crawl; a big gap is closed within
roughly `fan_ramp_time_s` (default 1.5 s). Measured on a real load step: 3843 -> 4403
rpm in 1.9 s, in ~50 rpm increments every 200 ms -- a continuous sweep, not a series of
steps. Coming down is deliberately never boosted; that asymmetry is what keeps the fan
from pumping, and it is the same reasoning as the temperature filter above.

This also let the 92 C emergency bypass go. It never fired -- `effective` is a
smoothed, biased number that rarely reaches 92 -- and a bypass that jumps straight to
the target is precisely the acoustic behaviour we are trying to remove. A gap-
proportional ramp reaches full speed in about two seconds anyway, without the jump.
