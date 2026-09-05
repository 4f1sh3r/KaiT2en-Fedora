# T2SMC Learnings

## GNOME/UPower charge-threshold requirements

GNOME's Settings -> Power -> "Battery Charging" section (Maximize Charge /
Preserve Battery Health) is driven entirely by UPower D-Bus properties on
`org.freedesktop.UPower.Device` (`ChargeThresholdSupported`,
`ChargeStartThreshold`, `ChargeEndThreshold`, `ChargeThresholdEnabled`,
method `EnableChargeThreshold`). UPower itself only populates these from two
standard `power_supply` sysfs attributes on the battery device:
`charge_control_start_threshold` and `charge_control_end_threshold` (or,
failing that, a `charge_types` attribute). Confirmed via `strings
/usr/libexec/upowerd`.

Neither the `sbs` driver behind BAT0 nor Apple's ACPI tables provide those
attributes, so BAT0 never had them - hence GNOME could never show the toggle,
even though `t2smc`'s own `battery_charge_limit` hwmon attribute worked fine
via SMC keys directly.

## BAT0 is an SBS battery, so battery_hook never fires

The obvious mechanism, `devm_battery_hook_register()` from `<acpi/battery.h>`,
does **not** work here, and this cost a full build/reload cycle to find. The
hook registers fine (`ACPI: battery: new hook: t2smc` in dmesg) but
`add_battery()` is never called, because `battery_hook_register()` only walks
`acpi_battery_list`, which is owned by `drivers/acpi/battery.c` and covers
`PNP0C0A` devices only.

On this MacBookPro16,2 that list is empty:

- `/sys/bus/acpi/devices/PNP0C0A:00` exists but reports `status = 0`, so the
  built-in ACPI battery driver never registers anything.
- The real BAT0 comes from `sbs`/`sbshc` (`ACPI0001:00`/`ACPI0002:00`,
  `CONFIG_ACPI_SBS=m`, smart-battery chip `bq20z451`).

An earlier note here claimed the battery_hook path worked - it does not, at
least not on this machine and kernel. Do not retry it.

## Fix: power_supply extension attached to BAT0 directly

`t2smc` now takes `power_supply_get_by_name("BAT0")` and calls
`power_supply_register_extension()` on it, which is driver-agnostic and works
for the `sbs` battery. Only
`POWER_SUPPLY_PROP_CHARGE_CONTROL_END_THRESHOLD` is exposed - the SMC has no
start threshold, and UPower tolerates a missing/failing start write (it only
errors when *both* writes fail, see `err_count == 2` in
`up_device_supply_battery_set_battery_charge_thresholds()`).

The attach is deliberately *not* gated on `has_chls`/`has_chwa`. An early
version was, which would have been a regression against the old app path: the
hwmon `battery_charge_limit` attribute has no `is_visible` and is always
present, and `t2smc_write_charge_limit_method()` returns 0 when neither key
exists, so BCLM alone still gets written - which is how charge limiting worked
on pre-T2 Intel Macs. A board with neither key would have kept working through
the app but lost the native path.

Attach happens in probe and, if BAT0 was not there yet, again from the
existing `power_event_work` - `power_supply_register()` ends with
`power_supply_changed()`, so a late `sbs` still wakes our notifier. That
second attempt is not theoretical: on a real boot t2smc probes a full second
before the battery exists, and the log reads

    00:10:17 t2smc APP0001:00: t2smc 0.0.1 ready (fans=2)
    00:10:18 ACPI: Smart Battery System [SBS0]: Battery Slot [BAT0]
    00:10:18 t2smc APP0001:00: charge control attached to BAT0 Detach is a
devm action registered before the notifier's, so devres unwinds the notifier
(and cancels the work) first.

The extension shows up as `/sys/class/power_supply/BAT0/extensions/
t2smc-charge-control` and adds `charge_control_end_threshold`. hwmon's
`battery_charge_limit` stays and reads/writes the same SMC keys.

## SMC amnesia: UPower alone is enough, no driver-side latch

A first version latched the last requested value in the driver and re-applied
it from a `t2smc_resume()` PM callback. That was dropped again: the SMC only
forgets the limit on a drained battery or a T2 crash, and both come with a
reboot anyway, so a suspend/resume hook guards a case that does not occur. The
latch plus PM ops cost 37 of 181 added lines in the driver for nothing.

Nothing of ours is needed, which is why the old config file and
`kait2en-t2-smc-charge-limit.service` could be dropped entirely: UPower
persists the on/off state in `/var/lib/upower/charging-threshold-status` and
re-applies it on every coldplug
(`up_device_battery_recover_battery_charging_threshold()` in
`src/up-device-battery.c`). Verified on this machine: set 65 by hand,
`systemctl restart upower`, value came back as 80.

