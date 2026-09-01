# t2bce_stack

This is the DKMS build unit for the tightly coupled T2 BCE driver stack. It
builds `t2bce_dma`, `t2bce_core`, `t2bce_vhci`, and `t2bce_audio` together so
Kbuild and modpost share one symbol-version namespace.

The individual source directories remain independently buildable for driver
development. They are staged below this directory by the Fedora DKMS installer.
