<p align="center">
  <img src="../assets/kaiT2en-logo-tr.png" alt="KaiT2en logo" width="220">
</p>

# Introduction

1. [Introduction](00-introduction.md) (you are here)
2. [Get Broadcom firmware from macOS](01-get-broadcom-firmware.md)
3. [Prepare macOS and the Fedora installer](02-prepare-macos-and-fedora-usb.md)
4. [Install Broadcom firmware on Fedora](03-install-broadcom-firmware.md)
5. [Install KaiT2en modules and apps](04-install-kait2en-modules-and-apps.md)

[Back to README](../README.md) | Next: [Get Broadcom firmware from macOS](01-get-broadcom-firmware.md)

KaiT2en is a practical Fedora path for developers and users on Apple T2 Macs.
Our goal is to keep T2 Macs usable on stock Linux, move real fixes upstream, and
reduce the need for special T2 distributions over time.
It can be installed on top of existing T2linux.org Fedora installations or
on top of stock Fedora. In our opinion this is T2linux as it should be to speed
up development and provide users with a properly working upstream kernel.

It blacklists already upstreamed drivers and replaces them with its own.
This makes it possible for developers to quickly test patches without the need
of recompiling the kernel. Also users can quickly profit from the latest efforts.

It is not a separate distribution or a repackaged Fedora image. The
base system is stock or T2linux Fedora. KaiT2en adds the missing T2-specific firmware
steps, kernel arguments, DKMS modules and helper apps from this repository.

KaiT2en targets Fedora as a deliberate choice and "single source of truth" for
development and debugging. Clean installs start from stock Fedora. Existing T2
Linux Fedora installations can use the installer to replace their current T2
drivers with KaiT2en DKMS modules and apps. Note there will be still remnants
of T2linux stuff when doing so.

A plain Fedora installer still needs an external keyboard and mouse at first,
because the stock kernel does not drive the internal T2 input devices yet.
In our opinion this is still the best way of installing KaiT2en and you can
be sure to have old workaround out of your way.

The setup is intentionally explicit. You will use the terminal, inspect logs and
know which file was installed where.

The DKMS modules in this repository build against the currently installed
kernel. That lets KaiT2en react to driver fixes without rebuilding and shipping
a whole patched kernel for every change.

The repository is meant to be used as an offline USB kit. Copy it to a USB
drive, keep that drive connected, and run all commands from the repository root
unless a guide says otherwise.

**This repository is meant to be copied to a USB drive before installation. Keep
that drive connected while working through the guides. Whenever a guide asks you
to run a script or paste a command block, open a terminal in the root folder of
this repository first.** The guides use relative paths, so files are expected below
that folder.

**macOS must stay installed.** It is the clean source for Apple firmware, it can
recover T2/bridgeOS hardware states, and it is the only place where bridgeOS
panic logs are available. If you want to erase macOS completely, KaiT2en is the
wrong path. We are focussed on fixing and debugging things. Don't ask for support
when using custom installations.

Each of our guides does one job. Read it, run the commands, check the result,
then move to the next guide. The point is to make the setup understandable
and repairable.

Firmware files copied from macOS are never distributed by KaiT2en. The guides
only show how to copy the correct firmware files from your own Mac and install
them locally. This is important because T2linux.org is missing a few important
steps here. For example it never installs the bluetooth firmware.
KaiT2en will find it and convert it from `.hex` to the needed `.hcd` format
using a fork of hex2hcd which was specifically re-written to support the
particularities of Apple Bluetooth firmware files.

So for now best luck for the installation. If you happen to run into issues or inconsistencies,
please open a PR or file an issue.

Next: [Get Broadcom firmware from macOS](01-get-broadcom-firmware.md)
