---
title: Hybrid graphics on the MacBookPro15,1, and the road upstream
date: 2026-08-09
author: KAIT2EN
summary: >-
  Sample post. Runtime power management for the discrete GPU shipped
  downstream first; here is what the upstream series looks like.
tags: [graphics, upstream]
---

*This is placeholder text for the new blog. Replace it with a real post.*

On a MacBookPro15,1 the discrete AMD GPU is wired up as the display GPU by
default, and it never idles. That costs battery for no benefit whenever you are
reading a terminal.

## What hybrid mode does

With hybrid graphics enabled, the integrated GPU drives the display and the AMD
GPU stays in D3cold until something asks for it. PRIME offload wakes it for
accelerated work and the kernel puts it back afterwards. You keep the discrete
GPU without paying its idle cost.

The switch lives in **T2 Hybrid GPU Control**, installed automatically on that
model. Changing the stored boot GPU never reboots for you - that is always a
separate, deliberate action, because the discrete-GPU setting is also the
recovery path when something goes wrong.

## The upstream series

Three patches, and they have to go together:

| Patch | Subsystem | What it does |
| --- | --- | --- |
| 1 | `platform/x86` | Teach `apple-gmux` the MacBookPro15,1 dGPU power-off sequence |
| 2 | `drm/amdgpu` | Consult Apple GMUX for runtime PM |
| 3 | `ALSA/hda` | Allow direct-complete when the GPU is already powered off |

The third one surprises people. The GPU carries an HDA audio function for HDMI
output, and if that function insists on being resumed during system suspend, the
whole runtime-PM arrangement unravels. Suspend has to be allowed to complete
directly while the device is off.

## Why it is not merged yet

Version three of the series is out for review. The board on the front page
tracks it, and the row will change state on its own the moment something
happens - the data behind it is the same data that drives the
`#upstream-work` channel on Discord.

Until then it works here, downstream, on one specific model. That is the honest
description of the situation, which is exactly what the two-axis board was
built to show.
