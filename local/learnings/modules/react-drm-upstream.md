# react-drm upstream integration learnings

## How react-drm is integrated (as of 2026-08-19)

`apps/react-drm/` is a **vendored copy** of
<https://github.com/dev-muhammad-adel/react-drm-for-touchbar> — no submodule,
no subtree, no upstream history. It arrived as two squashed manual drops:
`2ea7052` (KaiT2en startup animation) and `1bf72ce` (update + config-gui).

- Vendored base before this work: upstream **`e80d2b8`** (2026-08-14,
  "config-gui Electron editor").
- Upstream `85f11f7` (graphical installer, `install-gui/`) is deliberately
  **not** taken. KaiT2en installs via `scripts/fedora/install-apps.sh`
  (`--react-drm-only`); `apps/react-drm/install.sh` is only a 30-line wrapper
  around it. Upstream's own `install.sh`/`uninstall.sh` are fully replaced.
- `apps/react-drm/node_modules/react-drm` is a symlink to `..`, so the control
  center imports the parent package. A stale parent `dist/` makes
  `tsc -p linux-touchbar-control-center` report bogus "no exported member"
  errors — always run `npm run build:ts` in `apps/react-drm` first.

### Our local deltas against upstream

| Area | Files |
|---|---|
| KaiT2en installer/docs | `install.sh`, `README.md`, `system/99-react-drm.rules`, `system/react-drm-config-gui.desktop` |
| KaiT2en boot animation | `App.tsx`, `components/BootScreen.tsx`, `lib/hooks/useBootSequence.ts`, `public/kait2en-wordmark.png` |
| Hardware fixes | **new** `src/native/hardware.ts` (+ export in `src/index.ts`), `src/renderer/renderer.ts`, `native/src/udev_keyboard.{cpp,h}`, `native/src/binding.cpp`, `src/native/binding.ts`, `src/native/keyboard.ts` |
| Extra | **new** `gnome-extension/window-monitor-pro/` |
| Misc | `useMediaPlayers.ts`, `useAlbumArt.ts`, `lib/services/suspend.ts`, `lib/activeWindow/plasma.ts`, `layers/mediaScreen.tsx`, `config.blueprint.ts` |

## Upstream master does not build standalone

Upstream `lib/services/brightness.ts` imports `DISPLAY_BACKLIGHT_NAMES` from
`'react-drm'`, but upstream ships **no** `src/native/hardware.ts` and does not
export that symbol from `src/index.ts` — that file exists only in our copy.
Upstream master therefore only typechecks inside the KaiT2en fork. Worth
reporting to the maintainer.

## Slider update (upstream 4b2a04e + 5be8e8a), imported 2026-08-19

Imported as 13 files; the import was **conflict-free** because the maintainer
had already upstreamed both of our slider deltas:

- our serialized volume write queue against the audio crackling
  (`applyVolume(pct, done)` + `toFixed(4)`) → now `lib/services/volume.ts` and
  `lib/hooks/useVolume.ts`;
- our `DISPLAY_BACKLIGHT_NAMES` lookup instead of a hardcoded candidate list →
  now `lib/services/brightness.ts`.

`splittedLayer.tsx`, `components/BackButton.tsx` and `src/components/Button.tsx`
were byte-identical to the base, so overwriting them was safe. Verify that with
`git show <base>:<file> | diff - <file>` before any future manual drop.

## Subtree conversion (prepared 2026-08-19, branch `chore/react-drm-subtree`)

Converted **without** the usual `git rm -r` + `git subtree add` dance, which
would have produced a delete-everything/re-add diff. Instead the join was
recorded directly as an `ours` merge, which leaves the tree byte-identical:

```sh
git merge -s ours --no-commit --allow-unrelated-histories react-drm-upstream/master
# commit with these trailers -- this is what makes `git subtree` find the join:
#   git-subtree-dir: apps/react-drm
#   git-subtree-mainline: <our HEAD before the merge>
#   git-subtree-split: <upstream sha>
```

Verified, not assumed:

- `git subtree pull` against the unchanged upstream reports "Already up to
  date", i.e. the handcrafted join is recognised.
- Against a simulated upstream commit it does a real 3-way merge: a file we
  had patched (`useMediaPlayers.ts`) kept our delta *and* gained the upstream
  line, with no conflict.
- `install-gui/` comes back as a modify/delete conflict on every pull that
  touches it. That friction is by design and visible, not silent.

Constraints that must hold:

- `git-subtree` is **not** installed by default on Fedora
  (`sudo dnf install git-subtree`).
- The join was made without `--squash`, so every future pull must also omit
  it; mixing the modes breaks the merge base.
- The update PR must be merged as a **merge commit**. A squash merge drops the
  join commit and the next pull loses its base.

## Future integration (open, needs project owner decision)

`git subtree` is preferred over a submodule: we carry real local code changes,
which a submodule structurally cannot hold without a separate fork repo, and
`install-apps.sh` tars `apps/react-drm` (submodules are missing from tarball
releases and need `--recurse-submodules` everywhere). Subtree keeps the single
clone and gives real 3-way merges instead of manual copy-paste.
