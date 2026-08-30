# T2 Kernel Builder

GTK4/libadwaita frontend for `scripts/fedora-kernel-build-script/build.sh`.

The configuration page uses the actively maintained sysprog21 Kconfiglib fork
to display the real kernel Kconfig hierarchy, including bool/tristate values
and dependencies. The installer pins the tested upstream commit instead of the
outdated PyPI release.

The app downloads Fedora kernel sources, validates an alphabetically ordered
user patch directory, optionally runs `make localmodconfig` after an explicit
hardware warning, applies selected component groups and uses a configurable
parallel thread count. The default uses all detected hardware threads except one.
At least one patch and an explicitly selected base configuration are required.
Additional component groups are optional; the required T2 hardware profile is
always applied.

Run from the repository:

```bash
./apps/t2-kernel-builder/t2-kernel-builder.py
```

The kernel is built without elevated privileges. After compilation finishes,
the separate Install button uses PolicyKit and the restricted helper to install
exactly that completed kernel tree. No authentication dialog can time out while
the user is away during the build.
Completed builds are recorded in their cache directories. The app discovers
unfinished installations again after an app restart or unexpected shutdown and
offers them in the Completed build selector without recompiling them.
Removing an installed kernel uses a separate PolicyKit authentication dialog.
Downloads and build trees are kept below
`$XDG_CACHE_HOME/t2-kernel-builder/build` (normally `~/.cache`).

The Cleanup tab can remove selected inactive kernels or delete all cached files
and downloaded sources for selected kernel versions. The running kernel, rescue images
and build trees referenced by installed kernels are protected.

Install with:

```bash
sudo ./apps/t2-kernel-builder/install.sh
```
