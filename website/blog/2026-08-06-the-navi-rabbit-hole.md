---
title: The Navi rabbit hole
date: 2026-08-06
author: Andre Eikmeyer
summary: >-
  Hybrid graphics on the 16-inch MacBook Pros can turn the dGPU off. Bringing
  it back reliably is where the hardware starts telling a much stranger story.
tags: [graphics, debugging, work-in-progress]
---

Once hybrid graphics worked on the MacBookPro15,1, it was very tempting to
add the 16,1 and 16,4 to the same list and move on.

They have an Intel GPU, an AMD GPU and Apple GMUX. The names are the same. The
power path is not.

The 15,1 has a Polaris GPU. The 16-inch models use Navi behind their own AMD
PCIe switch. They also expose an AMD eDP connector that looks connected even
while the Intel GPU is driving the panel. Ignore that connector and runtime
power-off starts working. Ask the GPU to wake again and a completely different
set of failures begins.

## DynOff is not the finish line

Seeing `DynOff` in vga_switcheroo feels like success. It only proves that the
GPU went away. Hybrid graphics also needs it to return every time an app or an
external display asks for it.

On the Navi machines the whole PCIe branch can reach D3cold. On resume the
bridges return, PCI configuration space becomes visible and the GPU still may
fail during its secure processor or ASIC initialisation. We tried preserving
bridges, restoring bus numbers, changing reset paths, waiting at different
points and following the firmware power methods more closely.

At its best, the power side looked complete:

```text
0000:00:01.0  suspended  D3hot
0000:01:00.0  suspended  D3cold
0000:02:00.0  suspended  D3cold
0000:03:00.0  error      D3cold
DIS:           DynOff
```

That `error` next to a successfully powered-off GPU is a good summary of the
whole investigation. The machine reached the desired state, but the driver no
longer had a reliable path back from it.

Several versions looked promising for one boot. Some even survived suspend.
Then the next wake ended at a beachball, a dead GPU or a desktop that had to be
restarted from a TTY.

## This seems to be our Waterloo

Literally this was 18 hour days, 3 days in a row, 2 devs hammering trial and error
code with at one poor tester with a MacBook16,1.
Cudos goe out to @byte and @Err0r.

The useful outcome is that the clean, proven foundation for the 15,1 is now
separate from the Navi experiments. The 16,1 and 16,4 work continues on top of
that base instead of slowly turning the working model into a pile of special
cases.

This one nearly drove everyone involved around the bend, and it is not over.
The logs are better, the failure boundary is smaller, and we don't give up.
