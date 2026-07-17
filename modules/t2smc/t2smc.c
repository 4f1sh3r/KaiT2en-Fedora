// SPDX-License-Identifier: GPL-2.0-only
/*
 * t2smc - Minimal SMC driver for T2 Macs
 *
 * Copyright (C) 2026 André Eikmeyer <andre.eikmeyer@gmail.com>
 */

#define pr_fmt(fmt) KBUILD_MODNAME ": " fmt

#define T2SMC_VERSION "0.0.1"

#include <linux/delay.h>
#include <linux/acpi.h>
#include <linux/kernel.h>
#include <linux/bitops.h>
#include <linux/slab.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/hwmon.h>
#include <linux/io.h>
#include <linux/err.h>
#include <linux/rtc.h>
#include <linux/power_supply.h>
#include <acpi/battery.h>

/* MMIO register offsets for T2 SMC interface */
#define T2SMC_IOMEM_KEY_DATA      0x0000
#define T2SMC_IOMEM_KEY_STATUS    0x4005
#define T2SMC_IOMEM_KEY_NAME      0x0078
#define T2SMC_IOMEM_KEY_DATA_LEN  0x007D
#define T2SMC_IOMEM_KEY_SMC_ID    0x007E
#define T2SMC_IOMEM_KEY_CMD       0x007F
#define T2SMC_IOMEM_MIN_SIZE      0x4006

/* Key type info in MMIO (after GET_KEY_TYPE_CMD) */
#define T2SMC_IOMEM_KEY_TYPE_CODE      0
#define T2SMC_IOMEM_KEY_TYPE_DATA_LEN  5
#define T2SMC_IOMEM_KEY_TYPE_FLAGS     6

#define T2SMC_MIN_WAIT          0x0008

/* SMC commands */
#define T2SMC_READ_CMD               0x10
#define T2SMC_WRITE_CMD              0x11
#define T2SMC_GET_KEY_BY_INDEX_CMD   0x12
#define T2SMC_GET_KEY_TYPE_CMD       0x13

/* Known keys */
#define KEY_COUNT_KEY   "#KEY"  /* r-o ui32 */
#define FANS_COUNT      "FNum"  /* r-o ui8  */
#define FANS_MANUAL     "FS! "  /* r-w ui16 (legacy) */
#define T2SMC_RTC_COUNTER  "CLKM"  /* r-o 48-bit 32768 Hz counter */
#define T2SMC_RTC_OFFSET   "CLKO"  /* r-w 48-bit offset */
#define T2SMC_CHARGE_LIMIT "BCLM"
#define T2SMC_CHARGE_LIMIT_SW  "CHLS"
#define T2SMC_CHARGE_LIMIT_80  "CHWA"
#define FLOAT_TYPE      "flt "
#define TEMP_SENSOR_TYPE "sp78"

#define T2SMC_RTC_BYTES      6
#define T2SMC_RTC_BITS       (8 * T2SMC_RTC_BYTES)
#define T2SMC_RTC_SEC_SHIFT  15
#define T2SMC_CHLS_START_OFFSET  5
#define T2SMC_CHWA_FIXED_LIMIT   80
#define T2SMC_CHWA_DISABLE_AT    95

/* Fan speed key formats */
static const char *const fan_speed_fmt[] = {
	"F%dAc",  /* actual speed      - idx 0 */
	"F%dMn",  /* minimum speed     - idx 1 */
	"F%dMx",  /* maximum speed     - idx 2 */
	"F%dSf",  /* safe speed        - idx 3 */
	"F%dTg",  /* target speed (rw) - idx 4 */
};
#define FAN_MANUAL_FMT  "F%dMd"  /* T2 per-fan manual mode key */

#define INIT_TIMEOUT_MSECS  5000
#define INIT_WAIT_MSECS     50

/* -- SMC entry cache entry -- */
struct t2smc_entry {
	char key[5];
	u8   valid;
	u8   len;
	char type[5];
	u8   flags;
};

/* -- Main device structure -- */
struct t2smc_device {
	struct acpi_device *adev;
	struct device *dev;

	/* MMIO */
	bool iomem_ok;
	void __iomem *iomem;
	u32 iomem_addr, iomem_size;

	/* Key cache */
	struct mutex mutex;
	unsigned int key_count;
	unsigned int fan_count;
	unsigned int temp_count;
	struct t2smc_entry *cache;
	char (*temp_keys)[5]; /* [temp_count] dynamically allocated */
	struct rtc_device *rtc_dev;
	bool has_chls;
	bool has_chwa;
	struct acpi_battery_hook batt_hook;
	struct power_supply *battery;
};

/* -- MMIO helpers -- */
static inline void iomem_clear_status(struct t2smc_device *t2)
{
	if (ioread8(t2->iomem + T2SMC_IOMEM_KEY_STATUS))
		iowrite8(0, t2->iomem + T2SMC_IOMEM_KEY_STATUS);
}

static int iomem_wait_read(struct t2smc_device *t2)
{
	u8 status;
	int us, i;

	us = T2SMC_MIN_WAIT;
	for (i = 0; i < 24; i++) {
		status = ioread8(t2->iomem + T2SMC_IOMEM_KEY_STATUS);
		if (status & 0x20)
			return 0;
		usleep_range(us, us * 2);
		if (i > 9)
			us <<= 1;
	}
	dev_warn(t2->dev, "%s: timeout\n", __func__);
	return -EIO;
}

