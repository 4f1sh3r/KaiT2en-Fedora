This repository contains Apple T2 Thunderbolt and PCIe fixes.

## Patches

- `patches/0001-thunderbolt-add-device-links-for-integrated-Apple-T2-NHI.patch`
  Fix for integrated T2 Thunderbolt NHIs. It creates the
  missing device links for ACPI `TRP*` root ports so the warning
  `device links to tunneled native ports are missing!` goes away and
  Thunderbolt tunnel ordering is correct after sleep.
- `patches/0002-PCI-portdrv-use-INTx-for-Apple-T2-Thunderbolt-root-port-services.patch`
  It matches ACPI `TRP*` root ports and forces PCIe port services onto INTx instead of MSI/MSI-X. 

## Install t2thunderbolt fix

If you want to test the Thunderbolt patch as a module:

```sh
make
sudo make install
sudo dracut -f
```

This builds and installs `t2thunderbolt.ko` from `drivers/thunderbolt/`
against the running kernel.

For the pcie fix in `0002`, a kernel rebuild is required
because it changes `drivers/pci/pcie/portdrv.c`.
