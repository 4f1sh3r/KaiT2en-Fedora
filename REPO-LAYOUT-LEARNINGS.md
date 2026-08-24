# Repo layout: moving the installer tests

Branch: `chore/move-installer-tests-to-scripts` (pushed to `origin` = 4f1sh3r fork, no PR).

## What moved

`tests/installer/` -> `scripts/tests/` (11 bash scripts). The top-level
`tests/` directory is gone.

## Why the target depth matters

Every one of the 11 scripts resolves the repo root the same way:

```sh
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
```

The `../..` hardcodes "two levels below the repo root". `scripts/tests/` is
also two levels deep, so all 11 scripts work unchanged. `scripts/tests/installer/`
would have been three levels deep and required editing all 11 files.

Net diff: 23 lines in 2 files, plus 10 pure renames git detects on its own.

## Everything that referenced the old path

Only two files, which is why the move is cheap:

- `scripts/tests/static-check.sh` — the `shell_files` array (11 entries) and
  the 10 `bash tests/installer/...` invocations at the end.
- `.github/workflows/installer.yml` lines 40 and 47.

Nothing in `docs/`, `README.md`, `mkdocs.yml`, `packaging/` or `.gitignore`
referenced `tests/`, and the CI workflow has no path filter on `tests/**`.

`packaging/installer/build-in-container.sh` copies *individual named files*
out of `scripts/` (only `scripts/macos/prepare-fedora-installer.sh` and
`scripts/macos/download-fedora-iso.sh`), never the whole tree — so putting
test code under `scripts/` cannot leak it into a shipped installer kit.

## Verification

`bash scripts/tests/static-check.sh` passes end to end (exit 0) with
shellcheck present at `/usr/bin/shellcheck`, so the shellcheck stage really
ran instead of being skipped. Individual scripts were also confirmed to work
when invoked from an unrelated cwd, which is what actually exercises the
`repo_root` resolution.

## Conflict cost against in-flight branches — measured, not guessed

Initial estimate was that this would cause manual conflict work across the
7 branches that touch `tests/installer`. Measuring it with
`git merge-tree --write-tree` against both `main` (baseline) and the rename
branch showed that estimate was wrong. The rename adds almost nothing:

| Branch | conflicts vs main | vs rename branch |
|---|---|---|
| `pr-39-dkms-require-kernel-headers` | none | none |
| `upstream/pr/39` | none | none |
| `feat/brcmfmac-pm-max-dkms` | 2 | 2 (same ones) |
| `fix/react-drm-installer-healthcheck` | 1 | 2 |
| `feat/driver-disc` | 17 | 15 |

The lesson: `git merge-tree --write-tree <base> <branch>` reports conflicts
without touching the working tree, and comparing it against the *baseline*
separates conflicts the rename actually causes from ones the branch already
had. Do this before arguing about rename timing.

- `feat/brcmfmac-pm-max-dkms` conflicts on `static-check.sh` either way,
  because it edits the same array the rename rewrites. The rename changes
  nothing about the effort.
- `fix/react-drm-installer-healthcheck` is the only branch the rename makes
  worse, by exactly one item: it adds a new `tests/installer/react-drm-service.sh`,
  and git raises a "file location" conflict that names the fix
  (`scripts/tests/react-drm-service.sh`).
- `feat/driver-disc` is deeply conflicted with `main` on its own (README,
  `howto/`, 10 add/add hits in `packaging/`). The rename *reduces* its count,
  because rename detection resolves the two `tests/installer` add/add
  conflicts. That branch's problem is unrelated to this move.

## GitHub reported a conflict that a local rebase did not have

After PR #39 landed on `main`, GitHub flagged PR #51 as conflicting in
`tests/installer/static-check.sh`. The cause: commit `58923d0` inserted
`scripts/fedora/lib.sh` into the `shell_files` array on the line directly
above the block this rename rewrites. Overlapping hunk context turns what
would otherwise be an automatic rename/modify merge into a text conflict.

Notable: `git rebase upstream/main` resolved it locally with no conflict at
all. Git's rename detection handles a whole-directory rename plus an adjacent
edit; GitHub's merge check is more conservative and reported a conflict for
the same pair of commits. So "GitHub says conflict" does not always mean
there is manual work to do — try the rebase before assuming.

Rebasing (not merging) is the right fix for this branch: the value of the
change is that it stays a single pure-rename commit sitting on current `main`,
which is exactly what keeps rename detection working for everyone else.
Requires `git push --force-with-lease`.

## Open decision

Whether to land this before or after the open installer PR. The measurement
above says the ordering barely matters mechanically. The remaining argument
against the move is semantic, not technical: `scripts/` otherwise holds code
that runs on *user* machines during install, while these tests are CI-only
dev tooling, and `tests/` at repo root is the convention contributors expect.
