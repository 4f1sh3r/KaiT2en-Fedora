modname := t2bce
obj-m += $(modname).o

t2bce-objs := t2bce_main.o mailbox.o queue.o queue_dma.o vhci/vhci.o vhci/queue.o vhci/transfer.o audio/audio.o audio/protocol.o audio/protocol_bce.o audio/pcm.o

MY_CFLAGS += -DWITHOUT_NVME_PATCH
#MY_CFLAGS += -g -DDEBUG
ccflags-y += ${MY_CFLAGS}
CC += ${MY_CFLAGS}

KVERSION := $(KERNELRELEASE)
ifeq ($(origin KERNELRELEASE), undefined)
KVERSION := $(shell uname -r)
endif

KDIR := /lib/modules/$(KVERSION)/build
PWD := $(shell pwd)

.PHONY: all

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
	find $(PWD) -type f \( -name '*.o' -o -name '*.ko' -o -name '*.mod' -o -name '*.mod.c' -o -name '.*.cmd' -o -name 'Module.symvers' -o -name 'modules.order' \) -delete

install:
	$(MAKE) -C $(KDIR) M=$(PWD) modules_install