/* -- MMIO SMC read/write -- */
static int iomem_read_smc(struct t2smc_device *t2,
			   u8 cmd, const char *key, u8 *buffer, u8 len)
{
	u8 err, remote_len;
	u32 key_int;

	memcpy(&key_int, key, sizeof(key_int));
	iomem_clear_status(t2);
	iowrite32(key_int, t2->iomem + T2SMC_IOMEM_KEY_NAME);
	iowrite8(0, t2->iomem + T2SMC_IOMEM_KEY_SMC_ID);
	iowrite8(cmd, t2->iomem + T2SMC_IOMEM_KEY_CMD);

	if (iomem_wait_read(t2))
		return -EIO;

	err = ioread8(t2->iomem + T2SMC_IOMEM_KEY_CMD);
	if (err != 0) {
		pr_debug("read_smc_mmio(%x %.4s) failed: %u\n",
			cmd, key, err);
		return -EIO;
	}

	if (cmd == T2SMC_READ_CMD) {
		remote_len = ioread8(t2->iomem + T2SMC_IOMEM_KEY_DATA_LEN);
		if (remote_len != len) {
			dev_warn(t2->dev,
				 "read_smc_mmio(%x %.4s): len mismatch (remote=%u, req=%u)\n",
				 cmd, key, remote_len, len);
			return -EINVAL;
		}
	} else {
		remote_len = len;
	}

	memcpy_fromio(buffer, t2->iomem + T2SMC_IOMEM_KEY_DATA, remote_len);
	return 0;
}

static int iomem_write_smc(struct t2smc_device *t2,
			    u8 cmd, const char *key, const u8 *buffer, u8 len)
{
	u8 err;
	u32 key_int;

	memcpy(&key_int, key, sizeof(key_int));
	iomem_clear_status(t2);
	iowrite32(key_int, t2->iomem + T2SMC_IOMEM_KEY_NAME);
	memcpy_toio(t2->iomem + T2SMC_IOMEM_KEY_DATA, buffer, len);
	iowrite8(len, t2->iomem + T2SMC_IOMEM_KEY_DATA_LEN);
	iowrite8(0, t2->iomem + T2SMC_IOMEM_KEY_SMC_ID);
	iowrite8(cmd, t2->iomem + T2SMC_IOMEM_KEY_CMD);

	if (iomem_wait_read(t2))
		return -EIO;

	err = ioread8(t2->iomem + T2SMC_IOMEM_KEY_CMD);
	if (err != 0) {
		pr_debug("write_smc_mmio(%x %.4s) failed: %u\n",
			cmd, key, err);
		return -EIO;
	}
	return 0;
}

static int iomem_get_key_info(struct t2smc_device *t2,
			       const char *key, struct t2smc_entry *info)
{
	u8 err;
	u32 key_int, type;

	memcpy(&key_int, key, sizeof(key_int));
	iomem_clear_status(t2);
	iowrite32(key_int, t2->iomem + T2SMC_IOMEM_KEY_NAME);
	iowrite8(0, t2->iomem + T2SMC_IOMEM_KEY_SMC_ID);
	iowrite8(T2SMC_GET_KEY_TYPE_CMD, t2->iomem + T2SMC_IOMEM_KEY_CMD);

	if (iomem_wait_read(t2))
		return -EIO;

	err = ioread8(t2->iomem + T2SMC_IOMEM_KEY_CMD);
	if (err != 0) {
		pr_debug("get_key_type_mmio(%.4s) failed: %u\n", key, err);
		return -EIO;
	}

	info->len   = ioread8(t2->iomem + T2SMC_IOMEM_KEY_TYPE_DATA_LEN);
	type = ioread32(t2->iomem + T2SMC_IOMEM_KEY_TYPE_CODE);
	memcpy(info->type, &type, sizeof(type));
	info->flags = ioread8(t2->iomem + T2SMC_IOMEM_KEY_TYPE_FLAGS);

	pr_debug("get_key_type_mmio(%.4s): len=%u type=%.4s flags=%x\n",
		key, info->len, info->type, info->flags);
	return 0;
}

/* -- High-level SMC access (mutex protected) -- */
static int read_smc(struct t2smc_device *t2, const char *key,
		     u8 *buffer, u8 len)
{
	return iomem_read_smc(t2, T2SMC_READ_CMD, key, buffer, len);
}

static int write_smc(struct t2smc_device *t2, const char *key,
		      const u8 *buffer, u8 len)
{
	return iomem_write_smc(t2, T2SMC_WRITE_CMD, key, buffer, len);
}

static int get_smc_key_by_index(struct t2smc_device *t2,
				 unsigned int index, char *key)
{
	__be32 be = cpu_to_be32(index);
	return iomem_read_smc(t2, T2SMC_GET_KEY_BY_INDEX_CMD,
			      (const char *)&be, (u8 *)key, 4);
}

/* -- Key cache -- */
static struct t2smc_entry *t2smc_get_entry_by_index(struct t2smc_device *t2,
						     int index)
{
	struct t2smc_entry *cache = &t2->cache[index];
	char key[4];
	int ret;

	if (cache->valid)
		return cache;

	mutex_lock(&t2->mutex);
	if (cache->valid)
		goto out;

	ret = get_smc_key_by_index(t2, index, key);
	if (ret)
		goto out;
	memcpy(cache->key, key, 4);

	ret = iomem_get_key_info(t2, key, cache);
	if (ret)
		goto out;
	cache->valid = true;

out:
	mutex_unlock(&t2->mutex);
	if (ret)
		return ERR_PTR(ret);
	return cache;
}

