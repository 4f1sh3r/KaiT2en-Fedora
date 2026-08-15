# Apple T2 Thunderbolt device links v8

- Status: preparing
- Previous version: v7 submitted on 2026-07-31
- Scope: move Apple device-link discovery to `pci.c`, add Titan Ridge and
  Ice Lake matching
- Build status: builds against Linux 7.2
- Runtime test status: not tested
- Base: Linux 7.2

## Relationship to the downstream module

The series covers the PM ordering links currently created by
`modules/t2thunderbolt`:

- Patch 1 moves the existing discrete Apple link helper from `tb.c` to
  `pci.c` following maintainer feedback.
- Patch 2 enables the discrete helper for Titan Ridge NHIs. On the
  MacBookPro15,1 this corresponds to the links from the hotplug-capable Titan
  Ridge downstream ports to their NHI.
- Patch 3 adds Ice Lake links using the four Thunderbolt root-port PCI IDs.
  The downstream module currently identifies these ports through Apple's
  `TRP*` ACPI names instead.

The series does not implement the Titan Ridge xHCI D3 restriction used by the
known-good MacBookPro15,1 PC3 baseline. Testing the series alone therefore
cannot establish correct system resume on that model.

The series also does not enable NHI RTD3, change PCI D-state policy or modify
Thunderbolt suspend and resume callbacks. MacBookPro15,1 RTD3 and D3 policy
work can therefore continue independently while `t2thunderbolt` supplies the
equivalent Titan Ridge ordering links downstream.

## Test requirements

1. Test Titan Ridge device links on both MacBookPro15,1 revisions.
2. Test Ice Lake links on at least one four-port and one two-port T2 model.
3. Verify system suspend/resume ordering, Thunderbolt hotplug and the absence
   of missing-device-link warnings.

## Review before submission

- Patch 1 is a code move and still calls the helper from `tb_probe()`; the
  commit message should not imply broader connection-manager coverage.
- Patch 1 contains the wording `create add device link`.
- Patch 2 needs a problem statement and test description instead of only
  stating that Titan Ridge links are added.
- Patch 3 contains grammar and capitalization errors, uses raw root-port PCI
  IDs, and carries an obsolete co-developer email address.
- A cover letter and a series-level test account are still missing.
