# t2bdrm Learnings

> **Superseded as of 2026-08-16.** Everything below documents the bounded-request
> line of work, which ended with the wMaxPacketSize padding in `4579ed5`. That
> whole approach — splitting a frame into row bands to dodge a high-order
> allocation — is obsolete: `fix/t2bce-t2bdrm-scatter-gather` makes the VHCI
> accept scatter-gather, so a frame is a single logical URB over ordinary pages
> again and none of the splitting is needed. See `local/learnings/t2bce/vhci-sg.md`,
> and "Superseded: scatter-gather removed the need to split" at the end of this
> file. The analysis below is still worth keeping — the allocation measurements
> and the wMaxPacketSize root cause are what motivated the sg work.

Current branch: `fix/t2bdrm-bounded-requests` (supersedes `fix/t2bdrm-high-order-alloc`,
which in turn superseded `fix/t2bdrm-fragmented-memory`). The new branch carries the
same fix, rebased on a defined upstream base — see "Where the fork actually stands".

The branch is now a **single commit on top of `ad2d003`**, the KaiT2en commit that
replaced the fork's t2bdrm.c with vanilla mainline 7.1.5 `appletbdrm`. Our own
"sync with upstream appletbdrm as of v7.1" commit became redundant with it and was
dropped in the rebase.

## Framebuffer requests are high-order allocations

`.atomic_check` builds one flat buffer holding the request header, one
`appletbdrm_frame` per damage rect (each carrying its own pixel payload), and
the footer. Its size therefore scales with the damaged area, not with a fixed
protocol size.

The panel is 60x2008 in framebuffer coordinates (2008x60 on the device) at
24 bpp, so a full-frame update is 60 * 2008 * 3 + 140 = ~353 KiB. Anything
above `KMALLOC_MAX_CACHE_SIZE` goes straight to the page allocator, so this is
an **order-7** (512 KiB) request for physically contiguous pages, on every
update.

Measured on a 23 h session: `/proc/buddyinfo` zone Normal had **1** free
order-7 block versus **454** order-4 blocks. Result: 169 `page allocation
failure: order:7` splats with a full stack dump each, and the commit aborted
with `-ENOMEM`, i.e. the Touch Bar silently stopped updating while the device
was fine.

## react-drm always dirties the whole framebuffer