static int t2smc_get_lower_bound(struct t2smc_device *t2,
				  unsigned int *lo, const char *key)
{
	int begin = 0, end = t2->key_count;

	while (begin != end) {
		int middle = begin + (end - begin) / 2;
		struct t2smc_entry *entry = t2smc_get_entry_by_index(t2, middle);
		if (IS_ERR(entry)) {
			*lo = 0;
			return PTR_ERR(entry);
		}
		if (strcmp(entry->key, key) < 0)
			begin = middle + 1;
		else
			end = middle;
	}
	*lo = begin;
	return 0;
}

static int t2smc_get_upper_bound(struct t2smc_device *t2,
				  unsigned int *hi, const char *key)
{
	int begin = 0, end = t2->key_count;

	while (begin != end) {
		int middle = begin + (end - begin) / 2;
		struct t2smc_entry *entry = t2smc_get_entry_by_index(t2, middle);
		if (IS_ERR(entry)) {
			*hi = t2->key_count;
			return PTR_ERR(entry);
		}
		if (strcmp(key, entry->key) < 0)
			end = middle;
		else
			begin = middle + 1;
	}
	*hi = begin;
	return 0;
}

static struct t2smc_entry *t2smc_get_entry_by_key(struct t2smc_device *t2,
						    const char *key)
{
	int begin, end, ret;

	ret = t2smc_get_lower_bound(t2, &begin, key);
	if (ret)
		return ERR_PTR(ret);
	ret = t2smc_get_upper_bound(t2, &end, key);
	if (ret)
		return ERR_PTR(ret);
	if (end == begin)
		return ERR_PTR(-ENOENT);
	if (end - begin != 1)
		return ERR_PTR(-EUCLEAN);

	return t2smc_get_entry_by_index(t2, begin);
}

static int t2smc_read_key(struct t2smc_device *t2,
			   const char *key, u8 *buffer, u8 len)
{
	struct t2smc_entry *entry;
	int ret;

	entry = t2smc_get_entry_by_key(t2, key);
	if (IS_ERR(entry))
		return PTR_ERR(entry);

	if (entry->len != len)
		return -EINVAL;

	mutex_lock(&t2->mutex);
	ret = read_smc(t2, key, buffer, len);
	mutex_unlock(&t2->mutex);
	return ret;
}

static int t2smc_write_key(struct t2smc_device *t2,
			    const char *key, const u8 *buffer, u8 len)
{
	struct t2smc_entry *entry;
	int ret;

	entry = t2smc_get_entry_by_key(t2, key);
	if (IS_ERR(entry))
		return PTR_ERR(entry);

	if (entry->len != len)
		return -EINVAL;

	mutex_lock(&t2->mutex);
	ret = write_smc(t2, key, buffer, len);
	mutex_unlock(&t2->mutex);
	return ret;
}

static int t2smc_has_key(struct t2smc_device *t2,
			  const char *key, bool *present)
{
	struct t2smc_entry *entry;

	entry = t2smc_get_entry_by_key(t2, key);
	if (IS_ERR(entry)) {
		if (PTR_ERR(entry) == -ENOENT) {
			*present = false;
			return 0;
		}
		return PTR_ERR(entry);
	}
	*present = true;
	return 0;
}

/* -- T2 float conversion (fans use IEEE 754 "flt " type on T2) -- */
static inline u32 float_to_u32(u32 d)
{
	u8 sign = (u8)((d >> 31) & 1);
	s32 exp = (s32)((d >> 23) & 0xff) - 0x7f;
	u32 fr = d & ((1u << 23) - 1);

	if (sign || exp < 0)
		return 0;
	return (u32)((1u << exp) + (fr >> (23 - exp)));
}

static inline u32 u32_to_float(u32 d)
{
	u32 dc = d, bc = 0, exp;

	if (!d)
		return 0;
	while (dc >>= 1)
		++bc;
	exp = 0x7f + bc;
	return (u32)((exp << 23) |
		     ((d << (23 - (exp - 0x7f))) & ((1u << 23) - 1)));
}

static int t2smc_read_temp(struct t2smc_device *t2, const char *key, long *val)
{
	u8 buf[2];
	s16 raw;
	int ret;

	ret = t2smc_read_key(t2, key, buf, 2);
	if (ret)
		return ret;

	raw = (s16)(((u16)buf[0] << 8) | buf[1]);
	*val = (long)(raw >> 6) * 250;
	return 0;
}

