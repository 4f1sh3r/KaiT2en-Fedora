# t2sbs

`t2sbs` is a copy of the in-tree ACPI Smart Battery driver
(`drivers/acpi/sbs.c`, Linux 7.2.9) with two fixes. It replaces `sbs` until
the fixes are upstream. The installer blacklists `sbs` through
`module_blacklist`. `sbshc` stays in use as the SMBus host controller.

T2 Macs expose the battery gauge (bq20z451) through the ACPI SBS interface,
so `BAT0` comes from this driver.

## Fixes

- `charge_full` and `cycle_count`: `sbs` reads FullChargeCapacity (0x10) and
  CycleCount (0x17) only at probe or when the battery appears. The gauge
  updates both at run time, after a calibration charge for example. `t2sbs`
  reads them with the other state values. UPower computes the percentage on
  MacBooks from `charge_now / charge_full`, so a stale FullChargeCapacity
  shifts the percentage.
- `capacity`: `sbs` reports AbsoluteStateOfCharge (0x0e), which is relative to
  the design capacity. The power supply ABI defines `capacity` as the charge
  relative to full, which is RelativeStateOfCharge (0x0d).
- `status`: `sbs` reports `Full` whenever no current flows, also while a
  charge limit holds the battery, and `Discharging` for the small currents the
  gauge reports on AC. `t2sbs` reports `Full` only when the gauge sets
  FULLY_CHARGED in BatteryStatus (0x16), otherwise `Not charging`. On AC,
  discharge currents below 100 mA count as idle. The gauge reports 25 to
  80 mA there while a charge limit holds the battery, a real discharge on AC
  draws several amperes and stays `Discharging`.

`rebuild-initramfs.sh` loads `t2sbs` in the initramfs after `t2smc`. `BAT0`
then already has `charge_control_end_threshold` when udev and UPower in the
real root first see it. UPower only decides at that point whether the battery
supports a charge limit, and udev imports `CHARGE_LIMIT` only when the
attribute exists.

macOS reads the same gauge registers on Intel Macs. RemainingCapacity (0x0f)
for `CurrentCapacity`, FullChargeCapacity (0x10) for `MaxCapacity`
(`AppleSmartBattery.cpp` in Apple's PowerManagement sources).

## Build

```sh
make
```

The installer builds it through DKMS like the other modules.
