# Installer boot loader default learnings

Branch: `fix/installer-grubby-set-default`
Started: 2026-08-17

## The report

A new user installed KaiT2en on a MacBookPro16,1. Phase 1 of the guided
installer (`kait2en-prepare`) ran all the way through the Fedora upgrade and
the temporary input kmod build, then stopped with

```
The param /boot/vmlinuz-7.1.8-200.fc44.x86_64 is incorrect
KaiT2en installation stopped at line 166 (status 1).
```

Line 166 of `kait2en-install` is `sudo "$prepare"`, so the failing command is
inside `kait2en-prepare`. The message itself comes from `/usr/sbin/grubby`
(`print_error "The param $1 is incorrect"` in `set_default_bls`), which means
the failing statement was the last one in phase 1:

```
grubby --set-default "$boot_root/vmlinuz-$target_kernel"
```

## grubby does not match kernel paths, it matches BLS `linux` strings

Fedora's `grubby` is a bash wrapper around the BLS entries in
`/boot/loader/entries`. `param_to_indexes()` accepts a `--set-default`
argument only when it equals the entry's `linux` line, or when
`/<basename of the argument>` equals it:

```bash
if [[ $param = "${bls_linux[$i]}" || "/${param##*/}" = "${bls_linux[$i]}" ]]
```

The `linux` line is not a file system path. It is the path grub itself sees,
so it depends on where `/boot` lives:

| `/boot` location                | `linux` line in the entry     | matches `/boot/vmlinuz-X`? |
| ------------------------------- | ----------------------------- | -------------------------- |
| own partition (Fedora default)  | `/vmlinuz-X`                  | yes, via the basename rule |
| directory on a plain root fs    | `/boot/vmlinuz-X`             | yes, exact                 |
| directory on a **Btrfs** root   | `/root/boot/vmlinuz-X`        | **no**                     |

The Btrfs case is produced by `/usr/lib/kernel/install.d/20-grub.install`: it
first writes `linux /vmlinuz-${KERNEL_VERSION}`, then rewrites the line with
`grub2-mkrelpath`, which prefixes the subvolume:

```
$ grub2-mkrelpath /etc/os-release
/root/usr/lib/os-release
```

So on a machine whose `/boot` is a directory inside the Btrfs root subvolume
(no separate `/boot` partition), every path-based `grubby --set-default` fails
even though the entry exists and the machine boots fine.

## Why the earlier grubby calls in phase 1 did not fail

`kait2en-prepare` sets the default twice more, but both times it feeds grubby
back its own output:

```bash
original_default=$(grubby --default-kernel)   # prints the entry's linux line
grubby --set-default "$original_default"      # therefore always matches
```

Only the final call built a path by hand, which is why the installer got all
the way through the Fedora upgrade and the kmod build before failing.

## The fix

Address the entry by index instead of by path, and match on the file name,
which is the one part of the `linux` line that never changes:

```bash
boot_entry_index() {
	grubby --info=ALL | awk -v want="/vmlinuz-$1" '...suffix match...'
}
grubby --set-default-index="$target_entry"
```

The index is also resolved right after the target kernel is picked, so a
machine with no entry at all fails immediately instead of after the ~10 minute
build, and with an actionable message (`kernel-install add ...`).

## Confirmed on the reporter's machine

```
$ findmnt /boot
(no output - /boot is not a mount point)
$ sudo sh -c "grep -h '^linux' /boot/loader/entries/*.conf"
linux /@/boot/vmlinuz-0-rescue-4b735c75e6244b498e13148c1b070260
linux /@/boot/vmlinuz-6.19.10-300.fc44.x86_64
linux /@/boot/vmlinuz-7.1.8-200.fc44.x86_64
$ sudo grubby --default-kernel
/@/boot/vmlinuz-6.19.10-300.fc44.x86_64
```

His Btrfs top level subvolume is `@`, not Fedora's `root`, so the entries read
`/@/boot/vmlinuz-...`. Neither grubby rule matches the `/boot/vmlinuz-7.1.8-...`
that prepare built: not the exact one, and not the basename one, which would
need `/vmlinuz-7.1.8-...`. `grubby --default-kernel` returning the same `/@/boot`
form is why the two earlier `--set-default` calls never failed.

Note the diagnosis command has to keep the glob inside the sudo shell:
`/boot/loader/entries` is `drwx------ root root`, so an unprivileged shell
cannot expand `/boot/loader/entries/*.conf` and grep is handed the pattern
verbatim, which looks exactly like "there are no entries".

## Why it "worked after a reboot"

The reporter hard powered off the machine during the first, autostarted phase 1
run, then booted, ran `kait2en-install` by hand and hit the error, rebooted, and
phase 1 went through. Nothing was repaired in between: the second boot came up
on 7.1.8, and a running kernel >= 7 makes prepare exit at the early return
(`kait2en-prepare:175`, now shifted by the helper) before it ever reaches the
`grubby` call. The bug is still there for every machine with this layout.

His install is nevertheless intact: the failure hit the very last statement of
phase 1, so the forced input drivers were already in the 7.1.8 initramfs and
`kmod-kait2en-input` was already removed. The hard power-off was survivable
because prepare is idempotent on retry.

## Same defect in the docs

`howto/07-updating.md` and `website/docs/post-install/updating.md` told users to
repair a broken kernel with `sudo grubby --set-default /boot/vmlinuz-...`, which
fails on exactly the same machines. Replaced with an index lookup.

## Delivery note

`kait2en-install` re-execs the repository copy of itself
(`delegate_to_repository`), but `kait2en-prepare` is always the copy the
Anaconda addon put in `/usr/local/bin` from the installer kit. A phase 1 fix
therefore does **not** reach a user through `git pull` alone - it needs a new
installer artifact, or the user has to run the repository copy by hand:

```
sudo bash /usr/local/src/KaiT2en-Fedora/packaging/installer/runtime/kait2en-prepare
```

Worth considering: let `kait2en-prepare` re-exec the repository copy after
`bootstrap_repository()`, guarded by an env var, the same way the installer
does.

## Not the cause

- No repository change: `kait2en-prepare` is untouched since 2026-07-27
  (`c2d3ff9`), and the installer kit changes of 2026-08-07..12 only touched the
  live ISO (`grub.cfg.in`, gmux blacklist, download failover).
- Not a missing BLS entry, not `/boot` running full: those would have made
  `grubby --default-kernel` or the `dracut` run fail earlier.
