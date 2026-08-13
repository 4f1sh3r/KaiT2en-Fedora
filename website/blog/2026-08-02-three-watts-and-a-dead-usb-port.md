---
title: Three watts and a dead USB port
date: 2026-08-02
author: Andre Eikmeyer
summary: >-
  Runtime-suspending Thunderbolt saved real power on the MacBookPro15,1, until
  USB 3 hotplug stopped working and the apparently obvious patch fell apart.
tags: [thunderbolt, power, debugging]
---

The MacBookPro15,1 has two Titan Ridge Thunderbolt controllers. Keeping both
awake costs roughly three watts. On a battery-powered laptop that is not a
rounding error, so getting them into runtime suspend looked like one of the
largest remaining wins.

And for a moment it was.

Both controllers suspended, the machine reached package C7, external displays
still worked and idle power dropped. Then a Samsung T7 stopped appearing when
plugged in. Another SATA-to-USB adapter worked only through one particular hub.
USB 2 was fine. USB 3 hotplug was not.

The first half of the result looked exactly like what we wanted:

```text
0000:06:00.0  auto  suspended  D3hot
0000:7c:00.0  auto  suspended  D3hot
```

The missing Samsung T7 was the part no power statistic could show us.

## The tempting wrong answer

The first patch kept the Titan Ridge USB controllers in D0. That restored
hotplug and made the immediate problem disappear. It also threw away a large
part of the power saving we had just found.

Worse, two nominally identical 15,1 machines initially behaved differently.
One looked healthy, one looked broken. That sent us toward a race condition in
the driver until we realised our test environments were not actually equal.
One module was available much earlier during boot than the other.

That is exactly how a workaround grows into a bad upstream quirk: test two
different setups, mistake the difference for hardware behaviour, then freeze
the accident into the kernel.

We withdrew the xHCI patch.

The withdrawn submission remains in
[Patchwork](https://patchwork.kernel.org/project/linux-usb/patch/20260730210655.15514-1-dev@deq.rocks/).
Keeping failed approaches visible matters because the next person will
otherwise rediscover the same attractive workaround.

## Native PCIe services changed the picture

Letting Linux manage the PCIe port services with `pcie_ports=native` restored
USB 3 hotplug without forcing the controllers to stay in D0. It also made the
PCIe tree behave much more consistently during runtime power management.

That does not mean the whole Thunderbolt story is finished. The RTD3 path can
still interact badly with full system suspend, and we are not going to call a
machine fixed when it occasionally needs a hard reset. But we now have a much
better baseline: deep package sleep and working USB 3, without an xHCI quirk
that hides the real problem.

Sometimes upstream progress is a patch being accepted. Sometimes it is
withdrawing your own patch before other people have to live with it.

The related Thunderbolt device-link work is still moving upstream separately;
its current revision can be followed in
[Patchwork](https://patchwork.kernel.org/project/linux-usb/patch/20260731161842.12636-1-atharvatiwarilinuxdev@gmail.com/).