/* -- Initialization -- */
static int t2smc_init_keycache(struct t2smc_device *t2)
{
	unsigned int count;
	__be32 be;
	u8 tmp[1];
	int ret;

	ret = read_smc(t2, KEY_COUNT_KEY, (u8 *)&be, 4);
	if (ret)
		return ret;
	count = be32_to_cpu(be);

	t2->cache = kcalloc(count, sizeof(*t2->cache), GFP_KERNEL);
	if (!t2->cache)
		return -ENOMEM;
	t2->key_count = count;

	/* Discover fan count */
	ret = t2smc_read_key(t2, FANS_COUNT, tmp, 1);
	if (ret) {
		kfree(t2->cache);
		t2->cache = NULL;
		t2->key_count = 0;
		return ret;
	}
	t2->fan_count = tmp[0];
	if (t2->fan_count > 10)
		t2->fan_count = 10;

	/* Discover temperature sensors (keys in T..U range, type sp78) */
	{
		unsigned int t, temp_begin, temp_end;

		ret = t2smc_get_lower_bound(t2, &temp_begin, "T");
		if (ret) {
			kfree(t2->cache);
			t2->cache = NULL;
			t2->key_count = 0;
			return ret;
		}
		ret = t2smc_get_lower_bound(t2, &temp_end, "U");
		if (ret) {
			kfree(t2->cache);
			t2->cache = NULL;
			t2->key_count = 0;
			return ret;
		}

		t2->temp_count = 0;
		for (t = temp_begin; t < temp_end; t++) {
			struct t2smc_entry *entry;

			entry = t2smc_get_entry_by_index(t2, t);
			if (IS_ERR(entry))
				continue;
			if (strcmp(entry->type, TEMP_SENSOR_TYPE))
				continue;
			t2->temp_count++;
		}

		if (t2->temp_count) {
			unsigned int idx = 0;

			t2->temp_keys = kcalloc(t2->temp_count,
						sizeof(t2->temp_keys[0]),
						GFP_KERNEL);
			if (!t2->temp_keys) {
				/* Non-fatal: just skip temperatures */
				t2->temp_count = 0;
			} else {
				for (t = temp_begin; t < temp_end; t++) {
					struct t2smc_entry *entry;

					entry = t2smc_get_entry_by_index(t2, t);
					if (IS_ERR(entry))
						continue;
					if (strcmp(entry->type, TEMP_SENSOR_TYPE))
						continue;
					memcpy(t2->temp_keys[idx],
					       entry->key, 4);
					t2->temp_keys[idx][4] = '\0';
					idx++;
				}
			}
		}
	}

	ret = t2smc_has_key(t2, T2SMC_CHARGE_LIMIT_SW, &t2->has_chls);
	if (ret)
		return ret;
	ret = t2smc_has_key(t2, T2SMC_CHARGE_LIMIT_80, &t2->has_chwa);
	if (ret)
		return ret;

	dev_info(t2->dev, "initialized: keys=%u fans=%u temps=%u\n",
		 t2->key_count, t2->fan_count, t2->temp_count);
	dev_info(t2->dev,
		 "charge keys: CHLS=%d CHWA=%d\n", t2->has_chls,
		 t2->has_chwa);
	return 0;
}

static int t2smc_try_enable_iomem(struct t2smc_device *t2)
{
	u8 test_val, ldkn_version;

	pr_debug("Trying to enable MMIO communication\n");
	t2->iomem = ioremap(t2->iomem_addr, t2->iomem_size);
	if (!t2->iomem)
		goto out;

	test_val = ioread8(t2->iomem + T2SMC_IOMEM_KEY_STATUS);
	if (test_val == 0xff) {
		dev_warn(t2->dev, "iomem init failed: status=0xff (is %x)\n",
			 test_val);
		goto out_unmap;
	}

	/* Verify communication works by reading LDKN key */
	if (iomem_read_smc(t2, T2SMC_READ_CMD, "LDKN", &ldkn_version, 1)) {
		dev_warn(t2->dev, "iomem init failed: LDKN read failed\n");
		goto out_unmap;
	}
	if (ldkn_version < 2) {
		dev_warn(t2->dev, "iomem init failed: LDKN version %u < 2\n",
			 ldkn_version);
		goto out_unmap;
	}

	dev_info(t2->dev, "MMIO interface enabled (LDKN v%u)\n", ldkn_version);
	t2->iomem_ok = true;
	return 0;

out_unmap:
	iounmap(t2->iomem);
	t2->iomem = NULL;
out:
	return -ENXIO;
}

/* -- ACPI resource walk -- */
static acpi_status t2smc_walk_resources(struct acpi_resource *res, void *data)
{
	struct t2smc_device *t2 = data;

	switch (res->type) {
	case ACPI_RESOURCE_TYPE_FIXED_MEMORY32:
		if (!t2->iomem_ok) {
			if (res->data.fixed_memory32.address_length <
			    T2SMC_IOMEM_MIN_SIZE) {
				dev_warn(t2->dev,
					 "iomem too small: %u\n",
					 res->data.fixed_memory32.address_length);
				return AE_OK;
			}
			t2->iomem_addr = res->data.fixed_memory32.address;
			t2->iomem_size = res->data.fixed_memory32.address_length;
		}
		return AE_OK;

	case ACPI_RESOURCE_TYPE_END_TAG:
		if (t2->iomem_addr)
			return AE_OK;
		return AE_NOT_FOUND;

	default:
		return AE_OK;
	}
}

/* -- Fan speed r/w -- */
static int t2smc_read_fan(struct t2smc_device *t2, int fan_idx, int option,
			   unsigned int *speed)
{
	struct t2smc_entry *entry;
	char key[5];
	u8 buffer[4];
	int ret;

	scnprintf(key, sizeof(key), fan_speed_fmt[option], fan_idx);
	entry = t2smc_get_entry_by_key(t2, key);
	if (IS_ERR(entry))
		return PTR_ERR(entry);
	if (!strcmp(entry->type, FLOAT_TYPE)) {
		u32 raw;

		ret = t2smc_read_key(t2, key, (u8 *)&raw, 4);
		if (ret)
			return ret;
		*speed = float_to_u32(raw);
	} else {
		ret = t2smc_read_key(t2, key, buffer, 2);
		if (ret)
			return ret;
		*speed = ((buffer[0] << 8 | buffer[1]) >> 2);
	}
	return 0;
}

