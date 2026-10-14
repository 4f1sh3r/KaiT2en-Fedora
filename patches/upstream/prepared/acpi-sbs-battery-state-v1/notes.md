# ACPI SBS battery state fixes

- Status: prepared
- Version: v1
- Base: Linux 7.2.9, `drivers/acpi/sbs.c` identical to Linux master on 2026-10-13
- Tested: MacBookPro15,1 (bq20z451 gauge) and MacBookAir9,1, each with a
  charge limit of 80 percent and with a full charge
- Interim fix: `modules/t2sbs`, functionally identical to this patch

## FULLY_CHARGED check

At 90 percent with the limit holding, BatteryStatus was `0x00c0`
(INITIALIZED, DISCHARGING). After a full charge to 100 percent the driver
reports `Full`, so the gauge sets FULLY_CHARGED (bit 5) there.
