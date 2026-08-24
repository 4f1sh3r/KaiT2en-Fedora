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

BAT0 on the T2 Mac is a plain ACPI battery (`PNP0C0A`, smart-battery chip
`bq20z451`) via the generic kernel `battery.c` driver. Apple's ACPI tables
don't provide the vendor extension methods a platform driver would hook to
add those attributes, so BAT0 never had them - hence GNOME could never show
the toggle, even though `t2smc`'s own `battery_charge_limit` hwmon attribute
worked fine via SMC keys directly.

## Fix: power_supply extension via battery_hook

Kernel (verified present at 7.1.3-201.fc44 via `/proc/kallsyms`) provides
exactly the mechanism needed:
- `devm_battery_hook_register()` (`include/acpi/battery.h`) to get an
  `add_battery`/`remove_battery` callback whenever an ACPI battery
  power_supply appears/disappears.
- `power_supply_register_extension()` (`include/linux/power_supply.h`) to
  attach extra core properties (`POWER_SUPPLY_PROP_CHARGE_CONTROL_START/
  END_THRESHOLD`) directly onto that power_supply device.

`t2smc` now registers this hook (gated on `has_chls || has_chwa`) and
attaches the extension to `BAT0`, reusing the exact same
`t2smc_get/set_charge_limit()` helpers the hwmon attribute already used.
Single source of truth (the SMC keys) - both interfaces always agree.

## Testing notes

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

## Open problem: two setters, one persistence mechanism

`t2-smc-control` has its own restore-on-boot workaround
(`kait2en-t2-smc-charge-limit.service` re-applies `/etc/t2-smc-control/
config.txt` at every boot) because the SMC sometimes forgets the limit,
e.g. after a T2 crash. That config file is only ever written by the
t2-smc-control app itself.

Now that GNOME can set the same limit natively via the power_supply
extension, it never touches `config.txt`. Verified by reproducing it: set
80% via the app (saved to config.txt), then set 65% via
`charge_control_end_threshold` (simulating a GNOME change), then reboot -
the restore service silently reverted the live value back to 80%,
discarding whatever was last set through the "native" path. Checked whether
`gsd-power` restores the threshold itself at login (which would sidestep
this) - it doesn't; `strings /usr/libexec/gsd-power` has zero
charge-threshold-related code. gnome-control-center's toggle is a one-shot
D-Bus call with no persistence of its own.

Also checked: writing either the power_supply attribute or the hwmon
attribute did **not** trigger a uevent on BAT0 (`udevadm monitor` stayed
silent), so there was no generic way for userspace to notice "the limit
changed" regardless of which interface changed it.

Fix applied here (kernel side only): `t2smc_set_charge_limit()` now calls
`power_supply_changed(t2->battery)` after a successful write, from both the
hwmon store path and the power_supply extension's `set_property` - verified
with `udevadm monitor` that a `change` event on BAT0 now fires for both.

Still open (userspace, deliberately left out of this PR to keep it
minimal): a udev rule reacting to that event to snapshot the current value
into `config.txt` (e.g. via a new `t2-smc-control --save-current-charge-
limit` flag), so the boot-time restore always re-applies whatever was last
actually in effect, regardless of which UI set it.
