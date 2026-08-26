# psr

Panel Self Refresh on Apple eDP panels.

Intel disabled PSR on every Apple panel in 2018 with 1035f4a65f58
("drm/i915: Disable PSR in Apple panels"), noting that i915 did not support
these panels yet. Nobody came back to it. The panels do not enter self refresh
from `DP_PSR_EN_CFG` alone: they need three vendor DPCD registers, no
`DP_PSR_CRC_VERIFICATION`, and a capture trigger at DPCD 0x4d4. The sequence
was taken from a DTrace capture of macOS.

The three patches add that handshake as an i915 DPCD quirk keyed on the Apple
sink OUI 00:10:fa, lift the 2018 block, and remove the then unused quirk flag.

Measured on a MacBookPro16,2 on an idle desktop: 0.380 W less package power,
0.515 W less battery draw. Tested on MacBookPro16,2, MacBookPro15,1,
MacBookAir9,1 and MacBookPro16,1, four different panels, all working.

On dual GPU machines the internal panel must be driven by the Intel GPU, see
`howto/06-configuring-gpus.md`. With the AMD GPU as primary i915 has no eDP and
the quirk never runs.

Check whether a running kernel has it: `dmesg | grep "Apple PSR handshake"`.

Destined for upstream. The patch files here are the series as it will be sent,
generated with `git format-patch` against 7.1.9.