`DISPLAY.partialFlush` defaults to `false` (`config.js`: "true = not ready
yet"), so `drmModeDirtyFB()` is called with zero clips and the damage iterator
falls back to full-plane damage. Every single flush is therefore the worst
case. This is why all 169 failures were order-7 and none were smaller.

Consequence for design: anything that only optimises the *multi-clip* case
(e.g. merging damage rects) is a no-op here and fixes nothing.

## kvmalloc memory cannot be passed to usb_bulk_msg()

`usb_bulk_msg()` DMAs directly out of the buffer it is given; the HCD calls
`dma_map_single()`, which needs a kernel-linear, physically contiguous address.
vmalloc memory has neither. `kvzalloc()` returns kmalloc *or* vmalloc memory
depending on fragmentation, so it would work most of the time and break exactly
under the memory pressure it was meant to fix.

The kernel refuses it rather than corrupting anything. `dma_map_single_attrs()`
in `include/linux/dma-mapping.h` starts with

    /* DMA must never operate on areas that might be remapped. */
    if (dev_WARN_ONCE(dev, is_vmalloc_addr(ptr),
                      "rejecting DMA map of vmalloc memory\n"))
            return DMA_MAPPING_ERROR;

so the submission fails with `-EAGAIN` and the driver logs
`Failed to send message (-11)`. Verified present in v6.15, v6.16, v7.0 and
v7.1, i.e. in every kernel the kvzalloc backport targets. This is a plain DMA
API check, not an IOMMU effect — an earlier guess in this file that the IOMMU
catches it was wrong.

This is what killed the first approach on `fix/t2bdrm-fragmented-memory`
(kvzalloc + kmalloc bounce buffer + chunked `usb_bulk_msg`). It worked, but it
needed chunk sizes to be multiples of `wMaxPacketSize` so that no chunk except
the last ends in a short packet, it had to force a USB reset on a short write
to resynchronize the OUT endpoint, and it left an unresolved hazard: a message
was no longer atomic on the wire, so a concurrent `appletbdrm_send_msg()` could
interleave into it.

### Measured: upstream v7.1 takes the vmalloc path most of the time

Upstream fixed the allocation failure in `aaaa684bab1f` ("drm/appletbdrm: Use
kvzalloc for big allocations", v7.1, `Cc: stable # v6.15+`) by switching the
`kzalloc()` to `kvzalloc()`. On this hardware that fixes nothing: the buffer
still goes straight into `usb_bulk_msg()`, where the DMA API rejects vmalloc
memory. `-ENOMEM` plus an allocation splat becomes `-EAGAIN` plus a DMA mapping
`WARNING`, and the update is dropped exactly as before.

Measured on 2026-08-04 with a throwaway module (`t2bdrmkv`, upstream v7.1 plus
counters plus a guard that refuses to submit a vmalloc buffer, so no bad DMA
ever happened). react-drm running normally, no artificial pressure at first:

| phase | kmalloc | vmalloc | share vmalloc |
|---|---|---|---|
| first burst right after load | 1 | 22 | 96% |
| next ~3 min, incl. a bounded `stress-ng` run (3 GB cgroup cap) | 11 | 40 | 78% |
| total | 12 | 62 | **84%** |

Each of those 62 is a 361584 byte buffer that vanilla appletbdrm would have
handed to the host controller. So this is not a rare corner case on a T2 Mac
with a normal desktop session — it is the common path. `/proc/buddyinfo`
during the run showed order-5 and above at zero in both `Normal` and `DMA32`.

A second run with the guard removed (module `t2bdrmv71`, i.e. exactly upstream
v7.1 behaviour) submitted those buffers for real. Result: 192 vmalloc requests,
192 failures, no exceptions.

    t2bdrmv71: submitting a 361584 byte vmalloc request buffer to the host controller
    t2bce_core 0000:e6:00.1: rejecting DMA map of vmalloc memory
    WARNING: ./include/linux/dma-mapping.h:523 at usb_hcd_map_urb_for_dma+0x51c/0x570
    t2bdrmv71 5-6:2.1: [drm] *ERROR* Failed to send message (-11)

Nothing is corrupted and no bad DMA reaches the device — the DMA API stops it
first (see above).

**How bad is it, really?** Less than the failure counts suggest. Those 192
rejects happened alongside 1712 successful updates, so roughly 10% of frames
were dropped, in bursts, while memory was tight. A user watching the panel
during the whole run reported no freeze, no stutter, nothing noticeable — the
Touch Bar stayed fully usable. At 30+ Hz a dropped frame is invisible unless
allocation fails continuously.

So the honest severity, for both the original `-ENOMEM` bug and the kvzalloc
successor, is: dropped updates plus a splat per occurrence in the kernel log,
not a dead Touch Bar. Do not overclaim this upstream. What makes it worth
fixing is that the splat is noise in everyone's journal, the frames are lost
for no reason, and after `aaaa684bab1f` the remaining diagnostic no longer
points at the cause.

Reproduce with the measurement build's module parameters:
`force_vmalloc=1` allocates via `vzalloc()` unconditionally,
`kmalloc_hits`/`vmalloc_hits`/`skipped_transfers` are readable under
`/sys/module/t2bdrmkv/parameters/`. Swapping the module needs the react-drm
dance (stop unit, `rmmod`, `insmod`, start unit) plus a temporary
`blacklist t2bdrm` in `/etc/modprobe.d/`, otherwise udev reloads the real
module when the device switches to configuration 2.

## Where the fork actually stands

`modules/t2bdrm/t2bdrm.c` was vendored from upstream `appletbdrm.c` at commit
`32a92f8c8932` (2026-02-22). Verified by diffing our file against the file at
every upstream commit for that path: against `32a92f` the only differences are
56 added lines (reset-on-timeout) and 4 rename lines, and no upstream line is
missing or altered.

Three upstream commits were missing:

| commit | date | in a release? |
|---|---|---|
| `aaaa684bab1f` kvzalloc | 2026-04-20 | yes, v7.1 |
| `5164f7e7ff8e` `drm_atomic_state` -> `drm_atomic_commit` | 2026-04-27 | no |
| `40a22ad1b5cb` allocate in `begin_fb_access` | 2026-06-10 | no |

Only the first is taken (branch commit "sync with upstream appletbdrm as of
v7.1"). The rename would not build against 7.1.x, which is what this module
ships for, so both master-only commits are left for the upstream submission.

### Update: the fork was moved to vanilla 7.1.5 (`ad2d003`, 2026-08-04)

The KaiT2en owner replaced `modules/t2bdrm/t2bdrm.c` with unmodified mainline
7.1.5 `appletbdrm.c`. That does two things:

- it picks up `aaaa684bab1f` (kvzalloc), so the fork now sits exactly on the
  code that was measured in "Measured: upstream v7.1 takes the vmalloc path
  most of the time". The bug is not fixed by the rebase, it only changes shape:
  no more order-7 allocation-failure splat, instead `dma_map_single_attrs()`
  rejects the vmalloc buffer — one `rejecting DMA map of vmalloc memory`
  WARNING (`dev_WARN_ONCE`, so once per boot) plus a `Failed to send message
  (-11)` per affected update, and that update is lost;
- it **drops the fork-local reset-on-timeout code** (`appletbdrm_queue_reset()`
  / `appletbdrm_reset_device_work()`, ~60 lines). Those queued a
  `usb_reset_device()` whenever a bulk transfer returned `-ETIMEDOUT`. Without
  it, a timed-out transfer leaves the panel wedged until the module is
  reloaded. This is the recovery path that produced the benign
  `Failed to send message (-110)` + reset seen after every driver reload. It is
  correct to drop it for an upstream submission (it is not in mainline), but it
  is a behaviour change for KaiT2en users and worth watching for.

Our fix was rebased onto that state with the reset code left out, i.e. resolved
in favour of vanilla. `ad2d003` also drops the trailing newline of the file;
harmless, but checkpatch will flag it for anyone who touches the last line.

Note for anyone tempted to drop the fork: Fedora does not build the in-tree
driver (`# CONFIG_DRM_APPLETBDRM is not set`), so the `appletbdrm` entry in the
`module_blacklist=` kernel argument is a no-op on stock Fedora kernels and
there is no in-tree module to fall back to.

## The fix: bound the request, split into row bands

The device's protocol is rectangle-based — a request updating a sub-rectangle
is exactly what partial damage already produces. So instead of making one
enormous message transportable, cap the request at a fixed bound and emit each
damage rect as several requests, each covering a band of framebuffer rows.

Why splitting along framebuffer **rows** specifically: only `y1`/`y2` change,
so `frame->begin_y` (`adev->height - rect->x2`) and `frame->height`
(`drm_rect_width`) are untouched, and consecutive bands are adjacent strips
along the device's x axis. A band is the same rectangle *shape* a full-frame
update already is, only shorter — nothing about the wire format changes. This
shape is independently confirmed by react-drm's `DrawBars`, which describes a
full-height band over an x-extent as "the shape appletbdrm accepts".

Because the buffer size is now fixed by panel geometry instead of by damage, it
is allocated once at probe and reused. That also deletes
`struct appletbdrm_plane_state` and the three plane funcs that existed solely to
carry per-commit allocations — `DRM_GEM_SHADOW_PLANE_FUNCS` is enough again.
Net: **-166/+155 lines**, and zero allocations in the commit path.

Fixed overhead is 140 B (48 request + 12 frame + 80 footer) and a framebuffer
row is `adev->height * 3 = 180` B — note `adev->height` is 60, the *device's*
height, which is the framebuffer's width. Each rung lands exactly inside its
intended order:

| bound   | rows | buffer  | order | bands (`ceil(2008/rows)`) |
|---------|------|---------|-------|---------------------------|
| 128 KiB |  727 | 131008  | 5     | 3                         |
| 64 KiB  |  363 |  65488  | 4     | 6                         |
| 32 KiB  |  181 |  32720  | 3     | 12                        |

`drmm_kzalloc` cannot be used for this buffer: `drmm_kmalloc` prepends a ~40 B
`drmres` header, which for the 32 KiB rung (32720 + 40 = 32760) sits right at
the 32768 boundary and would tip the "guaranteed" rung into order-4. Hence
plain `kzalloc` plus `drmm_add_action_or_reset`.

### Two things the reuse and the split require

- The buffer is no longer freshly zeroed per commit, and the footer's offset
  moves with the frame size. `memset(footer, 0, request_size - sizeof(*request)
  - frame_size)` clears the footer *and* the 16-byte alignment padding, which
  used to be zero by virtue of `kzalloc()` and is still sent to the device.
- `rows_per_request` divides by `adev->height`, so a zero display size reported
  over USB would now be a division by zero. `appletbdrm_get_information()`
  gained a size check next to its existing bpp/pixel-format checks.

## Transfers are latency-bound, not bandwidth-bound

An early reading of "64 KiB in 1.19 ms = 55 MB/s" suggested the transfers ran
at USB 2.0 high-speed line rate and were therefore bandwidth-bound, making the
extra round trips free. **That was wrong.** Halving the band size to 32 KiB was
measured directly:

| bound  | bands/frame | per band | per frame |
|--------|-------------|----------|-----------|
| 64 KiB | 6           | 1.264 ms | 7.59 ms   |
| 32 KiB | 12          | 1.096 ms | 12.75 ms  |

Halving the payload cut the per-band time by only 13%, so a transfer is
almost entirely a fixed request/response round trip. Note the device
is not on a real USB 2.0 bus at all — it hangs off `t2bce_vhci`, a virtual HCI
over the T2's PCIe buffer-copy engine, which is why the marginal cost per byte
is far below what high speed would imply and why the round trip dominates.

Solving the two measurements for a fixed per-request cost `f` and a
split-independent payload cost `p`:

    6f + p =  7.59 ms      f = 0.86 ms per request
   12f + p = 12.75 ms      p = 2.43 ms per full frame (149 MB/s marginal)

giving a cost model of `bands * 0.86 ms + 2.43 ms` for a full-frame flush:

| bound   | bands | model    |
|---------|-------|----------|
| none    | 1     | 3.29 ms  |
| 256 KiB | 2     | 4.15 ms  |
| 128 KiB | 3     | 5.01 ms  |
| 64 KiB  | 6     | 7.59 ms  |
| 32 KiB  | 12    | 12.75 ms |

Consequences:

- Band splitting is not free. The original single-request code was ~3.3 ms per
  full frame; 64 KiB bands cost ~7.6 ms. Still an easy trade against 169
  dropped frames per day, but it must not be described as free.
- Bigger bounds are strictly better for latency and for the time the CPU is
  held awake. 32 KiB was rejected for exactly this reason once measured.

The alternative fix (vmalloc + `usb_sg_init()` / `usb_sg_wait()`) would keep a
single transfer and need only order-0 pages, but `usb_sg_wait()` has no
timeout, so `APPLETBDRM_BULK_MSG_TIMEOUT` and the reset-on-`-ETIMEDOUT` logic
would have to be rebuilt on a timer. Given the latency finding above this is
now a more interesting option than it first appeared, but it is a much larger
change.

## Why the 128 / 64 / 32 KiB ladder

`PAGE_ALLOC_COSTLY_ORDER` is 3, i.e. **32 KiB**. At or below that the allocator
does not give up — it reclaims, swaps and OOM-kills before returning NULL.
Above it, allocations are "costly" and may simply fail. So only the last rung
is guaranteed; the ones above it can fail, cheaply and silently thanks to
`__GFP_NOWARN | __GFP_NORETRY`.

A fourth, smaller rung would be pointless: it could only run after a non-costly
allocation failed, at which point the machine is already dead.

The ladder was originally just `{64K, 32K}`. Adding the 128 KiB rung is
*strictly* never worse — if it fails the driver lands on exactly the previous
behaviour — and the failure-moment data (see below) says it succeeds ~81% of
the time even under the worst observed fragmentation. That turns a full frame
from 6 requests / 7.6 ms into 3 requests / 5.0 ms, expected ~5.5 ms.

Going further to 256 KiB was rejected: 45% success is a coin flip, and
performance that swings by 2x depending on how long the machine has been up is
a worse property than a slightly higher constant cost.

Note the row clamp means each rung allocates `min(bound, whole frame)`, so a
bound larger than the frame just yields the frame. Raising the bound is
therefore *not* a way to force the fallback in a test — the allocation call
itself has to be made to fail.

### Power / C-state angle

The driver blocks synchronously through all bands of a frame, so the bands are
one longer wake window, not several extra wakeups; the number of C-state exits
per frame is unchanged. Measured awake-time share:

| flush rate            | 64 KiB | 32 KiB |
|-----------------------|--------|--------|
| idle (0.12/s)         | 0.09%  | 0.15%  |
| active (3/s)          | 2.3%   | 3.8%   |
| animating (27/s)      | 20.5%  | 34%    |

Only the idle row matters for battery, and there the difference is negligible
because the Touch Bar barely redraws when nothing changes. The animating row is
significant but coincides with active use. The effective lever against it is
react-drm's flush rate, not the kernel buffer size.

### Verifying the fallback rung

Forced by temporarily making the first `kzalloc()` request `SZ_1G`
unconditionally: probe succeeded, 12.0 bands per flush (i.e. the 32 KiB rung),
display normal, and **zero** allocation-failure lines logged — confirming
`__GFP_NOWARN | __GFP_NORETRY` keeps the failed attempt silent and cheap.
Reverting restored 6.0 bands at 1.252 ms, matching the baseline exactly.

## What was actually free at the moment of failure

Every `page allocation failure` splat carries a full free-block dump, so the
journal is a record of the memory state at exactly the moments the driver
broke. Mining 183 surviving splats (`grep -oP 'Node 0 Normal: .*'`), counting a
request order as satisfiable if any free block of that order *or higher*
existed:

| order | block   | satisfiable | median free blocks |
|-------|---------|-------------|--------------------|
| 3     | 32 KiB  | 183/183 100%|               1650 |
| 4     | 64 KiB  | 183/183 100%|                416 |
| 5     | 128 KiB | 149/183 81.4%|                31 |
| 6     | 256 KiB |  83/183 45.4%|                 0 |
| 7     | 512 KiB |   5/183  2.7%|                 0 |

This is a worst-case-conditioned sample — these are by definition the bad
moments — which makes it the right basis for picking a bound.

Conclusions:

- 64 KiB is not merely "one order above the guaranteed line", it was
  satisfiable in **every** observed failure moment. The 32 KiB fallback is
  cheap insurance, not something the data shows is needed.
- 128 KiB would still have worked 81% of the time. A `{128K, 64K, 32K}` ladder
  is therefore *strictly* never worse than `{64K, 32K}` (a failed 128 KiB rung
  lands on exactly the previous behaviour) and cuts a full frame from 7.6 ms to
  5.0 ms in the common case. Expected cost ~5.5 ms. **This is what is
  implemented** — confirmed live with `drm.debug=2`, which logs
  `Using 727 rows per framebuffer request` at probe (was 363).
- 256 KiB and above is a coin flip and not worth the variance.

Note the free-block counts fluctuate a lot: at 23 h uptime there was 1 free
order-7 block, at 27 h there were 6 and `extfrag_index` was -1.000 for every
order up to 7. Any design whose performance depends on a high-order allocation
succeeding has performance that depends on how long the machine has been up.

## Alternatives considered and rejected

- **Allocate the whole frame once at probe, band-split only as fallback.**
  Restores the original 3.3 ms in the good case, one-constant change. Rejected:
  probe re-runs on every resume and every react-drm config switch (~5.6x per
  boot), and the table above says a 512 KiB request succeeds ~3% of the time
  under load — so the cheap path would mostly not be taken, and *when* it is
  taken would depend on uptime. Also pins a permanently unmovable 512 KiB
  region, which is itself a fragmentation contributor.
- **Single URB with `urb->sg` over order-0 pages.** Technically the best
  outcome: one round trip (~3.3 ms), no high-order allocation anywhere, no
  bands. `t2bce_vhci` even implements sg natively — the BCE hardware consumes
  an address/length segment list directly (`bce_vhci_urb_uses_sg`,
  `t2bce_core_set_next_submission_segment_list` in `transfer.c`). But its
  `hc_driver` never sets `sg_tablesize`, so `usb_bus.sg_tablesize == 0` and
  `usb_sg_init()` would split into one URB *per page* (86 of them) instead of
  one sg URB. Making this pay off needs a `t2bce_vhci` change too, and
  `usb_sg_wait()` has no timeout, so `APPLETBDRM_BULK_MSG_TIMEOUT` and the
  reset-on-`-ETIMEDOUT` path would have to be rebuilt on a timer plus
  `usb_sg_cancel()`. Upstream this would additionally still need the band path
  for HCDs without sg. Net: more code, not less — a follow-up optimisation, not
  the bug fix.
- **Pipeline: send all bands, then read all responses.** Would recover most of
  the per-request cost, but nothing documents that the device queues requests,
  and the footer timestamp matching suggests it expects one at a time. Not
  defensible upstream without device documentation.
- **Pack several damage rects into one bounded request.** *Implemented after
  all* — this was the one place the patch was a real regression: a multi-clip
  flush that used to be one request had become one request per rect (measured
  on tiny-dfr: 3.5 requests per flush where the old code sent 1, i.e. ~+2 ms).
  `appletbdrm_send_frame()` is now split into `appletbdrm_add_frame()` and
  `appletbdrm_send_frames()`, with `appletbdrm_frame_fits()` deciding when to
  flush. The invariant that makes it safe: `frames_capacity` is derived from the
  same allocation as `rows_per_request`, so a full-width band always fits into
  an empty buffer and the flush-then-add step can never loop. react-drm sees no
  change (its full-frame flush fills the buffer anyway); tiny-dfr is back to one
  request per flush.
- **No ladder at all — plain 64 KiB, fail probe on -ENOMEM.** The simplest
  possible patch, and the data says it would essentially never fail. Kept the
  fallback anyway because a failed probe here means no Touch Bar until the
  device is reset, and the ladder costs four lines. A reviewer may reasonably
  ask for it to go.

## Validated against tiny-dfr, not just react-drm

react-drm cannot exercise the multi-clip path at all (`partialFlush: false`), so
tiny-dfr 0.3.7 was built and run against the patched driver on 2026-08-04.
ftrace over the interactive test:

```
appletbdrm_primary_plane_helper_atomic_update    44 hits
appletbdrm_send_frame                           154 hits
```

Ratio 3.5, i.e. well below the 6 requests a full frame costs — that is the
proof partial damage was exercised: roughly 22 full redraws (6 each) mixed with
~22 small button updates (1 each). react-drm alone always yields exactly 6.0.
No allocation failures, no driver errors, no visual artifacts reported.

Two facts worth keeping:

- tiny-dfr's `SRC_X=0` and `SRC_W = mode width` mean damage never exceeds the
  60 px framebuffer width even though its dumb buffer is 64 px wide (pitch
  alignment). The request-size bound depends on exactly that.
- tiny-dfr's own clips are already "full-or-partial x range x a y range", i.e.
  the same shape the band split produces. A button clip is ~100-150 rows, well
  under `rows_per_request`, so those updates are one request each — byte for
  byte what the old code sent.

### Getting the Touch Bar into DRM mode without react-drm

Config 1 = firmware fn strip, config 2 = display/DRM interface (`5-6:2.1`).
Stopping react-drm runs its `ExecStopPost` detach helper, which returns the
device to config 1 — so `t2bdrm` has nothing to bind to and there is no card0.
`echo 2 > /sys/bus/usb/devices/5-6/bConfigurationValue` switches back, but the
first transfer after the switch times out (the firmware sleeps the display
interface when idle), probe fails with -110 and the driver's own reset worker
resets the device. After that, `echo 5-6:2.1 > /sys/bus/usb/drivers/t2bdrm/bind`
brings card0 up. This is pre-existing behaviour and worked as designed.

### stock tiny-dfr does not start on KaiT2en

Unrelated to this patch, but blocking for anyone pairing KaiT2en with tiny-dfr:
`find_backlight()` in tiny-dfr matches only `display-pipe`,
`228600000.dsi.0` and `appletb_backlight`, while `t2touchbar_bl.c` registers the
device as **`t2tb_backlight`**. tiny-dfr panics at startup with "No Touch Bar
backlight device found". Either rename ours back to `appletb_backlight` or get
the name added upstream to tiny-dfr.

## Unrelated fix folded in

`appletbdrm_flush_damage()` used to `goto end_fb_cpu_access` when
`drm_gem_fb_begin_cpu_access()` *failed*, calling `drm_gem_fb_end_cpu_access()`
on a framebuffer whose CPU access was never started. The restructured function
just returns instead.

## Open / not addressed

There is still no lock serializing the OUT endpoint. A frame now occupies
several messages, so a concurrent `appletbdrm_send_msg()` (e.g.
`appletbdrm_clear_display()` from `.atomic_disable`, or the reset worker) could
interleave between them. In practice both run from the atomic commit path,
which DRM serializes, and the same assumption already lets the single
per-device request buffer be shared — but the driver does not enforce it.

## Verifying a tester is actually running this build

A co-maintainer reported repeated `Failed to read response (-110)` /
`Actual size (16) doesn't match expected size (40)` with the bounded-request
branch, and had already switched from `make` to
`scripts/fedora/install-dkms-modules.sh`. He was still loading a different
module entirely.

Three fields of his `modinfo` gave it away, all checkable without touching his
machine:

| his modinfo | this repo |
|---|---|
| `version: 0.01` | no `MODULE_VERSION` in any commit on any branch |
| `author: 4f1sh3r` | `MODULE_AUTHOR("kait2en")` since the initial commit |
| `filename: /lib/modules/<ver>/updates/t2bdrm.ko` | DKMS installs `/lib/modules/<ver>/extra/t2bdrm.ko.xz` |

`git log --all -S'MODULE_VERSION' -S'MODULE_AUTHOR("4f1sh3r")'` (repo-wide, no
pathspec) returns nothing, and `t2bdrm.c` has only ever lived at
`modules/t2bdrm/t2bdrm.c`, so the loaded `.ko` was not built from this tree at
all. His file was also uncompressed and unsigned, which Fedora DKMS output is
not.

Why reinstalling did not help: `man 5 depmod.d` documents the built-in search
order as `"updates built-in"`, so `updates/` outranks `extra/`. A leftover from
an earlier `make install` (the module Makefile's `MODDEST` is
`/lib/modules/$(KVER)/updates`) permanently shadows the DKMS module. Rerunning
the installer changes what is on disk in `extra/` but never what modprobe
picks.

Cleanup is `rm -f /lib/modules/*/updates/t2bdrm.ko` + `depmod -a` + reinstall.

**Cheapest identity check for a remote tester:** the *absence* of the `version:`
line in `modinfo t2bdrm`. This driver defines no `MODULE_VERSION`, so any build
that prints one is not ours. `srcversion` is useless for cross-machine
comparison here: `scripts/mod/sumversion.c` hashes the source *and* every header
from the `.cmd` dependency list, so it changes with the kernel version.

## ad2d003 also dropped timeout recovery, not just kzalloc -> kvzalloc

`ad2d003` ("t2bdrm: use vanilla 7.1.5 mainline appletbdrm code", 2026-08-04)
replaced the KaiT2en t2bdrm with upstream's 7.1.5 appletbdrm. That is where the
`kvzalloc` allocation bug entered this tree, confirmed by the journal: the first
`Failed to send message (-11)` on this machine is Aug 04 18:25, nothing before.

Less obvious, and not mentioned in the commit: the old driver also had a
reset-after-timeout worker that upstream does not have. It logged

    [drm] send message timed out; scheduling USB device reset
    usb 5-6: appletbdrm: resetting device after timed out transfer

and then reset the USB device. Those lines appear in the journal only up to
Aug 04 and never again. `git log --all -S'scheduling USB device reset'` shows
the string only in `fd7359a` (initial commit) and `ad2d003` (removal). Both
`main` and `fix/t2bdrm-bounded-requests` are post-`ad2d003`, so neither has it.

Consequence: after a bulk timeout the IN stream stays desynced with no recovery
path. That matches a co-maintainer's MacBookPro15,1 report exactly (errors that
never stop, only spread further apart). Worth suspecting before blaming the
bounded-request patch on any machine that times out at all.

## The two failure modes are disjoint

Do not confuse `-11` with `-110`:

| | this machine (MacBookPro16,2), 47 boots | MacBookPro15,1 report |
|---|---|---|
| `-11` (EAGAIN, vmalloc DMA refusal) | 980 | 0 |
| `-110` (ETIMEDOUT) | 6, all reload/probe artifacts | many, read and send |
| `Actual size (16) doesn't match expected size (40)` | 0 | several |

The `Actual size (16) vs (40)` line is almost certainly fallout rather than a
cause: 40 is `sizeof(struct appletbdrm_fb_request_response)`, and after a bulk
read times out the device can still push data, so the next read picks up a
shifted remainder. Only the first unanswered read is worth investigating.

## Second failure report from the MacBookPro15,1: narrowing what can differ

Log (with 9f69e71, the response memset, already in):

```
 44.121226 Actual size (16) doesn't match expected size (40)
 47.515913 Failed to read response (-110)
 47.542809 Actual size (16) doesn't match expected size (40)
 48.604953 Failed to read response (-110)
 49.692221 Failed to send message (-110)      <- and every ~1.024 s from here
```

Vanilla mainline appletbdrm works on the same machine. That is the strongest
constraint available, and it rules out most of the patch.

### Only one of our changes is visible on the wire

Field by field, the bytes we put on the OUT endpoint are identical to what
vanilla produces for the same damage rectangle:

* `header.unk_00/unk_02/unk_04/size`, `unk_10`, `msg_id`, `unk_13[29]` are set
  the same way, and `unk_08`/`unk_13` stay zero because the buffer is
  `kzalloc`ed and those bytes are never written again
* the footer and the padding after it are re-zeroed on every reuse
* the frame geometry translation (`begin_x = y1`, `begin_y = height - x2`,
  `width = rect height`, `height = rect width`) is unchanged, it is just
  applied to a band instead of the whole damage rect - and a band is a legal
  sub-rectangle that vanilla would emit identically if userspace had damaged
  only that band
* `request_size = ALIGN(48 + frames_size + 80, 16)` is the same formula

Everything else in the patch (`drmm_kzalloc` response, `DRM_GEM_SHADOW_PLANE_FUNCS`,
the removal of `struct appletbdrm_plane_state`, the allocation moving to probe)
is kernel-side bookkeeping with no effect on what the device sees.

So the only wire-level change is: **an update larger than `rows_per_request`
rows is delivered as several requests instead of one**, each with its own
response read. Anything the device does differently must come from that.

### The 16 byte response fits "device rejects/short-acks a partial request"

`sizeof(struct appletbdrm_msg_response_header)` is 20, so a 16 byte answer does
not even carry the `msg` field. It is not a truncated `UDCL`; it looks like a
short status or error message. The sequence then explains itself:

1. first full-screen paint, request 1 of 3 -> device answers 16 bytes ->
   `-EBADMSG`, and requests 2 and 3 are never sent
2. the device is left mid-update and stops answering -> `-110` on the read
3. the 27 ms gap between the `-110` at 47.515 and the `16 vs 40` at 47.542 is
   the late answer to the *previous* request arriving: the IN stream is one
   response behind, which is exactly what an abandoned multi-request update
   produces
4. once the device stops draining OUT, every send times out - the ~1.024 s
   spacing is `APPLETBDRM_BULK_MSG_TIMEOUT` back to back

### The ZLP idea was considered and dropped

Both bulk endpoints are `wMaxPacketSize` 512 (high speed, 480 Mbps), and
`usb_bulk_msg()` never sets `URB_ZERO_PACKET`, so a request whose size is an
exact multiple of 512 would not be terminated by a short packet. For a
2170x60 panel the last band of a full update would be exactly 129024 = 252*512,
which is a tempting coincidence.

It is still probably wrong: request sizes track the damaged area, so arbitrary
sizes occur constantly on every machine, vanilla included. A missing ZLP would
therefore break vanilla occasionally too, and it does not. Do not chase this
before the split itself is ruled in or out.

### Our reference hardware

MacBookPro16,2, `Touch Bar Display` 05ac:8302, `bcdDevice` 0201, high speed,
bulk OUT `ep_01` / IN `ep_82`, both `wMaxPacketSize` 0x200. Panel 60x2008
(framebuffer 60 wide, 2008 tall). With the 128 KiB bound that is 727 rows per
request and a full update splits into 131008 / 131008 / 99872 bytes. Multi
request updates therefore *do* work here - the split is exercised on every
full-screen paint and 47 boots produced zero `Actual size` errors.

### "I still see the native esc key"

That is the device drawing its own fallback strip, which is what the Touch Bar
shows whenever the host is not painting it. It confirms the driver never got a
single frame through; it is not evidence that the driver half works.

### The knob to bisect this: `request_bound_kb`

`aae600f` adds `t2bdrm.request_bound_kb=<KiB>` (0 = the automatic
128/64/32 KiB ladder). At 512 the bound exceeds a full 60x2008 frame (361584
bytes), so `rows` clamps to the display width and every update becomes a single
request - wire-identical to vanilla, while all other changes stay in. It is
still a `kzalloc`, so the vmalloc/`-EAGAIN` problem does not come back.

* 512 works, 128 fails -> the split is the cause
* both fail -> the cause is not the split, and everything above is wrong

The same commit puts the transfer size and `actual_length` into the send/read
error messages, hex-dumps the first bytes of a short response, disambiguates
the two identically worded `Actual size` messages (`Actual response size` vs
`Actual sent size`), and logs the plan once at probe as
`Display 60x2008, 131008 byte requests of up to 727 rows (3 requests per full update)`.

## Local reference trace from the working MacBookPro16,2

Verified with `2f4833f` by `insmod`ing the freshly built module. Two things to
know before reloading `t2bdrm` by hand:

* stopping `react-drm` puts the Touch Bar back into function key mode, which
  switches the USB device to **configuration 1**. Interface `5-6:2.1` and with
  it the DRM interface only exist in configuration 2, so after `insmod` nothing
  binds until `react-drm` is started again. That is not a driver failure.
* `t2bdrm.ko` built out of tree is unsigned; this machine has Secure Boot
  disabled, so `insmod` works. The DKMS copy in `extra/` is signed.

Probe on this machine:

```
Bulk endpoints: in 0x82 maxp 512, out 0x01 maxp 512
GINF: 2008x60, 24 bpp, 6024 bytes per row, orientation 3, bitmap info 0x0, pixel format RGBA (0x52474241)
Display 2008x60, 131008 byte requests of up to 727 rows (3 requests per full update)
damage 60x2008+0+0, 2008 rows in bands of 727
tx #0: 131008 bytes (130872 frame bytes, 8 padding, 448 mod 512), msg_id 248
rx #0: 00 00 14 01 00 00 00 00 00 00 00 00 00 00 00 00 4c 43 44 55 00 00 00 00 00 00 00 00 18 00 00 00 f8 d8 a7 6e 10 2c 00 00
```

Notes on the reference data:

* **react-drm damages the full display on every single flush.** There is no
  partial damage in practice, so every update is 3 requests of
  131008 / 131008 / 99872 bytes. The multi request path is not an edge case
  here, it is the only path, and it works.
* the healthy response is `00 00 14 01`, twelve zero bytes, `UDCL` at offset 16,
  eight zero bytes, `0x18` at offset 28, then the echoed timestamp. A 16 byte
  answer is therefore exactly this response cut off right before `msg` - it
  looks like a truncated response, not a distinct error message.
* `bytes per row` is 6024 = 2008*3, i.e. reported in the device's own
  orientation. The driver ignores the field and derives the row size from
  `height`, which agrees.
* none of the request sizes are a multiple of 512 (448 and 32 remainder), which
  is consistent with the ZLP idea not mattering here, but does not test it.

### `request_bound_kb=512` needs `__GFP_RETRY_MAYFAIL`

A 512 KiB bound makes `rows` clamp to the display width, so a full update is a
single 361584 byte request - wire-identical to vanilla, while every other part
of the patch stays in. But 361584 bytes is an order-7 `kzalloc`, and plain
`GFP_KERNEL` gives up on costly orders: after 13 h of uptime the allocation
failed four times in a row. With `__GFP_RETRY_MAYFAIL` (only used when the
bound is forced) it succeeds immediately, and the trace confirms
`bands of 2008` and `tx #13: 361584 bytes`.

Still worth telling a tester to set the parameter via `modprobe.d` or the
kernel command line rather than `rmmod`/`insmod` after a long uptime.

## Reproducing a t2bdrm run without react-drm

Needed because the co-maintainer considers react-drm broken and will not run it,
while `t2bdrm` cannot even bind without it. Verified end to end on 2026-08-11.

The Touch Bar exposes the DRM interface only in **USB configuration 2**. In
configuration 1 there is a single interface `<bus>-6:1.0` (the HID fn strip) and
nothing for `t2bdrm` to bind to, so the module is never even loaded. react-drm
owns that switch: it attaches at startup and `react-drm.service` `ExecStopPost`
runs `react-drm-tb-detach`, which writes `0` then `1` to
`bConfigurationValue`.

The switch is only a sysfs write, so it can be done by hand in the other
direction:

```sh
echo 0 | sudo tee /sys/bus/usb/devices/<bus>-6/bConfigurationValue
sleep 1
echo 2 | sudo tee /sys/bus/usb/devices/<bus>-6/bConfigurationValue
```

`<bus>-6:2.0` and `<bus>-6:2.1` appear, `t2bdrm` binds and probes, and
`Initialized t2bdrm` shows up in dmesg. That alone yields the `Bulk endpoints`,
`GINF` and `Display ...` lines.

It does **not** yield any framebuffer traffic: the driver has no fbdev emulation
(`DRIVER_MODESET | DRIVER_GEM | DRIVER_ATOMIC`, no `drm_fbdev_shmem_setup`), so
without a DRM client nothing is ever flushed. `modetest` from Fedora's
`drm-utils` is enough:

```sh
sudo modetest -M t2bdrm -c                  # connector id and mode
sudo modetest -M t2bdrm -s 39:60x2008       # holds until interrupted
```

This produces exactly the same full-display damage react-drm produces
(`damage 60x2008+0+0`, three requests of 131008 / 131008 / 99872 bytes) and
fires the whole trace. It is the minimal reproduction and involves no
KaiT2en userspace at all.

### Reading `modinfo` as the version check

Since `aae600f` the module has a parameter, which makes identification trivial
and needs neither root nor a loaded module:

```
parm: request_bound_kb:Upper bound for a single framebuffer request in KiB (0 = automatic) (uint)
```

No `parm:` line means an older build, whatever else matches. This retires the
`/proc/kallsyms` symbol counting entirely - and note that the symbol check
returns `0 0` whenever the module is simply not loaded, which is the normal
state in configuration 1 and says nothing about the build.

## Root cause: a request that is an exact multiple of wMaxPacketSize wedges the device

Found 2026-08-11, reproduced on our own hardware, fixed in `4579ed5`.

The MacBookPro15,1 panel is **2170x60**, ours is 2008x60:

```
GINF: 2170x60, 24 bpp, 6510 bytes per row, orientation 3, bitmap info 0x0, pixel format RGBA
Display 2170x60, 131008 byte requests of up to 727 rows (3 requests per full update)
```

With the 128 KiB bound both machines use 727 rows per request, but the
remainder differs:

| width | bands | request sizes | mod 512 |
| --- | --- | --- | --- |
| 2008 (ours) | 727 / 727 / 554 | 131008 / 131008 / 99872 | 448 / 448 / 32 |
| 2170 (his)  | 727 / 727 / **716** | 131008 / 131008 / **129024** | 448 / 448 / **0** |

129024 = 252 * 512. `usb_bulk_msg()` never sets `URB_ZERO_PACKET`, so a
transfer that is an exact multiple of the endpoint's `wMaxPacketSize` is not
terminated by a short packet: the device keeps waiting for more data, never
answers, and everything after that times out. Vanilla escapes it only by
accident - its single request for a 2170 wide panel is 390752 bytes, 96 mod 512.

### Why the earlier dismissal of this idea was wrong

The ZLP hypothesis was raised and rejected once with the argument that request
sizes track the damaged area, so arbitrary sizes occur on every machine and
vanilla would break occasionally too. **That premise is false.** react-drm
damages the full display on every single flush, so the sizes are not arbitrary
at all - they are the same two or three values on every frame, fixed by the
panel width. A panel whose remainder band lands on a multiple fails on every
frame; a panel whose does not, never fails. That is exactly the observed
"works here, never works there" pattern.

### Reproduced locally

`request_bound_kb=126` makes a 2008x60 panel produce 716 row bands, i.e. the
same 129024 byte request:

```
tx #0: 129024 bytes (128892 frame bytes, 4 padding, 0 mod 512), msg_id 58
*ERROR* Failed to read response (-110), 0 of 40 bytes read
*ERROR* rx #0 for a 129024 byte request failed
```

`0 of 40 bytes read` - the device sends nothing at all, which also explains the
16 byte reads seen earlier as a stale answer to an earlier request arriving late
on a stream that is one response behind.

### The fix

`appletbdrm_send_frames()` grows the request by 16 bytes when it would be an
exact multiple of the OUT endpoint's `wMaxPacketSize`. The padding after the
footer is already variable (4, 8, 12 bytes observed), it is covered by
`header.size`, and the memset that clears the footer area clears it too, so
this is protocol-legal. `appletbdrm_alloc_request()` reserves those 16 bytes so
the buffer cannot overflow.

Verified on hardware with `request_bound_kb=125`, whose remainder band is
105984 bytes = 207 * 512:

```
tx #5: 106000 bytes (105852 frame bytes, 20 padding, 16 mod 512), msg_id 133
rx #5: 00 00 14 01 ... 4c 43 44 55 ... UDCL
```

The device accepts 20 bytes of padding without complaint. The default bound is
unchanged for a 2008 wide panel (727 rows, 131008 / 99872 as before).

A cleaner fix would be a hand rolled URB with `URB_ZERO_PACKET` instead of
`usb_bulk_msg()`; worth considering before this goes upstream, since padding
relies on the device ignoring trailing bytes while a ZLP does not change the
message at all.

## Superseded: scatter-gather removed the need to split

2026-08-16, branch `fix/t2bce-t2bdrm-scatter-gather`. Full detail in
`local/learnings/t2bce/vhci-sg.md`; this section records what it means for the
conclusions above.

The KaiT2en project owner's verdict on the padding was "wrong approach, the vhci
devices have to be converted to sg-dma, that solves all the problems". On
direction he was right, and it turned out to be reachable: `t2bce_vhci` now
advertises sg and builds BCE segment lists per URB, so `t2bdrm` allocates the
request with `vzalloc()` and hands the page list straight to
`usb_sg_init()`/`usb_sg_wait()`.

What that retires from this document:

- **The high-order allocation problem** is gone, not worked around. A
  page-fragmented buffer is now the *normal* case rather than the failure case,
  so `request_bound_kb`, the 128/64/32 KiB ladder, the row-band split and the
  reuse-across-frames machinery all have no reason to exist.
- **The wMaxPacketSize padding (`4579ed5`) is not needed.** It only ever fixed a
  bug our own splitting created: with one request per frame the sizes are 361584
  (2008 px) and 390752 (2170 px), neither a multiple of 512. The 2170-wide panel
  failed because *its* third band was exactly 129024 = 252 × 512.
- **The bounce buffer ("Jeff Bridges") on `fix/t2bdrm-fragmented-memory`** stays
  rejected, and for a reason that reads better in hindsight: it required chunk
  sizes to be multiples of `wMaxPacketSize` — the exact mirror image of the bug
  that later killed the bounded-request branch on the MacBookPro15,1.

One thing the padding analysis got right and sg does **not** fix: `usb_sg_init()`
sets only `URB_NO_INTERRUPT` (plus `URB_SHORT_NOT_OK` for IN), never
`URB_ZERO_PACKET`. The short-packet gap is still there; sg removes the *trigger*
by removing the split, it does not close the gap. `appletbdrm_send_sg_request()`
therefore still sends an explicit `usb_bulk_msg(NULL, 0)` when
`size % max_packet == 0`.

Also corrected by this work: the observation at "Reproduced locally" that a
16-byte read is "a stale answer to an earlier request arriving late on a stream
that is one response behind" was exactly right, and the one-response-behind state
is permanent once entered. `appletbdrm_read_update_response()` now detects it by
timestamp, discards the stale response and re-reads, which is the recovery this
document never had.

Latency, ftrace over four full-frame updates: sg send 0.755 ms, response read
2.121 ms, total **2.876 ms** — against ~3.3 ms for the old single linear transfer
and ~5.0 ms for three bounded requests. This also revises "Transfers are
latency-bound, not bandwidth-bound": that is still true, but the latency now sits
almost entirely in the device's response (74 % of the frame), not in the host
side of the transfer.

### Memory-pressure verification (2026-08-16)

The whole point of this document was that a 361584-byte physically contiguous
allocation is an order-7 request that fails once memory is fragmented — 169
`page allocation failure` splats in the original report. That failure mode is
now gone, and it was verified under conditions at least as hostile as the ones
that produced it.

Method: `react-drm` stopped, a throwaway libdrm client
(`scratchpad/tbgen.c` — dumb buffer, `drmModeSetCrtc`, then `drmModeDirtyFB`
over the full 60x2008 surface in a loop) drove the Touch Bar at 60 fps for
200 s while `stress-ng` ran in a `systemd-run --scope` with `MemoryMax` set, so
that an OOM kill lands inside the scope instead of on the session. Every pixel
is rewritten each iteration so no damage-clip shrinking applies — each frame is
a full-surface transfer through the sg path.

| | |
|---|---|
| frames submitted | 11851 |
| `drmModeDirtyFB` errors | 0 |
| avg / worst per frame | 3.118 ms / 127.5 ms |
| `page allocation failure` | 0 |
| t2bdrm/t2bce/drm lines in dmesg during the run | 0 |
| order-7 blocks in `Normal` at zero | 12 of 97 samples |

The last row is what makes the run meaningful: for 24 s of the test an order-7
`kzalloc` had **no** block to satisfy it in the `Normal` zone, and orders 8-10
were at zero far longer. That is precisely the state that produced the original
splats. Nothing was logged. The 127.5 ms worst case is a scheduling stall while
the OOM killer ran, not a driver error — the transfer itself completed.

The whole kernel log for the run contained nothing but the two memcg OOM reports
for `stress-ng` itself. Phase 2 was killed by the cgroup OOM killer (rc=137),
which is the intended outcome: the pressure was real enough to run out.

Not covered by this: the sg path allocates with `vzalloc()` and
`sg_alloc_table()`, which can still return `-ENOMEM` under true exhaustion. The
test shows the *high-order* failure is retired, not that allocation cannot fail
at all.
