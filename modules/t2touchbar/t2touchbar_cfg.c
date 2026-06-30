// SPDX-License-Identifier: GPL-2.0
/*
 * Apple T2 Touch Bar USB configuration selector.
 *
 * The Touch Bar display exposes configuration 1 as HID-only and
 * configuration 2 as HID + display transport.  Let usbcore choose
 * configuration 2 before interface drivers bind to configuration 1.
 */

#include <linux/module.h>
#include <linux/usb.h>
#include "hid-ids.h"

#define T2_TOUCHBAR_DISPLAY_CONFIG 2

static int t2touchbar_cfg_choose_configuration(struct usb_device *udev)
{
	struct usb_host_config *config;
	int i;

	for (i = 0; i < udev->descriptor.bNumConfigurations; i++) {
		config = &udev->config[i];
		if (config->desc.bConfigurationValue == T2_TOUCHBAR_DISPLAY_CONFIG) {
			dev_info(&udev->dev, "selecting Touch Bar display configuration %d\n",
				 T2_TOUCHBAR_DISPLAY_CONFIG);
			return T2_TOUCHBAR_DISPLAY_CONFIG;
		}
	}

	dev_warn(&udev->dev, "Touch Bar display configuration %d not found\n",
		 T2_TOUCHBAR_DISPLAY_CONFIG);
	return -ENODEV;
}

static const struct usb_device_id t2touchbar_cfg_id_table[] = {
	{ USB_DEVICE(USB_VENDOR_ID_APPLE, USB_DEVICE_ID_APPLE_TOUCHBAR_DISPLAY) },
	{ }
};
MODULE_DEVICE_TABLE(usb, t2touchbar_cfg_id_table);

static struct usb_device_driver t2touchbar_cfg_driver = {
	.name = "t2touchbar_cfg",
	.choose_configuration = t2touchbar_cfg_choose_configuration,
	.id_table = t2touchbar_cfg_id_table,
	.generic_subclass = 1,
	.supports_autosuspend = 1,
};

static int __init t2touchbar_cfg_init(void)
{
	return usb_register_device_driver(&t2touchbar_cfg_driver, THIS_MODULE);
}

static void __exit t2touchbar_cfg_exit(void)
{
	usb_deregister_device_driver(&t2touchbar_cfg_driver);
}

module_init(t2touchbar_cfg_init);
module_exit(t2touchbar_cfg_exit);

MODULE_AUTHOR("kait2en");
MODULE_DESCRIPTION("Kait2en T2 Touch Bar USB configuration selector");
MODULE_LICENSE("GPL");
