# COPR kernel experiments

This directory contains the GitHub Actions tooling for patched Fedora kernel
experiments. It is independent of the local helper in
`scripts/fedora-kernel-build-script/` and is not a replacement for the normal
Fedora kernel used by KaiT2en installations.

Each experiment lives in `patches/testing/<name>/` and contains:

- a `README.md` describing its intent, target hardware, and risks;
- a `series` file defining patch order;
- every `.patch` file named by `series` exactly once.

An internal pull request that changes one experiment runs
`.github/workflows/kernel-copr.yml`. GitHub Actions creates an SRPM from
Fedora's official kernel SRPM and submits it to the configured COPR project.
The workflow updates one marked pull-request comment and the job summary with
the build URL and, after success, exact install and removal commands.

The same experiment can be selected manually by directory name with
`workflow_dispatch` after the workflow is present on the repository's default
branch. The temporary fork rollout defaults to
`4lex404/kait2en-kernel-testing`; the repository variable `COPR_PROJECT` can
override that target. The repository environment `copr` must contain a
`COPR_CONFIG` secret holding the complete personal COPR API configuration.

Create the temporary personal project once:

```bash
copr-cli create kait2en-kernel-testing \
  --chroot fedora-44-x86_64 \
  --description "Patched test kernels for T2 Macs. Testers only, not for production."
```

The workflow sets the five-hour build timeout per submission. COPR's
project-level `--delete-after-days` option deletes the project itself and is
therefore intentionally not used as per-build retention.

Fork pull requests never receive the COPR credential. To build automatically,
put the experiment on a branch in the repository that owns the workflow or use
a manually reviewed dispatch. A pull request must change exactly one
experiment. Experiment names use lowercase letters, digits, and hyphens. The
kernel suffix replaces hyphens with underscores and appends the short source
commit ID.

`configs/t2-disable.list` belongs to this CI pipeline. It reduces the Fedora
kernel configuration for the x86_64 T2 test build; the local kernel builder
does not consume or maintain it.

