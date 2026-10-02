# local/rig — the integration branch for this machine

`local/rig` is the only branch this Mac installs from. It is `main` plus a
merge of every topic branch that is currently under test here. It never
becomes a pull request, and nothing is ever merged *out of* it.

Everything below must stay true: when an entry lands upstream or gets
dropped, remove it here and unmerge it.

## Workflow

Update:

    git checkout main && git pull upstream main
    git checkout local/rig && git merge main

Conflicts recur across updates, so `rerere.enabled` is on — a resolution
recorded once is replayed automatically.

Install (always from `local/rig`, never from a topic branch):

    sudo scripts/fedora/install-dkms-modules.sh     # kernel modules
    sudo scripts/fedora/install-apps.sh             # userspace apps
    sudo scripts/fedora/install-apps.sh --react-drm-only

App updates and module updates are independent: `--react-drm-only` does not
touch the DKMS stack.

Adding something new to `local/rig` is never automatic — it is asked about
and decided case by case.

## What is merged right now

### t2-fan-control platform bias

- Source: `feat/fan-platform-bias` (`eb7f7bb`)
- Touches: `apps/t2-fan-control`
- What it does: biases the fan curve using skin temperature and battery
  heat soak instead of package temperature alone.
- Why it is here: an experiment to find out whether this approach is viable
  at all.
- Why not upstream yet: downstream-only by intent. It goes upstream only if
  the approach proves itself here first.
- Upstream drift: upstream released t2-fan-control v0.05 (`3935b19`,
  `80cc852`) and has since kept building on it (`5cf6467` advanced options,
  `bc13a73` fewer idle wakeups), a rewrite of the same five source files. It is
  deliberately *not* taken here — the whole `apps/t2-fan-control/` directory, README
  included, is resolved to the `local/rig` side on every `merge main`, so this
  app stays pinned to the pre-v0.05 base the bias work was written against.
  `rerere` has that resolution recorded. The 2026-09-01 merge added packaging drift
  on top of the code drift: upstream's branding pass redrew the app icon and its
  `Makefile` grew a `check-build` guard against installing without a prior
  `make build`. Neither is taken here, so this app keeps the old icon while every
  other app got the new one.
- Config trap: `/etc/t2-fancontrol/config.txt` holds the bias keys and has no
  `config_version` line. v0.05 discards any config whose version does not match
  and rewrites the file from its own defaults, so running an upstream install
  (e.g. `kait2en-install`) destroys the file — the binaries round-trip, the
  config does not. A copy is kept at `/etc/t2-fancontrol/config.txt.rig-backup`.
  Back it up before letting any upstream install touch this machine. Revisit by rebasing the bias work
  onto v0.05 once the approach has proven itself; until then upstream's
  fan-control changes do not reach this machine.

## Landed upstream

- t2-touchbar three-finger brightness swipe — merged into `main` via PR #88
  (`ba5f000`, carrying `34ccd65` unchanged). Comes in through `merge main`
  now; local topic branch deleted.
- t2smc native battery charge limit — upstream now; `modules/t2smc/t2smc.c`
  is byte-identical to `upstream/main` (verified 2026-09-21). The code comes
  from upstream, so this branch no longer diverges here.
- t2bce/t2bdrm scatter-gather — merged into `main` on 2026-09-02 via PR #72
  (`3d0276d`, `47e8cd4`), identical to what this branch carried. Unmerged here
  on the same day; `apps/t2-fan-control` is now the only divergence from `main`.

## Not merged, deliberately

- `feat/t2bce-vhci-sg-tablesize` (`6aa98ce`) — redundant. Its `sg_tablesize`
  change is contained in the scatter-gather work that is now upstream.

## Open questions

### t2bce_audio: branch `t2bce-audio-troubleshoot`

Four fixes on that branch (`dac56f9`, `8958562`, `c111a90`, `b6f7ab0` — post-resume
pointer interpolation, frame capping, erase-head margin) are in neither `main` nor
`local/rig`. Meanwhile `main` grew its own attempt at the same clock problem
(`52fe962`, `ff8b927`), so merging the branch would be a semantic conflict, not just
a textual one.

Decided 2026-08-20: judge `main`'s version by ear first — headphone output, xruns,
and audio after suspend/resume. Merge the branch only if `main`'s approach actually
falls short.
