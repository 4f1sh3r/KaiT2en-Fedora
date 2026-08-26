---
draft: true
title: find pkg c-state blockers on your macbook running Linux
date: 2026-08-16
author: Andre Eikmeyer
summary: >-
  We explore the why deep pkg c-states are blocked
tags: [thunderbolt, power, c-states, guide]
---

As long this post is draft I will write down the commands in sequence
as a base structure and later write the story around them.

<figure>
  <img src="" alt="">
</figure>

## Debugfs

root access

```sh
sudo -i
```

mount debug fs

```sh
sudo mount -t debugfs none /sys/kernel/debug
```

## Checking pkg c-state residency

show the time the package has spent in which state in nanoseconds

```sh
cat /sys/kernel/debug/pmc_core/package_cstate_show
```

Package C-State Residency output:

```txt
Package C2 : 262676478
Package C3 : 342752985
Package C6 : 0
Package C7 : 0
Package C8 : 0
Package C9 : 0
Package C10 : 0
```

- PkgC2 / PkgC3: Light sleep states. The CPU cores are idle, but the system
  agent, L3 cache, and PCIe connections remain largely active.
- PkgC6 / PkgC7: Medium sleep states. Power supply to the individual cores is
  completely cut off, and caches are flushed or reduced.
- PkgC8 / PkgC9 / PkgC10: Very deep sleep states (particularly relevant for
  notebooks/SoCs). Here, the voltage for the display engine, the system agent
  unit, and large parts of the uncore area is drastically reduced or
  completely cut off.

## Checking LTR (Latency Tolerance Reporting)

LTR is a PCIe and PCH mechanism. Peripheral devices (e.g., NVMe, Wi-Fi, USB
controllers) inform the PMC of the amount of delay (latency) they can tolerate
when waking up. If a device requires extremely low latency, the PMC refuses to
transition to deep package C-states or SLP_S0 (Modern Standby / S0ix) in order
to honor that requirement.

```sh
ls /sys/kernel/debug/pmc_core/
```

ltr_show

Displays the current LTR requirements for all PCH IP blocks and PCIe lanes (in
microseconds). High values ​​allow deep C-states; very low values ​​(or an unset
value) prevent them.

ltr_ignore

Allows for overriding the mechanism. Writing the index of a problematic IP block
to this file (e.g., `echo 0 > ltr_ignore`) causes the PMC to ignore that block's
latency requirement. This makes it possible to test which device is preventing
power-saving modes.

ltr_restore

Restores LTR requirements that were ignored via `ltr_ignore` to their normal state.

```sh
cat /sys/kernel/debug/pmc_core/ltr_show
```

output

```txt
0	PMC0:SOUTHPORT_A            LTR: RAW: 0x883c883c        	Non-Snoop(ns): 61440           	Snoop(ns): 61440           	LTR_IGNORE: 0
1	PMC0:SOUTHPORT_B            LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
2	PMC0:SATA                   LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
3	PMC0:GIGABIT_ETHERNET       LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
4	PMC0:XHCI                   LTR: RAW: 0x13ff            	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
5	PMC0:Reserved               LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
6	PMC0:ME                     LTR: RAW: 0x8000800         	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
7	PMC0:EVA                    LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
8	PMC0:SOUTHPORT_C            LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
9	PMC0:HD_AUDIO               LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
10	PMC0:CNV                  LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
11	PMC0:LPSS                 LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
12	PMC0:SOUTHPORT_D          LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
13	PMC0:SOUTHPORT_E          LTR: RAW: 0x90029002        	Non-Snoop(ns): 2097152         	Snoop(ns): 2097152         	LTR_IGNORE: 0
14	PMC0:CAMERA               LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
15	PMC0:ESPI                 LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
16	PMC0:SCC                  LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
17	PMC0:ISH                  LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
18	PMC0:UFSX2                LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
19	PMC0:EMMC                 LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
20	PMC0:WIGIG                LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
21	PMC0:THC0                 LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
22	PMC0:THC1                 LTR: RAW: 0x0               	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
23	PMC0:CURRENT_PLATFORM     LTR: RAW: 0x40201           	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
24	PMC0:AGGREGATED_SYSTEM    LTR: RAW: 0x7c3e1f          	Non-Snoop(ns): 0               	Snoop(ns): 0               	LTR_IGNORE: 0
```

RAW values are 16bit words. Break down:

```txt
0x8  0x8  0x3  0xc  0x8  0x8  0x3  0xc
1000 1000 0011 1100 1000 1000 0011 1100
```

