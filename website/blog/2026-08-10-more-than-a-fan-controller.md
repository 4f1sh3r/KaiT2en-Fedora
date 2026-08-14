---
title: More than a fan controller
date: 2026-08-10
author: Andre Eikmeyer
summary: >-
  The T2 SMC gave Linux a working clock, charge limiting, standard fan control
  and enough sensors to finally ask where the heat and power are coming from.
tags: [smc, power, tools]
---

For a long time, SMC support on Intel Macs mostly meant temperatures and fan
speeds. That was already useful, but on a T2 Mac it left a surprising amount
of the hardware in our Macs behind a locked door. `applesmc` was built for a
different generation of hardware and is now on its way out. The future is
Asahi Linux's `macsmc`.

In KAIT2EN we already implemented that as `t2smc`.

## A downstream runway for macsmc

@ruiCON, better known around KAIT2EN as `@byte`, is working on the patches that
will make T2 Macs use `macsmc` upstream. Those patches are not submitted yet,
but waiting for the entire upstream journey before touching any of this would
leave both the hardware and the code largely untested.

We therefore wrote `t2smc` as a downstream relative of `macsmc`. It gives
KAIT2EN users the useful interfaces now, while giving us somewhere to test SMC
keys, find model differences and work out which parts are actually dependable.
The results feed back into the future `macsmc` support instead of becoming a
second driver we intend to maintain forever.

That distinction matters. `t2smc` is not an attempt to race `macsmc` into the
kernel under another name. It is the test bench we can put in users' hands
today.

## Back to the Future

The most boring feature turned out to be one of the most important: RTC.

The hardware clock on these machines was effectively broken under Linux. If the
time drifted far enough, the usual advice was to boot macOS and let it straighten
things out. That is annoying when macOS is still installed and a proper disaster
when someone has wiped it from the disk. Suddenly certificates are not valid
yet, SSL falls over and the network looks broken because the laptop thinks it
has travelled through time.

```text
andre@fedora:~/Develop/kait2en$ sudo dmesg
[19.70] rtc_cmos: 70s Baby! weed, afro hair, flared pants. AH YE!
```

`t2smc` exposes the clock behind the T2 as a normal Linux RTC. Loaded early in
the initramfs, it can become `rtc0` and set the system clock during boot. No
macOS rescue trip required. Just load `t2smc`, blacklist `applesmc` and prevent
`rtc_cmos` from initializing, and you're back to 2026 RAM and gas prices. Isn't
that better?

## 70s Dodge Charger goes for a Charge Limit

The same driver exposes Apple's battery charge limit. T2 SMC Control can set
that limit and restore it after boot, which is particularly useful for machines
that spend most of their lives attached to a charger.

This sounds like a small quality-of-life feature until the alternative is
keeping an ageing MacBook battery pinned at full charge all day. A slider and a
persistent setting are much better than another magic SMC key hidden in a shell
command.

## Any KAIT2EN fans here? Everyone should have fans

Fan control also moved onto the standard `hwmon` interface. Current, minimum,
maximum and target speeds are visible where ordinary Linux monitoring software
expects them, and writing a target switches that fan into manual mode.

That means users are no longer tied to the old `fan-rd` setup just because it
was the only thing that knew how to reach the hardware. Existing fan daemons
can use the same interfaces, while KAIT2EN provides its own GUI for people who
would rather not manage cooling with terminal commands.

<figure>
  <img src="../img/blog/t2-fan-control.jpg" alt="T2 Fan Control showing cooling profiles, a custom fan curve, temperatures and both fans">
  <figcaption>T2 Fan Control is one frontend for the standard hwmon interface. Profiles and custom curves are convenient, but the driver does not lock users into this particular app.</figcaption>
</figure>

## Drop it like it's hot! Drop it like it's hot!

Temperatures were only the beginning. `t2smc` discovers the available thermal
and power keys dynamically, and also exposes battery and adapter telemetry. On
our machines T2 SMC Control ends up presenting more than 100 values: CPU and GPU
temperatures, proximity sensors, voltage regulators, battery state, adapter
power and plenty of four-letter Apple keys that still need to earn a friendly
name. Because nobody knows what they actually show. But the sheer mass of sensors
is quite impressive and gives us hints about how much control the T2 and macOS have.

<figure>
  <img src="../img/blog/t2smc.jpg" alt="T2 SMC Control showing charge limiting, RTC status and a large collection of Apple SMC sensors">
  <figcaption>T2 SMC Control turns the driver's RTC, charge limit, fan, temperature and power interfaces into one view. On our test machines that means more than 100 reported values.</figcaption>
</figure>

That mass of data is not there merely because a large sensor list looks
impressive. It became one of our best tools for tracing PROCHOT. When every CPU
core suddenly drops to 800 MHz, the CPU registers can tell us that an external
throttle happened. The SMC sensors help us work out what around the CPU was hot,
how much power the machine was drawing and whether the shared cooling system was
already saturated.

Now fan curves, battery experiments, charger behaviour and power measurements no
longer have to be guessed from one package temperature.

## Helluvaloop

Testing T2 kernel work used to mean pointing somebody at a special T2Linux
kernel branch and asking them to compile the whole thing. Very few testers were
set up to do that, and even fewer felt like spending roughly two hours building
a kernel just to test two small changes. That is a fairly efficient way to lose a
volunteer before the first log arrives.

KAIT2EN packages `t2smc` through DKMS instead. A tester can install a new module
in minutes, reboot and report what their particular MacBook exposes. We can
send an adjustment back just as quickly. During active debugging, dozens of
willing users can try the same change without each of them becoming a kernel
builder first.

That feedback loop is gold for hardware we do not have on the desk. It is how
an unknown SMC key turns into a verified sensor, how a model-specific regression
gets caught early.

`t2smc` started as the missing hardware interface behind a fan controller. It
ended up fixing the clock, making charge limiting practical and giving the rest
of our power work eyes. @ruiCON's `macsmc` work gives that exploration a proper
upstream destination. Not bad for the part of the machine most users should
never have to think about.
