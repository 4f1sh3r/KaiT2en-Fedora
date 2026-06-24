# T2 Fan Control

<p align="center">
<img src="assets/fancontrol.svg" alt="T2 Fan Control logo" width="96">
</p>

<p align="center">
<img src="assets/t2fancontrol_dark.png" alt="T2 Fan Control dark mode screenshot" style="height: 400px;">
</p>
Fan controller application written in Rust for T2 Macs running Linux.
Supports applesmc, macsmc hwmon and t2smc (https://github.com/deqrocks/t2-smc) hwmon paths.


T2 Fan Control provides a compact desktop interface for monitoring temperatures, editing the fan curve and switching between presets.

Fan control is handled by a background daemon integrated with systemd. This keeps the state persistent across boot, suspend and resume, while the GUI talks to the daemon over a Unix socket.

## Installation

1. Download or clone the repository
2. Unpack it
3. Run:

```bash
make
sudo make install
```
This step is mandatory. It puts the binary, desktop entry, icon and systemd service in the correct system locations, then enables and starts `t2-fancontrol.service`.

If `t2fanrd.service` is present, `sudo make install` disables and stops it automatically to avoid conflicts.

`sudo make install` does all of the following:

- installs the binary to `/usr/local/bin/t2-fancontrol-gtk`
- installs the desktop entry to `/usr/local/share/applications/org.t2fancontrol.gtk.desktop`
- installs the icon to `/usr/local/share/icons/hicolor/scalable/apps/org.t2fancontrol.gtk.svg`
- installs the systemd unit to `/usr/local/lib/systemd/system/t2-fancontrol.service`
- reloads systemd
- enables and starts `t2-fancontrol.service`
- disables and stops `t2fanrd.service` if it exists

## Uninstall

```bash
sudo make uninstall
```

This removes the installed files, disables `t2-fancontrol.service`, and re-enables `t2fanrd.service` if it is present on the system.

`sudo make uninstall` does all of the following:

- disables and stops `t2-fancontrol.service`
- removes the installed binary
- removes the desktop entry
- removes the installed icon
- removes the installed systemd unit
- reloads systemd
- re-enables and starts `t2fanrd.service` if it exists

## Build Dependencies

You need a Rust toolchain and the usual native build dependencies for GTK4 and Libadwaita.

- `cargo`
- `make`
- `pkg-config`
- `glib-compile-resources`
- GTK4 development files
- Libadwaita development files

## Support

[Fund my bugs](https://donate.stripe.com/eVq14n8a7agh2lQdqq14400)
