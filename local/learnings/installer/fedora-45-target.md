# Fedora 45 auto-installer target (GA day, 2026-11-03)

## Derived values (all from live GA sources)

- GA compose is `45-1.4` — the GA trees under `releases/45/` carry the same
  build as the last test compose; ISOs are named `*-45-1.4.x86_64.iso`.
- ISO kernel / fast-path base: `7.2.11-300.fc45` (from `releases/45/Everything/x86_64/os/Packages/k/`
  listing; `development/45` already carries newer 7.2.13+ so it must not be used
  for derivation).
- kernel-devel sha256 `efd30f2a6a664ec177ec207e56b4513c8eca89fb580b3f64ec4d62b8aab16ce5`
  (hashed from the exact kojipkgs URL).
- anaconda `45.30-1.fc45`.
- container base digest: `registry.fedoraproject.org/fedora@sha256:c15d8d6067038f1d6b10d8e1b8440f787a474fe6dc641ef77a5c74b1bf01a05d` (`skopeo inspect docker://registry.fedoraproject.org/fedora:45`).
- loader layout unchanged vs F44: `/boot/x86_64/loader/{linux,initrd}` — verified on both
  GA ISOs without downloading them by parsing ISO9660 via HTTP range requests
  (PVD at 32768, root dir record there; dir names live at record offset 33;
  iterate records by their `length` byte, not the EA byte).

## Input patch state for 45

`fedora-45-input.patch` only needs the softdep hunk
(`pre: t2hid hid_t2magicmouse post: t2bce_vhci` in t2bce_main.c).
Hunk (a) `LINUX_VERSION_CODE` guard and hunk (b) `kmalloc_obj` are now in
upstream main (`< 7.0.0` guard at t2bce_main.c line ~945 region, `kmalloc_obj`
at t2hid.c) so they're correct for 7.2 out of the box. Verified:
`KAIT2EN_KMOD_SKIP_PSR=1 build-input-kmod.sh ... 7.2.13-300.fc45.x86_64` built
clean on the laptop and the RPM's t2bce_core.ko carries the intended softdep.
If F45 live-boot shows boot-time ordering is fine without it, drop the hunk
and remove the patch file — then the conf entry must still name an existing
tracked patch, so keeping the one-hunk file is the safe interim.

## Fork test release

Branch `installer/fedora-45-target` (off upstream/main dc43189), tag
`v0.1.0-fedora-45-test` on the fork. CI matrix then builds BOTH fedora-44 and
fedora-45 kits into one release; bootstrap picks `KaiT2en-Fedora-45-1.12-...`
by sort -V. Kit version stayed 1.12 (kit code unchanged).

## Timing notes

- `releases.json` gained the plain `45` GA entries on GA morning (before that
  only `45 Beta`) — edition-catalog CI gate passes from then on.
- For several hours after GA, `metalink?repo=fedora-45` still points at
  `development/45` trees and mirrors lack `releases/45` paths; GA-only source
  was `dl.fedoraproject.org/pub/.../releases/45/` (serves range requests fast).
