### Ambitions

**Fix things, do not work around them.** There is a distinct difference between
making broken things work and fixing things.
**Stay on the distribution.** Wrap Fedora, do not fork it.

KAIT2EN is meant to disappear. Every module and every fix that lands upstream
leaves this repository. What remains at the end are the few T2-specific
applications that cannot be upstreamed at all.

### Architecture

Fedora keeps its own kernel and its own update path. KAIT2EN adds T2 hardware
support next to it as DKMS modules: `t2bce` for the Apple T2 bridge and its USB,
audio and storage endpoints, `t2touchbar` and `hid_t2magicmouse` for input,
`t2bdrm` for the Touch Bar display, `t2gmux` for graphics switching, `t2smc` for
sensors and charging, and a handful of smaller pieces.

Because everything is DKMS, a Fedora kernel update is a normal Fedora kernel
update. DKMS notices the new kernel and rebuilds our modules against it. You are
not waiting for anyone to rebase a distribution kernel.

### Installation

The installer never modifies the official Fedora image. It downloads and
verifies the image Fedora publishes, writes it to a USB drive unchanged, and
then adds separate KAIT2EN boot files to the drive's EFI partition. Keyboard,
trackpad and Wi-Fi come up in the live system through initramfs overlays, so
Fedora's own installation payload stays exactly as shipped.

Apple firmware is collected from your own Mac. Nothing is redistributed, and
macOS stays installed as the clean source for it.

### Upstreaming

The feature board above is not decoration. Each row says what a user gets today
and, separately, what is happening to that work upstream: prepared, submitted,
merged, or rejected with a reason. Patches carry the names of the people who
sent them, and the links go to the mailing list or merge request itself.

That second axis exists because the two answers genuinely differ. A merged
kernel patch can be invisible to users for two releases, and a downstream-only
feature can be the most useful thing we ship.
