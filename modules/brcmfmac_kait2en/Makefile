# SPDX-License-Identifier: GPL-2.0

ccflags-y += \
	-I $(src) \
	-I $(src)/include

obj-m += brcmfmac.o

brcmfmac-objs += \
	cfg80211.o \
	chip.o \
	fwil.o \
	fweh.o \
	p2p.o \
	proto.o \
	common.o \
	core.o \
	firmware.o \
	fwvid.o \
	feature.o \
	btcoex.o \
	vendor.o \
	pno.o \
	xtlv.o

brcmfmac-$(CONFIG_BRCMFMAC_PROTO_BCDC) += \
	bcdc.o \
	fwsignal.o
brcmfmac-$(CONFIG_BRCMFMAC_PROTO_MSGBUF) += \
	commonring.o \
	flowring.o \
	msgbuf.o
brcmfmac-$(CONFIG_BRCMFMAC_SDIO) += \
	sdio.o \
	bcmsdh.o
brcmfmac-$(CONFIG_BRCMFMAC_USB) += usb.o
brcmfmac-$(CONFIG_BRCMFMAC_PCIE) += pcie.o
brcmfmac-$(CONFIG_BRCMDBG) += debug.o
brcmfmac-$(CONFIG_BRCM_TRACING) += tracepoint.o
brcmfmac-$(CONFIG_OF) += of.o
brcmfmac-$(CONFIG_DMI) += dmi.o
brcmfmac-$(CONFIG_ACPI) += acpi.o

KDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)
KVER ?= $(shell uname -r)
MODDEST ?= /lib/modules/$(KVER)/updates

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
	find $(PWD) -type f \( -name '*.o' -o -name '*.ko' -o -name '*.mod' -o -name '*.mod.c' -o -name '.*.cmd' -o -name 'Module.symvers' -o -name 'modules.order' \) -delete

install: all
	install -d -o root -g root -m 0755 $(MODDEST)
	install -o root -g root -m 0644 brcmfmac.ko $(MODDEST)/
	depmod -a $(KVER)

.PHONY: all clean install
