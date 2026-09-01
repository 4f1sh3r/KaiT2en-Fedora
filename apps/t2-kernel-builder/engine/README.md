# Fedora patched-kernel builder

This script builds a Fedora kernel with every `*.patch` file in its `patches/`
folder. The build can run on another x86_64 Fedora computer.

The patches and configuration files currently included in `patches/` and
`configs/` are examples from ongoing KaiT2en kernel testing. They are not a
generic patch set or configuration for every T2 Mac. Review and replace them
as appropriate before building a kernel for another machine or test.

## 1. Add patches

Place patches in `patches/`, or select another directory with `--patch-dir`.
They are applied alphabetically, so a numbered series should use names such as:

```text
patches/0001-first-change.patch
patches/0002-second-change.patch
```

Patches must apply after Fedora's own patch set has been applied.

Optional kernel configurations can be stored in the builder's `configs/`
directory independently from the selected patch series. The script does not
select these files automatically: pass the intended file explicitly with
`--config FILE`. Without `--config`, the Fedora x86_64 configuration from the
selected source package is used.

## 2. Install build dependencies

```bash
sudo dnf install bc binutils bison cpio curl dwarves elfutils-libelf-devel \
  flex gcc git-core make openssl-devel patch perl-core rpm-build rsync tar xz
```

## 3. Build

Simple example for installing a reduced T2 test kernel using the default patch
directory. Without an explicit kernel release, the script offers the latest
available build for each of the ten newest kernel versions found on Fedora
Koji interactively:

```bash
./build.sh --t2-config --local-install --localversion -your-custom-kernel-suffix-here
```

Building and installing directly on the target machine example:

```bash
./build.sh 7.1.7-200.fc44.x86_64 \
  --patch-dir /path/to/patch-series \
  --config ./configs/7.1.config \
  --t2-config \
  --local-install \
  --localversion -your-custom-kernel-suffix-here
```

With `--local-install`, compilation runs as the desktop user. The GUI adds
`--defer-install`, then offers installation as a separate PolicyKit-authorized
action after the build has completed.

Optional parameters:

```text
--config FILE          Custom kernel config file
--jobs NUMBER          Number of parallel compiler jobs
--clean                Recreate this version's source and build tree
--local-install        Install directly instead of creating RPM packages
--t2-config            Disable compilation of drivers for hardware not present in T2 Macs
--patch-dir DIRECTORY  Directory containing the patch series
--localversion SUFFIX  Unique suffix beginning with a hyphen
```

Each Fedora release gets a separate directory below `build/`. If patches,
configuration or the local version change, repeat the command with `--clean`.
Without `--local-install`, the final RPM paths are printed when the build
finishes. With `--local-install`, the script builds `bzImage` and modules,
installs them directly, and prints the installed kernel release instead.

## 4. Transfer the kernel to the MacBook

This section is only needed when the kernel was built on another computer.
Create a directory on the MacBook and copy the kernel and kernel-devel RPMs:

```bash
ssh user@macbook 'mkdir -p ~/kernel-test-rpms'
scp build/7.1.7-200.fc44.x86_64/kernel/linux-7.1.7/rpmbuild/RPMS/x86_64/kernel-*.rpm \
  user@macbook:~/kernel-test-rpms/
```

A USB drive can be used instead of `scp`.

## 5. Install on the MacBook

Keep the normal Fedora kernel installed. Install only the generated kernel RPM
with DNF:

```bash
cd ~/kernel-test-rpms
sudo dnf install ./kernel-[0-9]*.rpm
```

Do not pass every RPM to DNF. Fedora's `kernel-devel-matched` package pins the
distribution kernel-devel version and rejects the custom kernel-devel package.
The generated `kernel-headers` package is not needed.

Check that a separate kernel and module directory were installed:

```bash
ls -1 /boot/vmlinuz-*
ls -1 /lib/modules
```

KaiT2en's DKMS modules must exist for the new kernel before booting it. When
building directly on the MacBook, point the new module directory at the
prepared source tree:

```bash
TREE=/path/to/fedora-kernel-build-script/build/TARGET_RELEASE/kernel/linux-VERSION
KVER=VERSION-LOCALVERSION
sudo ln -sfn "$TREE" "/lib/modules/$KVER/build"
sudo ln -sfn "$TREE" "/lib/modules/$KVER/source"
```

When the build happened on another computer, install the copied kernel-devel
RPM directly instead:

```bash
sudo rpm -ivh --nodeps ./kernel-devel-*.rpm
```

Then build the DKMS modules and regenerate the initramfs:

```bash
sudo dkms autoinstall -k NEW_KERNEL_RELEASE
sudo dracut --force /boot/initramfs-NEW_KERNEL_RELEASE.img NEW_KERNEL_RELEASE
```

Select the patched kernel from GRUB's advanced kernel menu. The original
Fedora kernel remains the recovery option.

For tests such as the currently included MTRR patch, unload the existing
workaround or blacklist it before testing the patched kernel:

```bash
sudo modprobe -r t2smp
```

Restore it without rebooting with:

```bash
sudo modprobe t2smp
```