static int t2smc_write_fan(struct t2smc_device *t2, int fan_idx, int option,
			    unsigned int speed)
{
	struct t2smc_entry *entry;
	char key[5];
	u8 buffer[4];

	scnprintf(key, sizeof(key), fan_speed_fmt[option], fan_idx);
	entry = t2smc_get_entry_by_key(t2, key);
	if (IS_ERR(entry))
		return PTR_ERR(entry);

	if (!strcmp(entry->type, FLOAT_TYPE)) {
		u32 fval = u32_to_float(speed);

		memcpy(buffer, &fval, sizeof(fval));
		return t2smc_write_key(t2, key, buffer, 4);
	} else {
		buffer[0] = (speed >> 6) & 0xff;
		buffer[1] = (speed << 2) & 0xff;
		return t2smc_write_key(t2, key, buffer, 2);
	}
}

static int t2smc_write_fan_manual(struct t2smc_device *t2, int fan_idx,
				   unsigned int manual)
{
	char key[5];
	bool has_fmd;
	u8 buf[2];
	int ret;

	scnprintf(key, sizeof(key), FAN_MANUAL_FMT, fan_idx);
	ret = t2smc_has_key(t2, key, &has_fmd);
	if (ret)
		return ret;

	if (has_fmd) {
		buf[0] = manual ? 1 : 0;
		return t2smc_write_key(t2, key, buf, 1);
	} else {
		unsigned int val;
		ret = t2smc_read_key(t2, FANS_MANUAL, buf, 2);
		if (ret)
			return ret;
		val = (buf[0] << 8 | buf[1]);
		if (manual)
			val |= (0x01 << fan_idx);
		else
			val &= ~(0x01 << fan_idx);
		buf[0] = (val >> 8) & 0xff;
		buf[1] = val & 0xff;
		return t2smc_write_key(t2, FANS_MANUAL, buf, 2);
	}
}

/* -- hwmon interface -- */
#define T2SMC_FAN_OPT_ACTUAL  0
#define T2SMC_FAN_OPT_MIN     1
#define T2SMC_FAN_OPT_MAX     2
#define T2SMC_FAN_OPT_SAFE    3
#define T2SMC_FAN_OPT_TARGET  4

static int t2smc_hwmon_read(struct device *dev, enum hwmon_sensor_types type,
			     u32 attr, int channel, long *val)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	unsigned int speed;
	int ret;

	switch (type) {
	case hwmon_temp:
		if (attr != hwmon_temp_input)
			return -EOPNOTSUPP;
		if (channel >= t2->temp_count)
			return -EINVAL;
		return t2smc_read_temp(t2, t2->temp_keys[channel], val);

	case hwmon_fan:
		switch (attr) {
		case hwmon_fan_input:
			ret = t2smc_read_fan(t2, channel, T2SMC_FAN_OPT_ACTUAL, &speed);
			break;
		case hwmon_fan_min:
			ret = t2smc_read_fan(t2, channel, T2SMC_FAN_OPT_MIN, &speed);
			break;
		case hwmon_fan_max:
			ret = t2smc_read_fan(t2, channel, T2SMC_FAN_OPT_MAX, &speed);
			break;
		case hwmon_fan_target:
			ret = t2smc_read_fan(t2, channel, T2SMC_FAN_OPT_TARGET, &speed);
			break;
		default:
			return -EOPNOTSUPP;
		}
		if (ret)
			return ret;
		*val = (long)speed;
		return 0;

	default:
		return -EOPNOTSUPP;
	}
}

static int t2smc_hwmon_write(struct device *dev, enum hwmon_sensor_types type,
			      u32 attr, int channel, long val)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	unsigned int speed;
	int ret;

	switch (type) {
	case hwmon_fan:
		switch (attr) {
		case hwmon_fan_min:
			if (val < 0)
				return -EINVAL;
			speed = (unsigned int)val;
			return t2smc_write_fan(t2, channel, T2SMC_FAN_OPT_MIN, speed);
		case hwmon_fan_target:
			if (val < 0)
				return -EINVAL;
			speed = (unsigned int)val;
			/* Enter manual mode before setting target speed */
			ret = t2smc_write_fan_manual(t2, channel, 1);
			if (ret)
				return ret;
			return t2smc_write_fan(t2, channel, T2SMC_FAN_OPT_TARGET, speed);
		default:
			return -EOPNOTSUPP;
		}

	default:
		return -EOPNOTSUPP;
	}
}

static umode_t t2smc_hwmon_is_visible(const void *drvdata,
				       enum hwmon_sensor_types type,
				       u32 attr, int channel)
{
	switch (type) {
	case hwmon_temp:
		return 0444;
	case hwmon_fan:
		switch (attr) {
		case hwmon_fan_min:
		case hwmon_fan_target:
			return 0644;
		default:
			return 0444;
		}
	default:
		return 0;
	}
}


static int t2smc_hwmon_read_string(struct device *dev,
				   enum hwmon_sensor_types type, u32 attr,
				   int channel, const char **str)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);

	if (type == hwmon_temp && attr == hwmon_temp_label) {
		if (channel >= t2->temp_count)
			return -EINVAL;
		*str = t2->temp_keys[channel];
		return 0;
	}
	return -EOPNOTSUPP;
}


