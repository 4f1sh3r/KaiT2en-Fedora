# PSR test try 1

This experiment enables Panel Self Refresh (PSR) on Apple eDP panels and adds
the vendor handshake observed on the Banksia TCON in a MacBookPro16,2.

It contains two patches:

1. Stop applying the blanket `DP_DPCD_QUIRK_NO_PSR` flag to Apple panels.
2. Enable PSR for the Apple TCON and issue its vendor-specific AUX handshake.

The handshake was derived from a DTrace capture of
`AppleIntelPort::writeAUX` under macOS 15.7.7. This is an experimental patch
set, not a generally supported kernel configuration.

GitHub Actions derives the kernel suffix from this directory name. A build
from commit `abcdef0` therefore has a suffix similar to
`.kait2en.psr_test_try1.gabcdef0`. The workflow publishes the COPR build URL,
exact package versions, installation commands, and rollback instructions in
the pull-request comment and the GitHub Actions job summary.

## Test notes

- Target hardware: MacBookPro16,2 with its internal Apple eDP panel.
- Secure Boot must be disabled because the test kernel and DKMS modules are
  unsigned.
- Keep a stock Fedora kernel installed and available in GRUB.
- The handshake can be disabled at boot with `i915.t2_apple_psr=0`.
- After booting, record `uname -r`, the i915 PSR debug status, relevant i915
  messages from the kernel log, and any display corruption or resume issues.

