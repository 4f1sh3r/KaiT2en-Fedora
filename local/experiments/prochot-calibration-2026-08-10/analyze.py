#!/usr/bin/env python3
"""Rank SMC channels by how well they predict a PROCHOT assertion.

The decisive question from the plan: at the moment PROCHOT asserts, which
channels have *plateaued* (candidate cause) and which are still climbing
(follower)?
"""
import csv, sys, statistics as st

PATH = sys.argv[1] if len(sys.argv) > 1 else "run1.csv"

META = {"ts","phase","pkg_c","core_max_c","pkg_dtj","fan1","fan2","fan1_tgt",
        "fan2_tgt","pkg_w","mhz_avg","pkg_therm","pkg_prochot","pkg_pl",
        "core_prochot","core_thermal","core_rapl","core_vr","core_edp",
        "gfx_prochot","gfx_thermal","gfx_rapl","gfx_vr","pkg_thr_cnt"}

rows, hdr = [], None
with open(PATH) as fh:
    for line in fh:
        if line.startswith("#"):
            print(line.rstrip()); continue
        if hdr is None:
            hdr = line.rstrip("\n").split(","); continue
        parts = line.rstrip("\n").split(",")
        if len(parts) != len(hdr):
            continue
        rows.append(dict(zip(hdr, parts)))

sensors = [c for c in hdr if c not in META]
f = lambda r, k: float(r[k])
i = lambda r, k: int(float(r[k]))

print(f"\nsamples={len(rows)}  sensors={len(sensors)}")
for ph in ("idle", "cpu", "cpu_gpu", "cooldown"):
    sub = [r for r in rows if r["phase"] == ph]
    if not sub:
        continue
    print(f"  {ph:9s} n={len(sub):4d}  pkg {min(i(r,'pkg_c') for r in sub)}-"
          f"{max(i(r,'pkg_c') for r in sub)}C  "
          f"W {min(f(r,'pkg_w') for r in sub):.0f}-{max(f(r,'pkg_w') for r in sub):.0f}  "
          f"MHz {min(i(r,'mhz_avg') for r in sub)}-{max(i(r,'mhz_avg') for r in sub)}  "
          f"fan {min(i(r,'fan1') for r in sub)}-{max(i(r,'fan1') for r in sub)}")

# --- throttle events -------------------------------------------------------
def flagged(r):
    out = []
    for k, lbl in (("pkg_prochot","PKG_PROCHOT"),("core_prochot","CORE_PROCHOT"),
                   ("gfx_prochot","GFX_PROCHOT"),("core_thermal","CORE_THERMAL"),
                   ("gfx_thermal","GFX_THERMAL"),("core_rapl","RAPL"),
                   ("core_vr","VR_THERM"),("core_edp","EDP"),
                   ("pkg_therm","PKG_THERMAL")):
        if r.get(k) and i(r, k) == 1:
            out.append(lbl)
    return out

events = [(n, r, flagged(r)) for n, r in enumerate(rows)]
events = [(n, r, fl) for n, r, fl in events if fl]
print(f"\n=== throttle samples: {len(events)} / {len(rows)} ===")

from collections import Counter
print("reason histogram:", dict(Counter(x for _, _, fl in events for x in fl)))

# contiguous episodes
episodes = []
for n, r, fl in events:
    if episodes and n - episodes[-1][-1][0] <= 2:
        episodes[-1].append((n, r, fl))
    else:
        episodes.append([(n, r, fl)])
print(f"episodes: {len(episodes)}")

for e_i, ep in enumerate(episodes[:8]):
    n0, r0, fl0 = ep[0]
    print(f"\n--- episode {e_i}: sample {n0} phase={r0['phase']} len={len(ep)}s "
          f"reasons={sorted({x for _,_,fl in ep for x in fl})}")
    print(f"    pkg={r0['pkg_c']}C dTj={r0['pkg_dtj']} (TjMax-dTj="
          f"{100-i(r0,'pkg_dtj')}C) W={r0['pkg_w']} MHz={r0['mhz_avg']} "
          f"fan={r0['fan1']}/{r0['fan2']}")

if not episodes:
    print("\nNo throttle episode captured -> chassis guard cannot be calibrated "
          "from this run.")
    sys.exit(0)

# --- plateau vs follower at first assertion --------------------------------
n0 = episodes[0][0][0]
print(f"\n=== channel ranking at first assertion (sample {n0}) ===")
print(f"{'key':6s} {'@event':>7s} {'base5m':>7s} {'rise':>6s} "
      f"{'slope60s':>9s} {'verdict'}")

win_pre = rows[max(0, n0 - 300):n0]
slope_win = rows[max(0, n0 - 60):n0 + 1]
out = []
for s in sensors:
    at = f(rows[n0], s)
    if not (5.0 <= at <= 110.0):
        continue
    base = st.median([f(r, s) for r in win_pre]) if win_pre else at
    if len(slope_win) >= 10:
        ys = [f(r, s) for r in slope_win]
        xs = list(range(len(ys)))
        mx, my = st.mean(xs), st.mean(ys)
        den = sum((x - mx) ** 2 for x in xs)
        slope = (sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / den * 60) if den else 0.0
    else:
        slope = 0.0
    out.append((at, base, at - base, slope, s))

for at, base, rise, slope, s in sorted(out, key=lambda t: -t[3]):
    verdict = "FOLLOWER (still climbing)" if slope > 1.5 else \
              ("PLATEAU -> candidate" if rise > 3 else "flat/cold")
    print(f"{s:6s} {at:7.2f} {base:7.2f} {rise:6.2f} {slope:9.2f} {verdict}")

# --- repeatability across episodes ----------------------------------------
if len(episodes) > 1:
    print(f"\n=== repeatability: value at each assertion ({len(episodes)} episodes) ===")
    print(f"{'key':6s} " + " ".join(f"{'ep'+str(k):>7s}" for k in range(len(episodes))) + "   spread")
    idxs = [ep[0][0] for ep in episodes]
    scored = []
    for s in sensors:
        vals = [f(rows[n], s) for n in idxs]
        if not all(5.0 <= v <= 110.0 for v in vals):
            continue
        scored.append((max(vals) - min(vals), vals, s))
    for spread, vals, s in sorted(scored)[:15]:
        print(f"{s:6s} " + " ".join(f"{v:7.2f}" for v in vals) + f"   {spread:5.2f}")
    print("\nLowest spread = same absolute value at every assertion = causal candidate.")
