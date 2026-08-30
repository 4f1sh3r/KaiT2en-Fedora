---
title: How we fixed PSR on T2 Macs
date: 2026-08-29
author: Alexander Fischer
summary: A macOS display trace revealed the sink-specific sequence needed to enable Panel Self Refresh, reduce idle power and reach package C10 on T2 Macs.
tags: [graphics, power, upstream]
---

This did not start as a display problem.

I was experimenting with deep package C-states on a T2 Mac running Linux. Even when the machine was completely idle, it spent much less time in deep sleep states than I expected. This is where the journey began.

After testing a few possible blockers, I eventually traced part of the problem back to the display pipeline. As long as the GPU has to keep sending the same image to the panel, parts of the display engine and memory cannot fully go to sleep.

That is exactly what PSR (Panel Self Refresh) is supposed to solve.

## Why PSR was disabled

PSR lets the display panel keep the current image in its own memory. If nothing changes on screen, the GPU can stop sending the same frame again and again.

On T2 Macs, macOS already uses PSR, but Linux did not.

I did some research and found an old driver commit: [`1035f4a65f58`](https://github.com/torvalds/linux/commit/1035f4a65f58407951d8d2f54c289c2b252e499c) ("drm/i915: Disable PSR in Apple panels"). The commit explained that i915 did not yet support PSR on Apple panels.

Eight years later, that restriction was still there, so I simply removed it and tried again. :)

The panel reported PSR support and the Intel driver enabled the standard PSR register. Unfortunately, the panel never actually captured the current frame. The result was blinking and flickering. Sometimes the display stayed black for a moment before recovering. Or, as @deqrocks discovered even earlier than me: "it turned my bedroom into a 90ies discotheque."

Clearly, something was missing. Apple always "thinks differently."

## Looking at macOS

There was no public documentation for the Apple display controller, but macOS already knew how to make PSR work.

So I booted macOS 15.7.7 and traced the display communication with DTrace.

The trace showed an Apple display controller called a Banksia TCON. More importantly, macOS was sending a few vendor-specific commands that Linux did not know about.

The important sequence looked like this:

```text
DPCD 0x321 = 0x3c
DPCD 0x4d2 = 0x01
DPCD 0x4d1 = 0x03
DPCD 0x170 = 0x01
... and a few seconds later ...
DPCD 0x4d4 = 0x01
```

The last write was the key. Register `0x4d4` tells the TCON to capture the current frame. Without that step, PSR is enabled from the GPU side, but the panel has no frame to refresh.

That explained the blinking almost perfectly, but it did not solve the issue by itself. Linux normally enables CRC verification, while macOS leaves it disabled on these panels.

The working sequence was surprisingly simple:

1. Send the Apple setup commands.
2. Enable PSR without CRC verification.
3. Trigger the frame capture through register `0x4d4`.

## The Linux patch

I added this sequence to the existing i915 PSR path. The matching is intentionally strict: the quirk is only enabled when the display is internal eDP, the panel reports Apple's OUI `00:10:fa`, and the machine contains an Apple T2 bridge controller. It may work on other Macs too, but those systems are untested, so the patch deliberately leaves them unchanged.

## The result

With the patch, PSR now starts automatically at boot and still works after suspend and resume.

On my MacBookPro16,2, I measured about 0.5 W less idle power. More importantly, our T2 Macs can now reach **package C10 with the display active**, which was the reason I started looking at PSR in the first place.

We tested the fix in the KAIT2EN community on five machines with four different panel device IDs. All of them entered PSR successfully and kept working after suspend and resume without panel errors or flickering.

The patch has now been [submitted to the i915 maintainers](https://lore.kernel.org/all/20260829161622.11396-1-alexander.fischer@kait2en.org/).
