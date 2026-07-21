# t2thunderbolt

`t2thunderbolt` supplies the missing power-management dependencies between
Thunderbolt PCIe ports and their NHI on Apple T2 Macs. These device links make
the driver core resume the NHI before ports whose PCIe tunnels it restores.

The module is a quirk helper and does not bind to the Thunderbolt controller or
replace the in-tree `thunderbolt` driver. Titan Ridge controllers are matched
through their PCIe switch topology. Ice Lake controllers use Apple's `TRP*`
ACPI root-port names and are limited to the two Ice Lake NHI PCI IDs.

Apple's Darwin ACPI path powers down the switch ports hosting Titan Ridge xHCI
controllers, but the controllers subsequently report a context save/restore
error. The module keeps only those downstream ports and their xHCI functions
out of D3. Ice Lake has an integrated xHCI function instead of that switch
topology, so only its `8086:8a13` controller is kept in D0. This avoids changing
unrelated platform power-management policy.

## Known issue

The JHL7540 xHCI controllers still report `USBSTS 0x401` during resume. Their
internal context is lost even though PCI keeps the controllers in D0, so the
xHCI driver detects the failed restore and reinitializes them. A future in-tree
xHCI quirk should mark these controllers for reset on resume instead of first
attempting to restore a context the platform does not preserve.
