# `local/` — everything that never reaches `main`

This tree exists only on the `local/rig` branch. `main` has no idea these
paths exist, so a `git merge main` can never conflict here, and a topic
branch cut from `main` never carries this tree along.

Anything that belongs to the project rather than to this machine does *not*
live here — it goes onto a topic branch and upstream through a pull request.

## Layout

- `learnings/` — the working notes, one directory per area (`t2bce/`,
  `modules/`, `installer/`, `kernel/`, `power/`, `repo/`). Written in English.
  A new topic branch gets its own file in the matching directory.
- `experiments/` — measurement rigs and throwaway test builds. `c-states/`
  holds the A/B power scripts; every one of them resolves its siblings
  relative to its own location, so the directory has to move as a whole.
- `upstream/` — mail artifacts while they are still being prepared or
  investigated, split into `sent/`, `pending/`, `drafts/` and `research/`.
  Once a series is actually sent it also belongs in
  `patches/upstream/submitted/<series>-vN/` on `main`, with a `notes.md`
  carrying its status, date and Message-ID. Unsent drafts, raw traces and
  measurements stay here only.
- `tools/` — helper scripts for this machine (`sync-to-mac.sh` for the case
  that work happens from a separate development box, `switch-brcmfmac-pm.sh`).

## What is deliberately not tracked

Some files under `learnings/` and `upstream/` are untracked on purpose.
Leave them that way; do not `git add` them to tidy up the status output.
