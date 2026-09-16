# Independent binary packages

The four recipes build `t2-touchid`, `t2-ave`, `t2-journal` and
`t2-services-common`. Each feature owns its recipes under `packaging/`.
Selecting a recipe builds only that component. The shared crate sources
are compile-time inputs, not runtime library packages.
Kernel modules are not included in userspace recipes. The distribution
must supply the matching BCE/AVE kernel stack.

## Source preparation

Create a source directory named `t2-services-0.1.0` containing the repository's
`LICENSE`, `LICENSING.md`, `LICENSES/`, and `t2-services/` tree. Exclude Cargo
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

When migrating an existing source installation, first update its old
`kait2en-suspend.sh` through `scripts/fedora/install-suspend-service.sh` so it no
longer binds/unbinds NCM. Remove obsolete `/etc/systemd/system` overrides before
switching to vendor units. Otherwise they override the package's `/usr` paths.
Use the source installer's common-network migration to consolidate old NCM
profiles before introducing the package-owned profile. Package scripts do not
delete administrator network profiles or replace local units automatically.
