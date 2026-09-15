# psr

Panel Self Refresh on Apple T2 eDP panels.

Intel disabled PSR on every Apple panel in 2018 with 1035f4a65f58
("drm/i915: Disable PSR in Apple panels") because i915 did not support the
panel-specific enable sequence. These panels need three vendor DPCD setup
writes, `DP_PSR_CRC_VERIFICATION` left clear, and a capture trigger at DPCD
0x4d4.

The patch adds that handling as an i915 DPCD quirk. Apple iGPUs report
0x106b as their PCI subsystem vendor, which the quirk table uses to select
the Apple sink entries; the quirk then applies only to internal eDP sinks
with the Apple OUI 00:10:fa. External Apple sinks retain the existing PSR
block. The same sink-side protocol may also be present on T1 and Apple silicon
systems, but those systems remain unchanged because they have not been tested.

Measured on a MacBookPro16,2 on an idle desktop: 0.380 W less package power and
0.515 W less battery draw. Tested on MacBookPro16,2, MacBookPro16,1,
MacBookPro15,2, MacBookPro15,1 and MacBookAir9,1 across boot and
suspend/resume.

On dual-GPU machines the internal panel must be driven by the Intel GPU; see
`howto/06-configuring-gpus.md`.

The test build uses the drm-tip-based v3 submission copy unchanged. It
applies cleanly with `patch -p1` to Fedora's Linux 7.2.2 source tree
(vanilla and Red Hat 7.2 patches do not touch i915), so the COPR kernel
build picks it up on the 7.2 base.
