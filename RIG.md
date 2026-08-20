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

### t2bce/t2bdrm scatter-gather

- Source: `fix/t2bce-t2bdrm-scatter-gather` (`efd0f67`, `fa2a090`, `240003e`)
- Touches: `t2bce_core`, `t2bce_dma`, `t2bce_vhci`, `t2bdrm`
- What it does: sends Touch Bar framebuffer updates as chunked
  scatter-gather transfers instead of one linear buffer.
- Why it is here: it is the display path for the Touch Bar on this machine
  and needs real-world runtime before it can be defended upstream.
- Why not upstream yet: offered to the t2linux Discord for testing on
  2026-08-16. Nobody has picked it up so far. Follow up on 2026-08-21; the
  branch is only worth carrying while it is waiting for outside testers.

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
  `80cc852`), a rewrite of the same five source files. It is deliberately
  *not* taken here — the whole `apps/t2-fan-control/` directory, README
  included, is resolved to the `local/rig` side on every `merge main`, so this
  app stays pinned to the pre-v0.05 base the bias work was written against.
  `rerere` has that resolution recorded. Revisit by rebasing the bias work
  onto v0.05 once the approach has proven itself; until then upstream's
  fan-control changes do not reach this machine.

## Not merged, deliberately

- `feat/t2bce-vhci-sg-tablesize` (`6aa98ce`) — redundant. Its
  `sg_tablesize` change is already contained in the scatter-gather branch.