static const struct hwmon_ops t2smc_hwmon_ops = {
	.is_visible = t2smc_hwmon_is_visible,
	.read       = t2smc_hwmon_read,
	.write      = t2smc_hwmon_write,
	.read_string = t2smc_hwmon_read_string,
};

/* -- Battery charge limit as extra hwmon attribute group -- */
static int t2smc_write_charge_limit_method(struct t2smc_device *t2, u8 val)
{
	u8 buf[2] = { 0, 0 };
	u8 flag;

	if (t2->has_chls) {
		if (val > 0 && val < 100)
			buf[0] = val + T2SMC_CHLS_START_OFFSET;

		return t2smc_write_key(t2, T2SMC_CHARGE_LIMIT_SW, buf, 2);
	}

	if (t2->has_chwa) {
		flag = val < T2SMC_CHWA_DISABLE_AT ? 1 : 0;
		if (val != T2SMC_CHWA_FIXED_LIMIT && flag)
			dev_info(t2->dev,
				 "CHWA only supports a fixed %u%% charge limit\n",
				 T2SMC_CHWA_FIXED_LIMIT);

		return t2smc_write_key(t2, T2SMC_CHARGE_LIMIT_80, &flag, 1);
	}

	return 0;
}

static int t2smc_get_charge_limit(struct t2smc_device *t2, u8 *val)
{
	if (t2smc_read_key(t2, T2SMC_CHARGE_LIMIT, val, 1))
		return -ENODEV;
	return 0;
}

static int t2smc_set_charge_limit(struct t2smc_device *t2, u8 val)
{
	if (val > 100)
		return -EINVAL;
	if (t2smc_write_key(t2, T2SMC_CHARGE_LIMIT, &val, 1))
		return -ENODEV;
	if (t2smc_write_charge_limit_method(t2, val))
		return -ENODEV;
	if (t2->battery)
		power_supply_changed(t2->battery);
	return 0;
}

static ssize_t charge_limit_show(struct device *dev,
				  struct device_attribute *attr, char *buf)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	u8 val;

	if (t2smc_get_charge_limit(t2, &val))
		return -ENODEV;
	return sysfs_emit(buf, "%d\n", val);
}

static ssize_t charge_limit_store(struct device *dev,
					   struct device_attribute *attr,
					   const char *buf, size_t count)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	u8 val;
	int ret;

	if (kstrtou8(buf, 10, &val) < 0)
		return -EINVAL;

	ret = t2smc_set_charge_limit(t2, val);
	if (ret)
		return ret;
	return count;
}

static DEVICE_ATTR(battery_charge_limit, 0644,
		   charge_limit_show, charge_limit_store);

static struct attribute *t2smc_bclm_attrs[] = {
	&dev_attr_battery_charge_limit.attr,
	NULL,
};

static const struct attribute_group t2smc_bclm_group = {
	.attrs = t2smc_bclm_attrs,
};

static const struct attribute_group *t2smc_extra_groups[] = {
	&t2smc_bclm_group,
	NULL,
};

/* -- Same charge limit, exposed as a standard power_supply extension on BAT0 -- */
static int t2smc_psy_ext_get(struct power_supply *psy,
			      const struct power_supply_ext *ext,
			      void *data, enum power_supply_property psp,
			      union power_supply_propval *val)
{
	struct t2smc_device *t2 = data;
	u8 limit;

	switch (psp) {
	case POWER_SUPPLY_PROP_CHARGE_CONTROL_START_THRESHOLD:
		val->intval = 0;
		return 0;
	case POWER_SUPPLY_PROP_CHARGE_CONTROL_END_THRESHOLD:
		if (t2smc_get_charge_limit(t2, &limit))
			return -ENODEV;
		val->intval = limit;
		return 0;
	default:
		return -EINVAL;
	}
}

static int t2smc_psy_ext_set(struct power_supply *psy,
			      const struct power_supply_ext *ext,
			      void *data, enum power_supply_property psp,
			      const union power_supply_propval *val)
{
	struct t2smc_device *t2 = data;

	if (psp != POWER_SUPPLY_PROP_CHARGE_CONTROL_END_THRESHOLD)
		return -EINVAL;
	if (val->intval < 0 || val->intval > 100)
		return -EINVAL;
	return t2smc_set_charge_limit(t2, val->intval);
}

static int t2smc_psy_ext_is_writeable(struct power_supply *psy,
				       const struct power_supply_ext *ext,
				       void *data, enum power_supply_property psp)
{
	return psp == POWER_SUPPLY_PROP_CHARGE_CONTROL_END_THRESHOLD;
}

static enum power_supply_property t2smc_psy_ext_props[] = {
	POWER_SUPPLY_PROP_CHARGE_CONTROL_START_THRESHOLD,
	POWER_SUPPLY_PROP_CHARGE_CONTROL_END_THRESHOLD,
};

static const struct power_supply_ext t2smc_psy_ext = {
	.name                   = "t2smc-charge-control",
	.properties             = t2smc_psy_ext_props,
	.num_properties         = ARRAY_SIZE(t2smc_psy_ext_props),
	.get_property           = t2smc_psy_ext_get,
	.set_property           = t2smc_psy_ext_set,
	.property_is_writeable  = t2smc_psy_ext_is_writeable,
};

