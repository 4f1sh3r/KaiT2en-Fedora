# T2 SMC Control

Small GTK/libadwaita control app for T2 Macs using the `t2smc` kernel driver.

The required kernel driver is maintained here:

https://github.com/deqrocks/t2-smc

## Features

- Shows fan speeds reported by `t2smc`
- Shows temperature sensors exposed through hwmon
- Shows the `t2smc` RTC when available
- Reads and writes the battery charge limit through `battery_charge_limit`

## Requirements

- A T2 Mac
- Linux with the `t2smc` kernel module loaded
- Rust/Cargo
- GTK 4 and libadwaita development packages
- `pkexec` for granting write access to `battery_charge_limit`

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

The app auto-detects `t2smc`/`macsmc` under `/sys/class/hwmon`. Battery charge limit support depends on the loaded driver and the available SMC keys on the machine.