The 80 comes from UPower's hwdb default `CHARGE_LIMIT=75,80` in
`/usr/lib/udev/hwdb.d/60-upower-battery.hwdb`, which matches every machine.
GNOME's toggle is on/off only; a different value needs a local override in
`/etc/udev/hwdb.d/61-battery-local.hwdb` plus `systemd-hwdb update`. KDE's
PowerDevil lets the user pick the value directly (end-only hardware supported
since 5.24.1, KDE bug 449997) - untested here, no powerdevil installed.

## Testing notes

- Full cycle verified on 2026-09-05 with kernel 7.2.2-300.kait2en.i915_psr:
  GNOME Settings -> Power -> Battery Charging toggled off/on drives
  `charge_control_end_threshold` 80 -> 100 -> 80. Suspend/resume and reboot
  both keep the extension attached and the value at 80; the SMC did not drop
  it in either case - which is what later justified dropping the re-apply
  path altogether.

- After a **live module reload** (rmmod/insmod or dkms reinstall),
  `upowerd` does not notice the newly-appeared sysfs attributes on its own -
  it only inspects them once at its own startup/coldplug. Needed
  `sudo systemctl restart upower` to see `ChargeThresholdSupported` flip to
  `true`. On a normal boot this isn't an issue since upowerd starts after
  the module is already loaded.
- `busctl call ... EnableChargeThreshold` over a plain SSH session fails
  with "Operation is not allowed" - this D-Bus method requires polkit
  authorization tied to an active local session, so it can only be
  exercised from the real GNOME session on the Mac, not remotely.
- UPower synthesizes its own `ChargeStartThreshold` display value (saw 75
  when our sysfs `charge_control_start_threshold` read 0, end was 80) -
  that's upowerd's own convention for boards without an independently
  writable start threshold, not something to chase in the driver.
- CHWA-only boards (no CHLS) only support a fixed 80% cutoff -
  `charge_control_end_threshold` writes below 80 engage it, writes >= 80
  disable it. Not testable on this particular T2 Mac (`has_chls=1`).
- Reboot and suspend/resume both survive cleanly: module autoloads via DKMS,
  hook/extension re-attach with no dmesg warnings, srcversion matches
  before/after.

## Resolved: the two-setters problem

The old conflict (app config file vs. native path overwriting each other) is
gone because the app no longer sets the limit at all: the slider, the
`--set-charge-limit`/`--apply-saved-charge-limit` flags,
`/etc/t2-smc-control/config.txt` and the restore service were removed.
`make install` now disables and deletes the stale unit; an existing
`/etc/t2-smc-control/config.txt` is left in place, unread.

`t2smc_set_charge_limit()` still calls `power_supply_changed()` after a
successful write so a BAT0 `change` uevent fires whichever interface was used.

The app keeps a read-only readout of `battery_charge_limit` - the upstream
grid is left exactly as it was, the `Scale` in the right column just became a
`ProgressBar` with the same `overview-meter` class, so it now mirrors the
battery bar on the left instead of looking like a disabled slider. It also
gained a "Sync RTC" button (`pkexec /usr/sbin/hwclock --rtc /dev/rtcN --systohc`, the
recipe from `modules/t2smc/README.md`). The RTC readout is raw sysfs, which is
UTC on this machine (`/etc/adjtime` says UTC, `timedatectl` says "RTC in local
TZ: no"), so the label says "RTC (UTC)" - without that the button looks broken,
because a correctly synced clock still reads two hours off the wall clock in
CEST.

The stale `kait2en-t2-smc-charge-limit.service` is retired by
`remove_obsolete_apps()` in `scripts/fedora/install-apps.sh` (new
`OBSOLETE_UNITS` array), not by the app Makefile - the Makefile only installs
this app's own files. `systemctl list-unit-files <unit>` exits 1 when the unit
is unknown, which is what makes the block silent and idempotent on a fresh
install. Verified both ways by re-creating and enabling the unit and running
the real installer twice.

## Lock order: battery_lock never nests inside extensions_sem

`power_supply_set_property()` holds the psy's `extensions_sem` for reading
while it calls our `set_property`, which then takes `battery_lock`. So
`battery_lock` is always acquired *below* `extensions_sem`, and any path that
takes `battery_lock` first must not call into the extension API, which grabs
`extensions_sem` for writing.

`t2smc_detach_battery()` violated that: it held `battery_lock` across
`power_supply_unregister_extension()`. A concurrent write to
`charge_control_end_threshold` during rmmod/unbind deadlocks both sides. The
fix is to take the battery pointer out under the lock, NULL it, drop the lock,
and only then unregister and put the reference.

`t2smc_attach_battery()` still registers under the lock and is fine: it only
reaches `power_supply_register_extension()` while `t2->battery` is NULL, i.e.
while no extension of ours is registered, so no setter of ours can be in
flight on that psy.
