---
title: The small things you touch every day
date: 2026-07-29
author: Andre Eikmeyer
summary: >-
  Keyboard light, display brightness and the Fn key are small features until
  they reset after every reboot or disappear after every suspend.
tags: [input, upstream]
---

Not every useful kernel fix comes with a dramatic hardware failure. Some just
remove one small annoyance that happens every single day.

The keyboard backlight on T2 MacBooks gave us two of those. It could return
from suspend at zero brightness, and the reported brightness levels were read
with the wrong byte order. The light existed, Linux exposed a control for it,
and the result still felt broken.

Both fixes are now upstream:

- [Restore the keyboard backlight after resume](https://patchwork.kernel.org/project/linux-input/patch/20260718121527.15924-1-dev@deq.rocks/)
- [Read the keyboard backlight levels correctly](https://patchwork.kernel.org/project/linux-input/patch/20260718151241.7496-1-dev@deq.rocks/)

## One key, two layouts

The Touch Bar has another everyday problem: sometimes you want media
controls, sometimes you want ordinary F-keys. Holding Fn works, but repeatedly
holding a modifier for an entire application is not a great interface.

The Fn double-press patch adds a persistent layer switch. Two quick presses
toggle between media controls and F-keys, while the normal hold behaviour
remains available. That work is in its
[third upstream revision](https://patchwork.kernel.org/project/linux-input/patch/20260722141221.13844-1-dev@deq.rocks/).

Later runtime-PM testing found a deeper Touch Bar issue. Suspending one of its
HID devices could ask runtime PM to suspend the same path again from inside the
callback. When the timing lined up, suspend or resume simply stopped. The
[deadlock fix](https://patchwork.kernel.org/project/linux-input/patch/20260810140352.35866-1-dev@deq.rocks/)
keeps that nested request out of the PM callback.

## And then there was the display slider

Fedora could restore display brightness on ordinary machines but forgot it on
these Macs after every reboot. The backlight device had no stable path ID, so
systemd had nowhere reliable to store the value.

The fix was was teaching udev how to build a
stable path for the Apple PNP device. That change was
[merged into systemd](https://github.com/systemd/systemd/pull/43255).

This bugged us from 2019 until mid 2026!
