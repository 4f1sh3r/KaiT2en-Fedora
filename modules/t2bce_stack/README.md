# t2bce_stack

This is the DKMS build unit for the tightly coupled T2 BCE driver stack. It
builds `t2bce_dma`, `t2bce_core`, `t2bce_vhci`, `t2bce_audio`, and
`t2bce_ave` together so Kbuild and modpost share one symbol-version namespace.
The installer stages AVE from `t2-services/t2-ave/kernel/t2bce_ave`; the other
members remain under `modules/`. AVE source ownership does not change the shared
DKMS build or module names.

The individual source directories remain independently buildable for driver
development. They are staged below this directory by the Fedora DKMS installer.
