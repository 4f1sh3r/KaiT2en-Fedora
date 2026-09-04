# t2bce_vhci scatter-gather Learnings

**Status: solved, running on hardware.** Branch `fix/t2bce-t2bdrm-scatter-gather`:

| commit | what |
| --- | --- |
| `efd0f67` | t2bce: implement chunked scatter-gather transfers (core / dma / vhci) |
| `fa2a090` | t2bdrm: send framebuffer updates with scatter-gather |

This supersedes the bounded-request + wMaxPacketSize-padding workaround on
`fix/t2bdrm-bounded-requests` (see `local/learnings/modules/t2bdrm.md`). A full Touch Bar frame
is one logical URB again, and `t2bdrm` now allocates it with `vzalloc()` *on
purpose* — page-fragmented memory is the normal case, not the failure case.

Earlier state of this file: "advertising sg is necessary but not sufficient, the
OUT data phase never starts". That is resolved; the sections below keep the
measurements because they are the evidence for *why* each piece was needed.

## 1. The two missing bus fields (necessary, not sufficient)

`vhci.c` never set either of these, so `usb_sg_init()` never built a
scatter-gather URB at all:

- `hcd->self.sg_tablesize` — `usb_sg_init()`
  (`drivers/usb/core/message.c:578`) keys on exactly this:
  `if (dev->bus->sg_tablesize > 0)` builds **one** URB with `num_sgs = nents`,
  otherwise **one URB per scatterlist entry**.
- `hcd->self.no_sg_constraint` — without it `usb_submit_urb()`
  (`drivers/usb/core/urb.c:488`) rejects any URB whose sg entries (except the
  last) are not a multiple of the endpoint max packet size. BCE takes arbitrary
  address/length pairs, so the constraint does not apply.

Field types (`include/linux/usb.h:467`): `unsigned sg_tablesize`,
`unsigned no_sg_constraint:1`. `~0U` matches what xhci does.

Measured with a throwaway `t2bdrmsg` module (vanilla t2bdrm forced to
`vzalloc()` + `usb_sg_init()`/`usb_sg_wait()`), against the **unpatched** vhci:

| counter | unpatched | with the two fields |
| --- | --- | --- |
| `sg_last_nents` | 89 | 89 |
| `sg_last_entries` | **89** | **1** |
| `sg_last_us` | 1055469 | 1053472 |
| `sg_sends` | 0 | 0 |
| `sg_timeouts` / `sg_errors` | 1 / 1 | 3 / 3 |

So the fields do exactly what they should — 89 page segments fold into a single
URB — and the transfer still did not complete. 89 separate URBs on this endpoint
do not merely cost more, they never complete at all; they wedge until cancelled.

Note `io.entries` is reused by usbcore during `usb_sg_wait()` and reads back as
`-1` afterwards, so it must be captured right after `usb_sg_init()`.

## 2. Why the data phase never started

ftrace on the vhci transfer path, 6 s window, react-drm running, against a
linear URB as the control:

| function | sg URB | usb_bulk_msg (control) |
| --- | --- | --- |
| `bce_vhci_urb_create` | 2 | 2 |
| `bce_vhci_urb_init_sg_segments` | 2 | 2 |
| `bce_vhci_urb_data_update` | **0** | 2 |
| `bce_vhci_urb_set_sg_submission` | **0** | 0 |

The URB was created and its segment array built, then nothing. For OUT the
device drives the data phase, and `bce_vhci_urb_send_out_transfer_data()` only
runs from `bce_vhci_urb_data_update()` on an incoming
`BCE_VHCI_CMD_TRANSFER_REQUEST`. That message never arrived for an sg URB.

