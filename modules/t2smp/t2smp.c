// SPDX-License-Identifier: GPL-2.0
/*
 * CPU resume ordering quirk for Apple T2 Macs
 *
 * Apple firmware selects a different suspend path when _OSI("Darwin") is
 * active. On T2 Macs, bringing secondary CPUs online during the kernel's
 * early resume phase can then take several seconds per CPU. Taking them
 * offline before suspend preparation keeps them out of the suspend core's
 * frozen CPU mask. They can be restored normally after platform resume.
 */

#define pr_fmt(fmt) KBUILD_MODNAME ": " fmt

#include <linux/cpu.h>
#include <linux/cpumask.h>
#include <linux/delay.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/notifier.h>
#include <linux/pci.h>
#include <linux/platform_data/x86/apple.h>
#include <linux/suspend.h>
#include <linux/workqueue.h>

#define PCI_DEVICE_ID_APPLE_T2_BRIDGE 0x1801
#define T2SMP_RESTORE_RETRIES 50

static cpumask_var_t t2smp_offlined_cpus;
static DEFINE_MUTEX(t2smp_lock);
static unsigned int t2smp_restore_retries;

static bool t2smp_restore_cpus(void)
{
	unsigned int cpu;
	unsigned int restored = 0;
	bool retry = false;
	int ret;

	for_each_cpu(cpu, t2smp_offlined_cpus) {
		ret = add_cpu(cpu);
		if (ret) {
			if (ret == -EBUSY) {
				retry = true;
				continue;
			}
			pr_err("failed to restore CPU%u: %d\n", cpu, ret);
			continue;
		}

		cpumask_clear_cpu(cpu, t2smp_offlined_cpus);
		restored++;
	}

	if (restored)
		pr_info("restored %u secondary CPUs\n", restored);
	return retry;
}

static void t2smp_restore_workfn(struct work_struct *work);
static DECLARE_DELAYED_WORK(t2smp_restore_work, t2smp_restore_workfn);

static void t2smp_restore_workfn(struct work_struct *work)
{
	bool retry;

	mutex_lock(&t2smp_lock);
	retry = t2smp_restore_cpus();
	mutex_unlock(&t2smp_lock);

	if (retry && t2smp_restore_retries) {
		t2smp_restore_retries--;
		schedule_delayed_work(&t2smp_restore_work,
				      msecs_to_jiffies(20));
	} else if (retry) {
		pr_err("CPU hotplug remained busy; CPUs are still offline\n");
	}
}

static void t2smp_offline_cpus(void)
{
	unsigned int cpu;
	unsigned int offlined = 0;
	int ret;

	cancel_delayed_work_sync(&t2smp_restore_work);
	mutex_lock(&t2smp_lock);

	if (!cpumask_empty(t2smp_offlined_cpus)) {
		pr_err("CPUs from the previous suspend remain offline\n");
		t2smp_restore_cpus();
		if (!cpumask_empty(t2smp_offlined_cpus)) {
			pr_err("secondary CPU workaround skipped\n");
			goto out_unlock;
		}
	}

	for_each_present_cpu(cpu) {
		if (cpu == 0 || !cpu_online(cpu))
			continue;

		ret = remove_cpu(cpu);
		if (ret) {
			pr_err("failed to offline CPU%u: %d\n", cpu, ret);
			continue;
		}

		cpumask_set_cpu(cpu, t2smp_offlined_cpus);
		offlined++;
	}

	pr_info("took %u secondary CPUs offline\n", offlined);

out_unlock:
	mutex_unlock(&t2smp_lock);
}

static int t2smp_pm_notify(struct notifier_block *nb, unsigned long action,
			   void *unused)
{
	switch (action) {
	case PM_SUSPEND_PREPARE:
		t2smp_offline_cpus();
		break;
	case PM_POST_SUSPEND:
		t2smp_restore_retries = T2SMP_RESTORE_RETRIES;
		schedule_delayed_work(&t2smp_restore_work, 0);
		break;
	default:
		break;
	}

	return NOTIFY_OK;
}

static struct notifier_block t2smp_pm_notifier = {
	.notifier_call = t2smp_pm_notify,
	.priority = 1,
};

static int __init t2smp_init(void)
{
	struct pci_dev *t2;
	int ret;

	if (!x86_apple_machine)
		return -ENODEV;

	t2 = pci_get_device(PCI_VENDOR_ID_APPLE, PCI_DEVICE_ID_APPLE_T2_BRIDGE,
			    NULL);
	if (!t2)
		return -ENODEV;
	pci_dev_put(t2);

	if (!alloc_cpumask_var(&t2smp_offlined_cpus, GFP_KERNEL))
		return -ENOMEM;

	ret = register_pm_notifier(&t2smp_pm_notifier);
	if (ret) {
		free_cpumask_var(t2smp_offlined_cpus);
		return ret;
	}

	pr_info("initialized\n");
	return 0;
}

static void __exit t2smp_exit(void)
{
	unregister_pm_notifier(&t2smp_pm_notifier);
	cancel_delayed_work_sync(&t2smp_restore_work);
	mutex_lock(&t2smp_lock);
	t2smp_restore_cpus();
	mutex_unlock(&t2smp_lock);
	free_cpumask_var(t2smp_offlined_cpus);
}

module_init(t2smp_init);
module_exit(t2smp_exit);

MODULE_AUTHOR("Andre Eikmeyer <dev@deq.rocks>");
MODULE_DESCRIPTION("Apple T2 secondary CPU resume ordering quirk");
MODULE_LICENSE("GPL");
MODULE_VERSION("0.1");

MODULE_ALIAS("pci:v0000106Bd00001801sv*sd*bc*sc*i*");
