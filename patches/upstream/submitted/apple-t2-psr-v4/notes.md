# Apple T2 eDP Panel Self Refresh v4

- Status: submitted
- Version: v4
- Date: 2026-10-16
- Base: drm-tip `5fe1ae49f9cd4c3e98bbf07d4b44adf672eb7e8b`
- Tested: MacBookPro16,2
- Message-ID: `<20261016140707.1900947-1-alexander.fischer@kait2en.org>`
- In-Reply-To: `<20260916072558.147343-1-alexander.fischer@kait2en.org>` (v3, threading confirmed on lore)
- Link: https://lore.kernel.org/all/20261016140707.1900947-1-alexander.fischer@kait2en.org/
- Series: https://patchwork.freedesktop.org/series/173055/ (now version 4)

## Changes from v3

- Rebased onto drm-tip. `intel_psr.c` was converted to the
  `drm_dp_dpcd_write_byte()` helper upstream in September (`writeb()` still
  exists but the file no longer uses it); the four new Apple DPCD writes now
  use the same helper.
- No functional changes.

## Context

v3 (2026-09-16) has had no review from the Intel maintainers since submission;
Lukas Wunner reviewed v1 and v2 and both rounds of his feedback are already
incorporated. The PSR core has moved underneath the patch since (helper
conversion, joiner/Panel Replay work by Mika Kahola, async-flip frame-change
discussion), so v4 resends on a current base instead of a bare ping.

## Validation

- `git apply --check` against drm-tip `5fe1ae49`: passed (v3 failed there on
  the two `_psr_enable_sink` hunks).
- `git mailinfo` (what `git am` stores): the commit message starts at
  "Commit 1035f4a65f58 ..." and ends at `Signed-off-by`; the changelog below
  the `---` fold is stripped.
- `perl scripts/checkpatch.pl --strict`: 0 errors, 0 checks; the single
  "Unknown commit id 1035f4a65f58" warning is a false positive of the shallow
  drm-tip clone (the commit exists, titled "drm/i915: Disable PSR in Apple
  panels", by José Roberto de Souza).
- Compile test from a clean `git archive` of the v4 commit: `defconfig` +
  `EXPERT`, `DRM_I915=y`, `DRM_XE=m`, `DRM_XE_DISPLAY=y`, `DRM_I915_WERROR`,
  `DRM_XE_WERROR`, built with `W=1`. `intel_psr.o` and `intel_quirks.o` for
  both i915 and xe (`xe/i915-display/`) compile with zero warnings.
- Source commit in the drm-tip clone: `12d6ef481` on branch `psr-apple-v4`.

## Sending

Sent 2026-10-16 16:07 +0200 via `git send-email` from the repo root with
`--in-reply-to` set to the v3 Message-ID; `--to`/`--cc` as listed under
Recipients below.

## Recipients

To:

- intel-gfx@lists.freedesktop.org

Cc:

- intel-xe@lists.freedesktop.org
- dri-devel@lists.freedesktop.org
- linux-kernel@vger.kernel.org
- Lukas Wunner <lukas@wunner.de>
- Jani Nikula <jani.nikula@linux.intel.com>
- Rodrigo Vivi <rodrigo.vivi@intel.com>
- Joonas Lahtinen <joonas.lahtinen@linux.intel.com>
- Tvrtko Ursulin <tursulin@ursulin.net>
- David Airlie <airlied@gmail.com>
- Simona Vetter <simona@ffwll.ch>
- José Roberto de Souza <jose.souza@intel.com>
- André Eikmeyer <andre.eikmeyer@kait2en.org>
- Yossef Osman <yossef.osman@proton.me>