static int t2smc_battery_add(struct power_supply *battery,
			      struct acpi_battery_hook *hook)
{
	struct t2smc_device *t2 = container_of(hook, struct t2smc_device, batt_hook);
	int ret;

	if (strcmp(battery->desc->name, "BAT0"))
		return -ENODEV;

	ret = power_supply_register_extension(battery, &t2smc_psy_ext, t2->dev, t2);
	if (ret)
		return ret;

	t2->battery = battery;
	return 0;
}

static int t2smc_battery_remove(struct power_supply *battery,
				 struct acpi_battery_hook *hook)
{
	struct t2smc_device *t2 = container_of(hook, struct t2smc_device, batt_hook);

	t2->battery = NULL;
	power_supply_unregister_extension(battery, &t2smc_psy_ext);
	return 0;
}

/* -- RTC (48-bit 32768 Hz counter + offset) -- */
static int t2smc_read_rtc_key(struct t2smc_device *t2, const char *key, u64 *val)
{
	u8 buf[T2SMC_RTC_BYTES];
	int ret;

	ret = t2smc_read_key(t2, key, buf, T2SMC_RTC_BYTES);
	if (ret)
		return ret;

	*val = 0;
	memcpy(val, buf, T2SMC_RTC_BYTES);
	return 0;
}

static int t2smc_write_rtc_key(struct t2smc_device *t2, const char *key, u64 val)
{
	u8 buf[T2SMC_RTC_BYTES];

	memcpy(buf, &val, T2SMC_RTC_BYTES);
	return t2smc_write_key(t2, key, buf, T2SMC_RTC_BYTES);
}

static int t2smc_rtc_read_time(struct device *dev, struct rtc_time *tm)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	u64 ctr, off;
	time64_t now;
	int ret;

	ret = t2smc_read_rtc_key(t2, T2SMC_RTC_COUNTER, &ctr);
	if (ret)
		return ret;
	ret = t2smc_read_rtc_key(t2, T2SMC_RTC_OFFSET, &off);
	if (ret)
		return ret;

	now = sign_extend64(ctr + off, T2SMC_RTC_BITS - 1) >> T2SMC_RTC_SEC_SHIFT;
	rtc_time64_to_tm(now, tm);
	return 0;
}

static int t2smc_rtc_set_time(struct device *dev, struct rtc_time *tm)
{
	struct t2smc_device *t2 = dev_get_drvdata(dev);
	u64 ctr, off;
	int ret;

	ret = t2smc_read_rtc_key(t2, T2SMC_RTC_COUNTER, &ctr);
	if (ret)
		return ret;

	off = ((u64)rtc_tm_to_time64(tm) << T2SMC_RTC_SEC_SHIFT) - ctr;
	return t2smc_write_rtc_key(t2, T2SMC_RTC_OFFSET, off);
}

static const struct rtc_class_ops t2smc_rtc_ops = {
	.read_time = t2smc_rtc_read_time,
	.set_time = t2smc_rtc_set_time,
};

static int t2smc_register_rtc(struct t2smc_device *t2)
{
	struct device *dev = &t2->adev->dev;
	bool has_counter, has_offset;
	int ret;

	ret = t2smc_has_key(t2, T2SMC_RTC_COUNTER, &has_counter);
	if (ret)
		return ret;
	ret = t2smc_has_key(t2, T2SMC_RTC_OFFSET, &has_offset);
	if (ret)
		return ret;

	if (!has_counter || !has_offset) {
		dev_info(t2->dev, "RTC keys not present, skipping RTC\n");
		return 0;
	}

	t2->rtc_dev = devm_rtc_allocate_device(dev);
	if (IS_ERR(t2->rtc_dev))
		return PTR_ERR(t2->rtc_dev);

	t2->rtc_dev->ops = &t2smc_rtc_ops;
	t2->rtc_dev->range_min =
		S64_MIN >> (T2SMC_RTC_SEC_SHIFT + (64 - T2SMC_RTC_BITS));
	t2->rtc_dev->range_max =
		S64_MAX >> (T2SMC_RTC_SEC_SHIFT + (64 - T2SMC_RTC_BITS));

	ret = devm_rtc_register_device(t2->rtc_dev);
	if (ret)
		return ret;

	dev_info(t2->dev, "RTC registered\n");
	return 0;
}

#define MAX_FANS 10

