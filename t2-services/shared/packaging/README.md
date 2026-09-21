# Independent binary packages

The four recipes build `t2-touchid`, `t2-ave`, `t2-journal` and
`t2-services-common`. Each feature owns its recipes under `packaging/`.
Selecting a recipe builds only that component. The shared crate sources
are compile-time inputs, not runtime library packages.
Kernel modules are not included in userspace recipes. The distribution
must supply the matching BCE/AVE kernel stack.

## Source preparation

Create a source directory named `t2-services-0.1.0` containing the repository's
`LICENSE`, `LICENSING.md`, `LICENSES/`, `packaging/lifecycle/`, and the
`t2-services/` tree. Exclude Cargo
`target/` directories and generated SELinux/kernel output. Preserve component
lockfiles. Before an isolated build, prepare the locked Rust dependencies like:

```sh
mkdir -p .cargo
cargo vendor --locked \
  --manifest-path t2-services/t2-touchid/Cargo.toml \
  --sync t2-services/t2-ave/Cargo.toml \
  --sync t2-services/t2-journal/Cargo.toml \
  vendor > .cargo/config.toml
```

Run that command inside the source directory. Dependency retrieval belongs to
source preparation. Package builds use `--frozen` and don't fetch dependencies.
A populated Cargo cache also suffices for local recipe checks. Distribution
maintainers may substitute packaged Rust sources according to their policies.

Archive the directory as `t2-services-0.1.0.tar.gz`. For RPM, put it in the
builder's `SOURCES` directory and select, for example:

```sh
rpmbuild -ba t2-services/t2-touchid/packaging/rpm/t2-touchid.spec
```

For Debian, unpack a fresh source tree per component, copy that component's
`packaging/debian/` to the source root as `debian/`, and place the source archive
next to the tree as `<package>_0.1.0.orig.tar.gz`. Then run
`dpkg-buildpackage -us -uc`. Debian helpers must be installed in the build image.

## Activation and existing installations

Recipes preserve configuration files, use systemd presets on first installation,
and only restart already active units. Administrators can enable the desired
feature explicitly. Installing Touch ID does not bind fingerprints to the
package-installer account and does not enable PAM.

Fedora's Touch ID recipe compiles and installs the SELinux policy. The Debian
recipe targets the default non-SELinux setup. SELinux-enabled Debian systems
must build/install the policy from `t2-touchid/integration/selinux` as well.

Lifecycle actions collect errors, continue, and print a summary. Failures are
also appended to `/var/log/t2-services-install.log`. An activation error still
requires administrator attention.

## Source installation migration and removal

Package installation retires recognized source-installed binaries from
`/usr/local/bin`, matching local service units and the fprintd drop-in before
activating vendor units. Existing enablement is repaired without running the
common suspend unit. Package upgrades never invoke sleep hooks.

The common package migrates known combined suspend helpers to the current
Wi-Fi/Bluetooth-only implementation. That independent helper stays with the
OS installation, it is not removed together with the T2 services. NCM profiles
are consolidated on disk while preserving the selected source profile's UUID.
Unrelated profiles are untouched. No live network disconnect is issued.

Ownership receipts live in `/var/lib/kait2en/ownership/<package>.json`.
Recoverable migration backups live in `/var/lib/kait2en/migration/<package>/`.
Removal cleans unchanged, verified migration backups and generated files.
It never restores obsolete files that would shadow another package install.
Administrator-modified files, unrecognized local units and symlinks are
preserved and reported. Package-manager configuration-file retention still
applies. Logs are retained for diagnosis.

Touch ID records the checksum of its installed SELinux module and removes
only that unchanged module at removal. The source installer records whether
it introduced the authselect fingerprint feature and the resulting profile.
Removal disables it only if the owned profile is still unchanged. Packages
do not enable fingerprint PAM themselves. An older source installation with
an already enabled fingerprint feature but no receipt cannot prove who enabled
it. That setting is reported and preserved, never retrospectively claimed.

The migration engine is shared as source under `packaging/lifecycle/`.
It is unrelated to the live-image/OS installer under `auto-installer/`.
