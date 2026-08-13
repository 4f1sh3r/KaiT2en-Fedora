---
title: The resume that took a minute
date: 2026-07-24
author: Andre Eikmeyer
summary: >-
  Suspend worked, but every CPU took several seconds to return. The fix was
  not making CPU startup faster. It was waiting for the right moment.
tags: [suspend, upstream]
---

Suspend on a T2 Mac used to be one of those features where the answer was
technically "yes" and practically "please go make coffee".

The machine went to sleep. It also woke up again. But during resume Linux
would bring the secondary CPU cores back one after another, and every single
one could take several seconds. On a six-core MacBook Pro with Hyper-Threading
that adds up quickly. The screen could already be lit while keyboard, trackpad
and the rest of the system were still waiting for the CPU parade to finish.

All in all 30 seconds wake time on an I7 with 12 cores. Users with I9 even
reported 1 minute and 30 seconds.

## The clock was telling us something

The strange part was that the CPUs were not generally broken. Taking a core
offline and bringing it back after the machine had resumed was fast. It was
only slow during the early resume path. The `smpboot` aka CPU-bringup and
EC-unblock.

Long story short: The solution was to move the secondary CPUs offline before the
generic suspend code closes the CPU hotplug window, then restore them after the
platform has resumed far enough for normal hotplug to be fast again.

That changed the experience from a machine slowly assembling itself after
every wake to a normal laptop resume. In other words: we went from a 30
second wake to a 5 second wake.

## From shell workaround to kernel patch

The first version lived in our suspend helper scripts because that was the quickest
way to prove the idea on real machines. Once it was clear that the timing was
the actual problem, it moved into a small T2-specific module which we called t2smp.
The final step was turning it into a kernel patch instead of keeping a permanent
workaround downstream.

It has since been tested on several T2 MacBooks and an iMac. The upstream
submission is limited to machines where the T2 chip is present.

The current submission is available in the
[kernel mailing list archive](https://lore.kernel.org/all/20260812120326.155226-1-dev@deq.rocks/).

## And still... 5 seconds is too slow

This one was easier. The Apple platform explicitly denied advanced error
reporting (AER). But Linux would still use it by default. Luckily, someone with
far greater knowledge than I will ever have, implemented the kernel param
`pci=noaer`. And that is what KAIT2EN ships with now and what makes the embedded
controller unblock instantly.
