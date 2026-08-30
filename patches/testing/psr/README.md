# psr

Panel Self Refresh on Apple T2 eDP panels.

Intel disabled PSR on every Apple panel in 2018 with 1035f4a65f58
("drm/i915: Disable PSR in Apple panels") because i915 did not support the
panel-specific enable sequence. These panels need three vendor DPCD setup
writes, `DP_PSR_CRC_VERIFICATION` left clear, and a capture trigger at DPCD
0x4d4.

The patch adds that handling as an i915 DPCD quirk. It is limited to internal
eDP sinks with the Apple OUI 00:10:fa on systems containing an Apple T2 (PCI
106b:1801). Pre-T2 panels and external Apple sinks retain the existing PSR
block. The same sink-side protocol may also be present on T1 and Apple silicon
systems, but those systems remain unchanged because they have not been tested.

Measured on a MacBookPro16,2 on an idle desktop: 0.380 W less package power and
0.515 W less battery draw. Tested on MacBookPro16,2, MacBookPro16,1,
MacBookPro15,2, MacBookPro15,1 and MacBookAir9,1 across boot and
suspend/resume.

On dual-GPU machines the internal panel must be driven by the Intel GPU; see
`howto/06-configuring-gpus.md`.

The test build uses the drm-tip-based submission copy unchanged. It also
applies cleanly to Fedora's patched Linux 7.2.2 source tree.
