---
title: The MacBookPro15,1 finally has hybrid graphics
date: 2026-08-12
author: André Eikmeyer
summary: >-
  The discrete GPU can finally sleep while Intel drives the display, saving
  roughly 12 watts without giving up PRIME offload or external monitors.
tags: [graphics, upstream]
---

The MacBookPro15,1 normally boots Linux with its AMD GPU in charge of the
display. It works, it is fast and it uses around 24 watts while doing almost
nothing. Roughly half of that disappears when the discrete GPU is actually
allowed to turn off.

That missing half is why hybrid graphics mattered so much.

With Intel as the primary GPU, the desktop runs on the integrated graphics and
the AMD GPU can stay in D3cold. An application using `DRI_PRIME=1` wakes it on
demand. An external monitor wakes it because the display outputs are connected
there. Close the application or unplug the monitor and it goes back to sleep.

That is how everyone expects a dual-GPU laptop to behave. It just took quite a
lot of work to make this particular one agree.

## Turning it off was the easy part

Apple GMUX can cut power to the GPU. The hard part was getting the Polaris GPU
back after that happened. The normal GMUX sequence left its PCI configuration
space inaccessible. Following the additional link transitions exposed by the
firmware made power-on reliable on both the 2018 and 2019 revisions.

Before runtime switching was possible at all, AMDGPU also needed a reset quirk
for this machine. Without it the GPU's SMU could remain dead after resume. That
smaller foundation patch was
[merged upstream](https://lore.kernel.org/all/20260722125734.6541-1-dev@deq.rocks/)
before the larger hybrid series was ready.

AMDGPU still had to learn that GMUX is a valid runtime power method. Linux
already understood other laptop power schemes, just not this one.

Then system suspend woke the powered-off GPU again for its HDMI audio
function. That was the final surprise: the graphics fix needed a small ALSA
change so an audio device attached to a GPU that is physically off can remain
off during suspend.

Three subsystems, one feature.

## Making it usable

The firmware preference for the boot GPU lives in an Apple NVRAM variable.
Expecting users to discover and write that variable by hand would turn a
working kernel feature into trivia for people who already know the answer.

KAIT2EN therefore installs **T2 Hybrid GPU Control** on the 15,1. It selects
the integrated boot GPU, shows whether the discrete GPU is in DynOff or DynPwr
and leaves ordinary PRIME offload to the desktop and applications.

The feature has been tested with repeated wakeups, suspend and resume, and a
collection of Thunderbolt and USB-C displays. It is now our default path for
this model.

## Upstream is where it should live

The downstream version requires KAIT2EN to rebuild parts of AMDGPU whenever
Fedora updates the kernel. That is inconvenient for users and a maintenance
job we do not want forever.

The three-patch series has now been sent upstream to the platform, AMDGPU and
ALSA maintainers. Review already improved the HDA integration and caught a PCI
device lifecycle problem in the GMUX patch. It also produced several revisions
in one afternoon, including one correction after we checked what Apple's ACPI
methods really return instead of assuming.

That process can look messy from outside. It is still better than carrying the
same private kernel patch for years. The goal is a future Fedora kernel where
the 15,1 simply has hybrid graphics, whether KAIT2EN is installed or not.

The current series is available in the
[kernel mailing list archive](https://lore.kernel.org/all/20260812144750.36797-1-dev@deq.rocks/).

## Phoronics picked it up the same day

Actually impressive how fast these guys are. And also the fact that they picked
it up shows that hybrid graphics support on Apple MacBooks is a desperately
wanted feature by users. Also I believe this will be an interesting find for all
other muxed Macbooks like T1 etc.., Here is the link to the
[Phoronics article](https://www.phoronix.com/news/Linux-2026-Patches-For-2018-MBP)
