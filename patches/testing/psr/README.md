# psr

Panel Self Refresh on Apple eDP panels.

Intel disabled PSR on every Apple panel in 2018 with 1035f4a65f58
("drm/i915: Disable PSR in Apple panels"), noting that i915 did not support
these panels yet. Nobody came back to it. The panels do not enter self refresh
from `DP_PSR_EN_CFG` alone: they need three vendor DPCD registers, no
`DP_PSR_CRC_VERIFICATION`, and a capture trigger at DPCD 0x4d4. The sequence
was taken from a DTrace capture of macOS.

Patch 1 adds that handshake as an i915 DPCD quirk, matched on the Apple sink
OUI 00:10:fa and on the presence of an Apple T2 (PCI 106b:1801). Patch 2 lets
panels carrying the quirk past the `DP_DPCD_QUIRK_NO_PSR` check. Apple panels
on machines without a T2 keep the 2018 block and are untouched.

Measured on a MacBookPro16,2 on an idle desktop: 0.380 W less package power,
0.515 W less battery draw. Tested on MacBookPro16,2, MacBookPro16,1,
MacBookPro15,2, MacBookPro15,1 and MacBookAir9,1 - five machines, four
different panel device IDs, all working from boot and across suspend/resume.

On dual GPU machines the internal panel must be driven by the Intel GPU, see
`howto/06-configuring-gpus.md`. With the AMD GPU as primary i915 has no eDP and
the quirk never runs.

Check whether a running kernel has it: `dmesg | grep "Apple PSR handshake"`.

Destined for upstream. The patch files here are generated against 7.1.9; the
series as it will be sent is rebased onto drm-tip and is otherwise identical.
