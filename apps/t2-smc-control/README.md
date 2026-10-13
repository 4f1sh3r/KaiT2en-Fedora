# T2 SMC Control

Small GTK/libadwaita control app for T2 Macs using the `t2smc` kernel driver.

## Features

- Shows fan speeds reported by `t2smc`
- Shows temperature sensors exposed through hwmon
- Shows all dynamically discovered `P...` SMC power stats below the primary
  battery/adapter values, using known human-readable key descriptions
- Shows event-driven SMC battery and power-adapter telemetry
- Shows the `t2smc` RTC (in UTC) when available and can write the current
  system time to it
- Shows the battery charge limit read-only, and when it is lifted for a
  calibration charge
- `t2-smc-gaugingd` lifts the charge limit for a full charge from time to
  time, so the battery gauge can recalibrate, like macOS does

## Requirements

- A T2 Mac
- Linux with the `t2smc` kernel module loaded
- Rust/Cargo
- GTK 4 and libadwaita development packages
- `pkexec` and `hwclock` for writing the hardware clock

## Build

```sh
make build
```

## Install

```sh
sudo make install
```

This installs:

- `/usr/local/bin/t2-smc-control`
- `/usr/local/share/applications/org.t2smccontrol.gtk.desktop`
- `/usr/local/share/icons/hicolor/scalable/apps/org.t2smccontrol.gtk.svg`
- `/usr/local/lib/systemd/system/t2-smc-gaugingd.service`, enabled and started

## Battery charge limit

The charge limit is shown here. `t2smc` exposes it as
`/sys/class/power_supply/BAT0/charge_control_end_threshold`, so the desktop
environment drives it natively and restores it on its own at every start.
GNOME puts it under Settings -> Power -> Battery Charging, KDE under Energy
Saving -> Charge Limit.

## Gauge calibration

A permanent charge limit keeps the battery gauge from learning Qmax and DOD0.
Its estimate drifts and the remaining capacity can collapse under load. powerd
on macOS therefore lifts the limit for a full charge when DOD0 did not change
for 3 days, Qmax for 14 days or the last full charge is older than 21 days.
It gives up on a full charge after 12 hours.

`t2-smc-control --daemon`, run by `t2-smc-gaugingd.service`, does the same. It
reads Qmax and DOD0 per cell from `t2smc` and sets
`battery_charge_limit_suspended` while a full charge is needed. The limit set
in the desktop stays as it is. The dates live in
`/var/lib/t2-smc-control/gauging`.

The app auto-detects `t2smc`/`macsmc` under `/sys/class/hwmon`.