**Cause:** the old code wrote a bare array of address/length pairs with no
header, and left the SQ entry's own `addr`/`length` fields at zero. The firmware
had nothing to read. The generic BCE driver
(<https://github.com/t2linux/apple-bce-drv/blob/master/queue_dma.c>) prefixes the
list with a header, and uses the SQ entry's `addr`/`length` as a **byte window**
into the data the list describes — not as a buffer pointer:

```c
struct bce_segment_list_header {
    u64 element_count;
    u64 data_size;
    u64 next_segl_addr;
    u64 next_segl_length;
};
```

Which of the two omissions was the fatal one was not bisected; both were wrong
and both were fixed together in `280e6b6`.

## 3. The 32-element limit, and the CATERR

bridgeOS rejects a single segment-list descriptor past **32 elements**. This
limit is empirical for this VHCI/bridgeOS, not documented anywhere.

`next_segl_addr` / `next_segl_length` exist for exactly this case in the generic
driver — chain a second descriptor. **Do not do that here.** The chaining attempt
is the run that produced a **CATERR** (the machine's catastrophic-error signal, a
hard platform-level failure, not a driver oops). VHCI therefore deliberately
leaves both fields zero and continues a long transfer as **independent
page-sized segment lists**, one SQ submission each, while keeping a single
logical URB. That decision is recorded in a comment at `queue.c:28` so nobody
re-derives it the hard way.

The pool stays page-granular for the same reason: the reference implementation
allocates and writes segment lists in page granularity. The ~3.5 KiB wasted per
chunk is not worth a firmware protocol experiment.

## 4. How many chunks actually occur

The BCE device's IOMMU group is `type=identity` (`intel_iommu=on iommu=pt`), so
`dma_map_sg()` goes through dma-direct and does **not** coalesce. A full frame is
89 page segments → `DIV_ROUND_UP(89, 32)` = **3 chunks**, every frame. The
continuation path is therefore exercised continuously in normal operation, not
just in theory. If a future kernel or boot option puts this device in a
translated domain, segments would coalesce and the multi-chunk path would go
quiet — worth remembering before assuming it is still covered.

## What the driver does now

**`t2bce_dma`** owns the wire format. `t2bce_dma_create_segment_list()` builds
the chunk list once per URB from the *mapped* scatterlist, out of a `dma_pool` of
page-sized coherent buffers. Coherent means no `dma_sync_*` dance, and immutable
after build means several submissions can safely reference the same list.
`t2bce_dma_set_next_submission_segment_list()` locates the chunk containing a
byte offset and submits the window that fits inside it, reporting back how much
it actually took.

**`t2bce_vhci`** keeps one logical URB across those submissions:
`request_end` is the window the device granted, `submitted_length` is what is
currently in flight, `receive_offset` is what has completed.
`bce_vhci_urb_continue_in()` / `bce_vhci_urb_continue_out()` issue the next chunk
from the completion path (non-blocking reservation, `*timeout == 0`, safe under
`q->urb_lock`). `mem_flags` is now threaded from `urb_enqueue()` all the way to
`dma_pool_zalloc()`, so a `GFP_ATOMIC` submitter is honoured.

**`t2bdrm`** allocates the request with `vzalloc()` and describes it with an
`sg_table` built via `vmalloc_to_page()` (`appletbdrm_alloc_sg_request()`), then
submits it with `usb_sg_init()`/`usb_sg_wait()`. `io_lock` serialises request and
response so a concurrent `appletbdrm_send_msg()` cannot interleave on the wire.

## Recovery rules (`eb7dbb6`, `38be7e1`)

- **A failed chunk must reset the endpoint.** Completing the URB alone leaves
  bridgeOS holding an open request window for the rest of the data, and the next
  completion arrives as "completion while no requests are pending".
  `bce_vhci_urb_fail_data_transfer()` stalls the queue and schedules the reset,
  mirroring what `bce_vhci_urb_control_check_status()` already did.
  Exception: a short IN with `URB_SHORT_NOT_OK` — the short packet closes the
  window legitimately, there is nothing to discard.
- **Completion status is checked strictly.** `ABORTED` is filtered earlier (queue
  flush); `ERROR`, `NO_SPACE`, `OVERRUN` are real transport failures and now also
  trigger recovery. Note this reaches the control data phase too, so it is a new
  path by which EP0 can be stalled and reset — worth watching in the journal
  given the history in `local/learnings/t2bce/resume.md`.
- **An OUT URB must not be resumed like an IN URB.** `bce_vhci_urb_resume()` used
  to call `bce_vhci_urb_data_transfer_in()` for *any* URB in
  `WAITING_FOR_COMPLETION`; for a bulk-OUT-only queue `q->sq_in` is NULL, which
  is a NULL deref. OUT now resets its window and waits for a fresh
  `TRANSFER_REQUEST` instead.
- **Only proven zero-length URBs are supported.** The `URB_ZERO_PACKET` branch
  was written, never exercised by any submitter, and would have sent an
  unsolicited 0-length submission after the granted window was consumed. Removed.
  `bce_vhci_urb_is_zlp()` is a pure predicate now; it cannot double-send because
  after the ZLP completes `receive_offset < request_end` is false and a second
  `TRANSFER_REQUEST` in `WAITING_FOR_COMPLETION` hits the `-EAGAIN` deferral.
- **`usb_sg_init()` never sets `URB_ZERO_PACKET`** (only `URB_NO_INTERRUPT`, plus
  `URB_SHORT_NOT_OK` for IN). t2bdrm therefore still sends its own explicit ZLP
  via a separate `usb_bulk_msg(NULL, 0)` when `size % max_packet == 0`. In
  practice a full frame is 361584 or 390752 bytes, neither a multiple of 512, so
  the trigger from the padding-era bug does not occur — but the gap is real and
  the guard stays.
- **`io.status` is the timeout authority.** An `expired` flag set by the timer
  callback races: if the timer fires between `usb_sg_wait()` returning and
  `timer_delete_sync()`, the frame went out but is reported as `-ETIMEDOUT`, the
  response is never read, and the stream is one response behind **forever**.
  After `timer_delete_sync()`, `io.status == -ECONNRESET` is the only reliable
  signal. This was the mechanism behind the observed
  `Response timestamp … doesn't match request timestamp` bursts, where every
  line's response timestamp equalled the previous line's request timestamp.
- **A desync must be repaired, not just reported.** `appletbdrm_read_update_response()`
  discards one stale response and re-reads. A desync can only ever be "one
  response too many queued", never one too few, so a single retry is sufficient.
  Worst case it holds `io_lock` for 2 × `APPLETBDRM_BULK_MSG_TIMEOUT` (2 s) in
  the atomic commit tail.
- **Missing HCD sg support is a probe-time rejection**, not a per-frame
  `drm_err`. t2bdrm hard-depends on the patched vhci; there is no linear
  fallback, because the buffer is deliberately vmalloc'd.

## Measured

ftrace over four full-frame updates:

| stage | mean |
| --- | --- |
| sg send | 0.755 ms |
| response read | 2.121 ms |
| total | **2.876 ms** |

Against the references in `local/learnings/modules/t2bdrm.md`: a single linear transfer was
~3.3 ms, three bounded requests ~5.0 ms. Note the split — the send is only 26 %
of the frame cost now; the rest is the device answering. Further driver-side
optimisation has little left to win.

## Two failure modes injected deliberately

- Timer forced to fire *after* a completed sg transfer → no false `-ETIMEDOUT`.
- One response deliberately left unread → the next frame discarded it and
  resynchronised permanently.

## Test procedure

`t2bdrm` has to be out of the way and udev re-binds it as soon as the device
switches to configuration 2, so a temporary blacklist is required:

    systemctl --user stop react-drm.service
    echo "blacklist t2bdrm" | sudo tee /etc/modprobe.d/zz-t2bdrm-test.conf
    sudo rmmod t2bdrm
    sudo insmod <build>/t2bdrm.ko
    systemctl --user start react-drm.service

Reverse it, `rm` the blacklist and `modprobe t2bdrm` to restore. The
`Failed to send message (-110)` during teardown is the known benign reload
artifact.

Any change to `t2bce_vhci` needs a **reboot** — reloading it live takes the
keyboard, trackpad and Touch Bar with it. `/tmp` is tmpfs and does not survive
that, so test modules and traces belong somewhere persistent.

`bce_vhci_urb_send_out_transfer_data` is not individually ftrace-able (inlined).
`set_ftrace_filter` must be written one symbol at a time; a single unresolvable
symbol fails the whole write with EINVAL.

## Verifying the running build

`t2bce_core`, `t2bce_dma` and `t2bce_vhci` expose `/sys/module/<mod>/srcversion`
and `modinfo -F srcversion` works, so installed-vs-loaded compares directly.
`t2bdrm` exposes **no** `srcversion` and `modinfo -F srcversion t2bdrm` returns
empty — use the GNU build-id instead:

    od -An -tx1 /sys/module/t2bdrm/notes/.note.gnu.build-id | tr -d ' \n' | tail -c 40
    xz -dc /lib/modules/$(uname -r)/extra/t2bdrm.ko.xz > /tmp/x.ko
    readelf -n /tmp/x.ko | grep -A1 'Build ID'

## Open

- The IN direction has never carried a **multi-chunk** transfer. IN traffic on
  this HCD is HID and audio, far below 32 segments, so
  `bce_vhci_urb_continue_in()` is written and reviewed but not exercised.
- The OUT resume path assumes bridgeOS re-issues a `TRANSFER_REQUEST` after
  pause → resume. Probably already covered incidentally by the cancel path
  (`bce_vhci_transfer_queue_cancel_w()` pauses and resumes), but not confirmed by
  a deliberate suspend/resume with a frame in flight. Failure mode if wrong is a
  wait into the URB's own timeout, not a crash.
- Whether bridgeOS has an upper bound on the *number* of chunks, as opposed to
  the 32 elements within one. `sg_tablesize` is `~0U` today.
- Nothing here has been offered upstream. Mainline `appletbdrm` still uses
  `kvzalloc()` + `usb_bulk_msg()`, which on this hardware means the DMA API
  rejects most buffers outright (see `local/learnings/modules/t2bdrm.md`).

## Handing the branch to external testers (2026-08-16)

Four modules change together: `t2bce_dma`, `t2bce_core`, `t2bce_vhci`, `t2bdrm`.
Three constraints decide how this can be distributed at all.

### Fedora builds without CONFIG_MODVERSIONS

    $ grep CONFIG_MODVERSIONS /boot/config-$(uname -r)
    # CONFIG_MODVERSIONS is not set

There is **no CRC check on symbol resolution**. Both `t2bce_dma` and
`t2bce_core` changed the signature of `..._set_next_submission_segment_list()`
and dropped `struct t2bce_{core,dma}_dma_segment`, yet a stock-built
`t2bce_vhci` will link against a new `t2bce_core` without a single log line and
then call the new implementation with the old argument list. The failure mode is
silent memory corruption, not a load error. Any distribution mechanism therefore
has to swap all of dma/core/vhci atomically — a partial install is worse than no
install.

`t2bce_audio` is the one consumer that does *not* need rebuilding: it only uses
`t2bce_core_set_next_submission_single()` and never touches a segment list, and
none of the structs it sees changed layout. Confirmed by comparing
`Module.symvers` between a `main` worktree build and the branch build (all
shared symbols identical, only the four new `*_segment_list` symbols added).

### The initramfs holds three of the four

    force_drivers+=" t2bce_dma t2bce_core t2bce_vhci t2hid "
    /etc/dracut.conf.d/90-kait2en-input.conf

Those three are loaded from the initramfs and stay loaded across `switch_root`,
so **installing the modules without rebuilding the initramfs tests nothing** —
the stock build keeps running while `/lib/modules/.../extra` holds the new one.
`t2bdrm` is not in the initramfs and does load from the root filesystem, which
is harmless only because `t2bdrm` calls no `t2bce_*` symbol at all (it is a
plain USB driver on the VHCI bus).

### Booting an older kernel is not an escape hatch

`install-dkms-modules.sh` removes every registered version with `dkms remove
--all` (all kernels) but reinstalls with `dkms install` **without `-k`**, i.e.
for the running kernel only. After any KaiT2en DKMS run, the other installed
kernels have no T2 modules at all. A tester who breaks their keyboard cannot
recover by picking the previous GRUB entry; they need a USB keyboard.

### What this makes the distribution path

Because `install-dkms-modules.sh` wipes every registered version of every module
before reinstalling from the checkout it is run in, the branch needs no test
harness of its own:

    git clone -b fix/t2bce-t2bdrm-scatter-gather <fork> ~/kait2en-sg
    sudo ~/kait2en-sg/scripts/fedora/install-dkms-modules.sh
    sudo ~/kait2en-sg/scripts/fedora/rebuild-initramfs.sh

and reverting is the same two scripts run from a stock checkout. Nothing extra
is registered, so nothing extra has to be unregistered.

A side-by-side variant was built first — same sources staged under a suffixed
DKMS version (`0.06+sg1`) with `AUTOINSTALL="no"`, leaving the stock trees
untouched. It works (verified through a full install/revert cycle), but it buys
nothing over the above and costs a reboot-time hazard: `AUTOINSTALL="no"` means
a kernel upgrade silently drops back to stock. Discarded.

Two traps found while building it, both worth remembering:

- `dkms status ... | grep -Fq` under `set -o pipefail` is a race. `grep -q`
  exits at the first match, `dkms status` gets SIGPIPE, the pipeline reports
  failure, and the caller concludes the module is not installed. Use
  `grep -Fq ... <<<"$(...)"`. The same pattern is in `install-dkms-modules.sh`
  (`dkms_module_version_installed`) and survives only because `dkms status`
  output is short enough to be written before `grep` exits.
- DKMS 3.4 keeps the built module at
  `/var/lib/dkms/<n>/<v>/<kver>/<arch>/module/<n>.ko.xz` — already compressed
  and signed. `build/` is cleaned by the post-transaction hook, so verification
  has to compare against the `module/` copy, decompressed on both sides.

### Verifying a tester is on the branch build

`srcversion` is an MD5 over the module sources only, so it is identical on every
machine and every kernel for a given commit. For `fa2a090`:

    t2bce_core  702F341487D60A5895035B6
    t2bce_dma   5AC9D8C7E6647F6DBCC8EBD
    t2bce_vhci  9BE10F8B03AB63B80021C20

`cat /sys/module/<m>/srcversion` against that catches a stale initramfs in one
command. `t2bdrm` carries no `srcversion`; use the GNU build-id as above.

## Rebase onto upstream/main (2026-09-01)

The branch was 3 commits ahead of a merge base 110 commits behind
`upstream/main`. Rebased to `upstream/main` = `d6ce476`; it is now exactly
`upstream/main` + 2 commits (`3d0276d` t2bce chunked SG, `47e8cd4` t2bdrm SG).
Pre-rebase tip is kept as `backup/sg-pre-rebase-20260901`.

### What the rebase had to resolve

Only three upstream commits since the merge base touch the same sources:

- `6894352 modules: fix builds against Linux 7.2` — the branch carried its own
  copy of the identical change (`240003e`), made before the upstream one
  landed. Same content, different line offsets because the branch's
  `t2bdrm.c` already had the SG rewrite, so the patch-ids differ and `git
  rebase` could not drop it up front. The single conflict was the two
  neighbouring includes (`linux/version.h` from upstream, `linux/vmalloc.h`
  from SG); keeping both makes the tree identical to the old branch tip, after
  which `240003e` was dropped as "patch contents already upstream". All four
  7.2 compat pieces survive: the include, the
  `LINUX_VERSION_CODE < KERNEL_VERSION(7,2,0)` `drm_atomic_commit` define, and
  the three plane-helper signatures.
- `b1855a0 make module author email adress consistent` — one line each in
  `t2bce_core/t2bce_main.c` and `t2bce_dma/queue.c`, no conflict, but it does
  change those modules' `srcversion`.
- `bdfa6a2 dkms: merge t2bce modules` — see below.

### The DKMS merge removes this branch's biggest hazard

Upstream now ships one DKMS package `t2bce_stack/0.01` that builds
`t2bce_dma`, `t2bce_core`, `t2bce_vhci` and `t2bce_audio` in a single
`M=` kernel build (`modules/t2bce_stack/Makefile` with `obj-m += <dir>/`).
The per-module `dkms.conf` files are gone; `install-dkms-modules.sh` stages all
four component directories under `/usr/src/t2bce_stack-0.01/` and the old
hand-copying of a sibling's `Module.symvers` is gone with them.

This matters for this branch specifically. The SG work changes the
`t2bce_core` <-> `t2bce_vhci` interface, and Fedora builds without
`CONFIG_MODVERSIONS`, so a half-swapped set used to link silently and corrupt
memory. With one package that mismatch is no longer constructible: the four are
built together against one `Module.symvers` and installed or removed as a unit.

The retired split packages are not deleted — `install-dkms-modules.sh` rewrites
their `dkms.conf` to `AUTOINSTALL="no"` so older kernels keep working modules
while new kernels only get `t2bce_stack`.

`rebuild-initramfs.sh` also changed: it now skips `/lib/modules/<r>` without a
matching `/boot/vmlinuz-<r>`, runs `depmod -a` when `modules.dep` is missing,
and `fail`s instead of warning when a rebuild fails.

### Post-rebase verification

Both packages build clean against the running 7.2.2 kernel — the merged stack
staged the way DKMS stages it (all four `.ko`, no warnings) and `t2bdrm`
standalone. The rebased build's `srcversion`s equal the ones loaded on the
development machine, which is installed from `local/rig` (SG merged), so the
rebase changed no module content. Current values, replacing the `fa2a090` ones
above:

    t2bce_dma    58B3878BBEB6199DF55DAF7
    t2bce_core   00F764B6DB21CFEBF983904
    t2bce_vhci   EAF5DD6181473C3A838EFC4
    t2bce_audio  221B5167882912C5DE070D6

### Consequence for the tester instructions

The install and revert commands are unchanged, but the branch that testers had
been pointed at (`fa2a090`) predates the 7.2 build fix and fails to build on a
7.2 kernel; the rebased tip carries it via upstream. Testers who already cloned
must re-clone or hard-reset, since the branch was force-pushed.
