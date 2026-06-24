# SPDX-License-Identifier: GPL-2.0-only
ccflags-y := -I$(src)
obj-m := t2thunderbolt.o
t2thunderbolt-objs := nhi.o nhi_ops.o ctl.o tb.o switch.o cap.o path.o tunnel.o eeprom.o
t2thunderbolt-objs += domain.o dma_port.o icm.o property.o xdomain.o lc.o tmu.o usb4.o
t2thunderbolt-objs += usb4_port.o nvm.o retimer.o quirks.o clx.o

t2thunderbolt-${CONFIG_ACPI} += acpi.o
t2thunderbolt-$(CONFIG_DEBUG_FS) += debugfs.o
t2thunderbolt-${CONFIG_USB4_KUNIT_TEST} += test.o
CFLAGS_test.o += $(DISABLE_STRUCTLEAK_PLUGIN)