LTR is standardized according to the [PCI-SIG ECN spec.](https://pcisig.com/specifications)

Bit numbering is from right(LSB = bit 0) to left (MSB = bit 15)

```txt
          1  0  0  0  1  0  0  0  0  0  1  1  1  1  0  0
Bit-No.: 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0

Bit15 (req)  = 1
Bit14-13     = 00
Bit12-10     = 010  → scale 2 → x1024
Bit9-0       = 0000111100 = 60
60 × 1024 = 61440 ns
```

So the req bit is enabled. Let's try to ignore it and see if we
can get deeper pkg c-state residency:

```sh
# ignore SOUTHPORT_A where 0 is the index of the device
echo 0 > /sys/kernel/debug/pmc_core/ltr_ignore
```

Then make sure we see `LTR_IGNORE: 1` in the table
```sh
cat /sys/kernel/debug/pmc_core/ltr_show
```

And check if pkg c-state is now deeper than pc3

```sh
sudo cat /sys/kernel/debug/pmc_core/package_cstate_show
```

### Find Snoopy

```sh
sudo lspci -vvv | awk '
  /^[0-9a-f]{2}:/ { dev = $0; printed = 0; in_ltr = 0 }
  /Latency Tolerance Reporting/ { in_ltr = 1; next }
  in_ltr && /Max (no )?snoop latency/ {
    if (!printed) { print dev; printed = 1 }
    print "  " $0
  }
  in_ltr && /^[ \t]*$/ { in_ltr = 0 }
'
```

The script filters for the line "Latency Tolerance Reporting." `lspci` only
generates this line if the device possesses the PCIe Extended Capability for LTR
(ID 0x0018) at the hardware level. Devices lacking LTR support do not appear in
the output at all.

Significance of 0ns: The content of the LTR Control Register is read. A value of
0ns indicates that while the LTR function is enabled on the PCIe port, neither
the driver nor the firmware (NVRAM) has entered a specific value; the register
remains in its default reset state of 0.

The Power Management Controller (PMC) interprets this not as "missing," but as
"the device requires a 0-nanosecond delay for memory access." This immediately
blocks deep package C-states (PC8/PC10).

```txt
01:00.0 VGA compatible controller: Advanced Micro Devices, Inc. [AMD/ATI] Baffin [Radeon RX 460/560D / Pro 450/455/460/555/555X/560/560X] (rev e3) (prog-if 00 [VGA controller])
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
02:00.0 Mass storage controller: Apple Inc. ANS2 NVMe Controller (rev 01) (prog-if 02)
  		Max snoop latency: 3145728ns
  		Max no snoop latency: 3145728ns
03:00.0 Network controller: Broadcom Inc. and subsidiaries BCM4364 802.11ac Wireless Network Adapter (rev 03)
  		Max snoop latency: 0ns
  		Max no snoop latency: 0ns
04:00.0 PCI bridge: Intel Corporation DSL6540 Thunderbolt 3 Bridge [Alpine Ridge 4C 2015] (rev 06) (prog-if 00 [Normal decode])
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
06:00.0 System peripheral: Intel Corporation JHL7540 Thunderbolt 3 NHI [Titan Ridge 4C 2018] (rev 06)
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
07:00.0 USB controller: Intel Corporation JHL7540 Thunderbolt 3 USB Controller [Titan Ridge 4C 2018] (rev 06) (prog-if 30 [XHCI])
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
7a:00.0 PCI bridge: Intel Corporation DSL6540 Thunderbolt 3 Bridge [Alpine Ridge 4C 2015] (rev 06) (prog-if 00 [Normal decode])
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
7c:00.0 System peripheral: Intel Corporation JHL7540 Thunderbolt 3 NHI [Titan Ridge 4C 2018] (rev 06)
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
7d:00.0 USB controller: Intel Corporation JHL7540 Thunderbolt 3 USB Controller [Titan Ridge 4C 2018] (rev 06) (prog-if 30 [XHCI])
  		Max snoop latency: 71680ns
  		Max no snoop latency: 71680ns
```

Broadcom BCM4364 (03:00.0): The brcmfmac driver fails to initialize LTR
correctly, resulting in a value of 0ns in the register. The Power Management
Controller (PMC) interprets 0ns as "zero latency allowed" and blocks all package
C-states deeper than PC3.

First let's check that LTR is activated on the root port and what it demands

```sh
sudo lspci -s $(lspci -PP -s 03:00.0 | cut -d/ -f1) -vvv | awk '/DevCap2/,/LTR1.2_Threshold/'
``` 

This is what we need from the output

```sh
DevCap2: Completion Timeout: Range ABC, TimeoutDis+ NROPrPrP- LTR+
		DOReq- IDOCompl- LTR+ EmergencyPowerReductionReq-
			   T_CommonMode=40us LTR1.2_Threshold=90112ns
```

We can see it wants to see at least 90112ns for LTR1.2

Set snoop manually:

```sh
# Set snoop Latency to 102.400 us(Scale 2, Value 100 -> 0x0864)
sudo setpci -s 03:00.0 ECAP_LTR+0x04.w=0x0864

# No-Snoop Latency to 102.400 us
sudo setpci -s 03:00.0 ECAP_LTR+0x06.w=0x0864
```

Then check with

```sh
sudo lspci -s 03:00.0 -vvv | grep -A 3 -i "Latency Tolerance"
```

And my output is

```sh
Capabilities: [1b0 v1] Latency Tolerance Reporting
	Max snoop latency: 102400ns
	Max no snoop latency: 102400ns
Capabilities: [220 v1] Physical Resizable BAR
```

## Enabling ASPM L1.2

We need to check for ASPM L1.2 capability and if it's activated

```sh
sudo lspci -s 00:1c.0 -vvv | grep -A 8 "L1 PM Substates"
```

And there we have a blocker

```bash
Capabilities: [200 v1] L1 PM Substates
L1SubCap: PCI-PM_L1.2+ PCI-PM_L1.1+ ASPM_L1.2+ ASPM_L1.1+ L1_PM_Substates+
  PortCommonModeRestoreTime=40us PortTPowerOnTime=44us
L1SubCtl1: PCI-PM_L1.2- PCI-PM_L1.1- ASPM_L1.2- ASPM_L1.1-
   T_CommonMode=40us LTR1.2_Threshold=90112ns
L1SubCtl2: T_PwrOn=44us
Capabilities: [250 v1] Downstream Port Containment
DpcCap: IntMsgNum 0, RPExt+ PoisonedTLP+ SwTrigger+ RP PIO Log 4, DL_ActiveErr+
DpcCtl: Trigger:0 Cmpl- INT- ErrCor- PoisonedTLP- SwTrigger- DL_ActiveErr-
```

Enabling ASPM L1.2 requires adjustments on both sides of the PCIe connection
(Root Port and Endpoint) as well as compliance with LTR thresholds.

1. Establish prerequisites (LTR & ASPM L1)

L1.2 only engages when the Wi-Fi card's LTR value exceeds the Root Port's
LTR1.2_Threshold (90.112 ns in this case) and basic ASPM L1 is active:

```Bash
# Set Wi-Fi card LTR to 102.400 ns
sudo setpci -s 03:00.0 ECAP_LTR+0x04.w=0x0864
sudo setpci -s 03:00.0 ECAP_LTR+0x06.w=0x0864
# Enable Base ASPM L1 (offset 0x68, bits 0+1)
sudo setpci -s 00:1c.0 68.w=0003:0003
sudo setpci -s 03:00.0 68.w=0003:0003
```

2. Determine L1 PM Substates capability offsets

Read the hex base offset for L1 PM Substates for both devices:

```Bash
sudo lspci -s 00:1c.0 -vvv | grep -B 1 "L1 PM Substates"
sudo lspci -s 03:00.0 -vvv | grep -B 1 "L1 PM Substates"
```

The L1SubCtl1 register is always located at Base Offset + 0x08. 3. Enable L1.1
and L1.2 in the L1SubCtl1 register
Writing 0x0f to the lowest 4 bits enables PCI-PM_L1.2, PCI-PM_L1.1, ASPM_L1.2,
and ASPM_L1.1:

```Bash
# Root Port 00:1c.0 (Base 200 -> Register 208)
sudo setpci -s 00:1c.0 208.l=0x0f:0x0f

# Endpoint 03:00.0 (Base 240 -> Register 248)
sudo setpci -s 03:00.0 248.l=0x0f:0x0f
```

4. Verification

```Bash
sudo lspci -s 00:1c.0 -vvv | grep -A 2 "L1SubCtl1"
sudo lspci -s 03:00.0 -vvv | grep -A 2 "L1SubCtl1"
```

Both devices must report the status ASPM_L1.2+ and ASPM_L1.1+ in the L1SubCtl1 line.
And this is what success looks like:

```bash
L1SubCtl1: PCI-PM_L1.2+ PCI-PM_L1.1+ ASPM_L1.2+ ASPM_L1.1+
		T_CommonMode=40us LTR1.2_Threshold=90112ns
L1SubCtl2: T_PwrOn=44us
L1SubCtl1: PCI-PM_L1.2+ PCI-PM_L1.1+ ASPM_L1.2+ ASPM_L1.1+
		T_CommonMode=0us LTR1.2_Threshold=90112ns
L1SubCtl2: T_PwrOn=44us
```

Now let's check pkg c-states again and see if it did something:

```sh
sudo cat /sys/kernel/debug/pmc_core/package_cstate_show
```

Nope:

```txt
Package C2 : 385609606
Package C3 : 520279083
Package C6 : 0
Package C7 : 0
Package C8 : 0
Package C9 : 0
Package C10 : 0
```

## Getting the apple out of Thunderbolterella

First let's check how she's doing

```bash
for dev in /sys/bus/pci/devices/*; do
  if lspci -s "${dev##*/}" | grep -qiE "Thunderbolt|NHI"; then
    echo "${dev##*/}: $(cat "$dev/power/runtime_status") (Control: $(cat "$dev/power/control"))"
  fi
done
```

Gives me

```bash
0000:04:00.0: active (Control: auto)
0000:05:00.0: active (Control: auto)
0000:05:01.0: suspended (Control: auto)
0000:05:02.0: suspended (Control: auto)
0000:05:04.0: suspended (Control: auto)
0000:06:00.0: active (Control: auto)
0000:07:00.0: suspended (Control: auto)
0000:7a:00.0: active (Control: auto)
0000:7b:00.0: active (Control: auto)
0000:7b:01.0: suspended (Control: auto)
0000:7b:02.0: suspended (Control: auto)
0000:7b:04.0: suspended (Control: auto)
0000:7c:00.0: active (Control: auto)
0000:7d:00.0: suspended (Control: auto)
```