/* Register hwmon device with fan, temp channels and BCLM extra group */
static int t2smc_register_hwmon(struct t2smc_device *t2)
{
	struct device *dev = &t2->adev->dev;
	struct device *hwmon_dev;
	struct hwmon_channel_info *fan_info;
	struct hwmon_chip_info *chip_info;
	u32 *fan_config;
	int i;

	fan_config = devm_kcalloc(dev, t2->fan_count + 1, sizeof(u32), GFP_KERNEL);
	fan_info  = devm_kzalloc(dev, sizeof(*fan_info), GFP_KERNEL);
	chip_info = devm_kzalloc(dev, sizeof(*chip_info), GFP_KERNEL);
	if (!fan_config || !fan_info || !chip_info)
		return -ENOMEM;

	for (i = 0; i < t2->fan_count; i++)
		fan_config[i] = HWMON_F_INPUT | HWMON_F_MIN |
				HWMON_F_MAX | HWMON_F_TARGET;

	fan_info->type   = hwmon_fan;
	fan_info->config = fan_config;

	/* Build info array: fan + optional temp, terminated by NULL */
	{
		struct hwmon_channel_info *temp_info = NULL;
		const struct hwmon_channel_info **info;
		u32 *temp_config;
		int nchans = 2; /* fan + sentinel */
		int idx = 0;

		if (t2->temp_count) {
			nchans++;
			temp_config = devm_kcalloc(dev, t2->temp_count + 1,
						   sizeof(u32), GFP_KERNEL);
			temp_info   = devm_kzalloc(dev, sizeof(*temp_info),
						   GFP_KERNEL);
			if (!temp_config || !temp_info)
				return -ENOMEM;
			for (i = 0; i < t2->temp_count; i++)
				temp_config[i] = HWMON_T_INPUT | HWMON_T_LABEL;
			temp_info->type   = hwmon_temp;
			temp_info->config = temp_config;
		}

		info = devm_kcalloc(dev, nchans, sizeof(*info), GFP_KERNEL);
		if (!info)
			return -ENOMEM;
		info[idx++] = fan_info;
		if (temp_info)
			info[idx++] = temp_info;
		info[idx] = NULL;
		chip_info->info = info;
	}

	chip_info->ops = &t2smc_hwmon_ops;

	hwmon_dev = devm_hwmon_device_register_with_info(dev, "t2smc", t2,
							  chip_info,
							  t2smc_extra_groups);
	if (IS_ERR(hwmon_dev))
		return PTR_ERR(hwmon_dev);

	return 0;
}

/* devm action: cleanup non-devm resources after hwmon devres */
static void t2smc_devm_cleanup(void *data)
{
	struct t2smc_device *t2 = data;

	if (t2->iomem)
		iounmap(t2->iomem);
	mutex_destroy(&t2->mutex);
	kfree(t2->cache);
	kfree(t2->temp_keys);
}

/* -- ACPI driver callbacks -- */
static int t2smc_add(struct acpi_device *adev)
{
	struct t2smc_device *t2;
	int ret;

	t2 = devm_kzalloc(&adev->dev, sizeof(*t2), GFP_KERNEL);
	if (!t2)
		return -ENOMEM;

	t2->adev = adev;
	t2->dev = &adev->dev;
	mutex_init(&t2->mutex);
	dev_set_drvdata(&adev->dev, t2);

	/*
	 * Register cleanup action before anything that can fail.
	 * devres runs in reverse order, so this runs AFTER hwmon devres,
	 * ensuring hwmon callbacks never see freed t2.
	 */
	ret = devm_add_action_or_reset(&adev->dev, t2smc_devm_cleanup, t2);
	if (ret)
		return ret;

	/* Walk ACPI _CRS to find MMIO region */
	ret = acpi_walk_resources(adev->handle, METHOD_NAME__CRS,
				  t2smc_walk_resources, t2);
	if (ACPI_FAILURE(ret) || !t2->iomem_addr) {
		dev_err(t2->dev, "No suitable MMIO resource found\n");
		ret = -ENXIO;
		return ret;
	}

	ret = t2smc_try_enable_iomem(t2);
	if (ret)
		return ret;

	/* Retry key cache init with timeout */
	{
		int ms;
		for (ms = 0; ms < INIT_TIMEOUT_MSECS; ms += INIT_WAIT_MSECS) {
			/* Free old cache from previous failed attempt */
			kfree(t2->cache);
			t2->cache = NULL;
			kfree(t2->temp_keys);
			t2->temp_keys = NULL;
			t2->key_count = 0;

			ret = t2smc_init_keycache(t2);
			if (!ret) {
				if (ms)
					dev_info(t2->dev,
						 "keycache init took %d ms\n", ms);
				break;
			}
			if (ret == -EUCLEAN)
				break;
			msleep(INIT_WAIT_MSECS);
		}
		if (ret) {
			dev_err(t2->dev, "Failed to init key cache: %d\n", ret);
			return ret;
		}
	}

	ret = t2smc_register_hwmon(t2);
	if (ret)
		return ret;

	if (t2->has_chls || t2->has_chwa) {
		t2->batt_hook.name = "t2smc";
		t2->batt_hook.add_battery = t2smc_battery_add;
		t2->batt_hook.remove_battery = t2smc_battery_remove;
		ret = devm_battery_hook_register(t2->dev, &t2->batt_hook);
		if (ret)
			return ret;
	}

	ret = t2smc_register_rtc(t2);
	if (ret)
		return ret;

	dev_info(t2->dev, "t2smc %s ready (fans=%u)\n",
		 T2SMC_VERSION, t2->fan_count);
	return 0;
}

static void t2smc_remove(struct acpi_device *adev)
{
	/* All resources (iomem, cache, mutex, hwmon, rtc) are devm-managed */
}

static const struct acpi_device_id t2smc_ids[] = {
	{ "APP0001", 0 },
	{ "smc-huronriver", 0 },
	{ "", 0 },
};
MODULE_DEVICE_TABLE(acpi, t2smc_ids);

static struct acpi_driver t2smc_driver = {
	.name  = "t2smc",
	.ids   = t2smc_ids,
	.ops   = {
		.add    = t2smc_add,
		.remove = t2smc_remove,
	},
};

module_acpi_driver(t2smc_driver);

MODULE_AUTHOR("André Eikmeyer <andre.eikmeyer@gmail.com>");
MODULE_DESCRIPTION("T2 Mac SMC driver");
MODULE_LICENSE("GPL");
MODULE_VERSION(T2SMC_VERSION);
MODULE_ALIAS("applesmc");
