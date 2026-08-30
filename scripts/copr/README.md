# COPR kernel experiments

Each experiment lives in `patches/testing/<name>/` and contains:

- a `README.md` describing intent, target hardware, and risks;
- one or more patches named `0001-*.patch`, `0002-*.patch`, and so on.

Without a `series` file, patches are applied in bytewise filename order. An
optional non-empty `series` overrides that order and must list every `.patch`
file exactly once.

By default, an experiment uses the latest kernel source package from Fedora's
stable `fedora` and `updates` repositories. To select another Fedora kernel, add an
optional `kernel-release` file containing exactly one of:

- a kernel series such as `7.2`, resolved to the newest matching Koji build for
  the Fedora release used by the builder;
- an exact release such as `7.2.0-200.fc44.x86_64`, downloaded directly from
  Fedora Koji for a reproducible build.

An explicit release must exist in Koji. The workflow never silently falls back
to the stable kernel or to a build for another Fedora release.

An internal pull request that changes exactly one experiment starts
`.github/workflows/kernel-copr.yml`. README-only changes are ignored. The
workflow builds a patched Fedora kernel in COPR and updates one PR comment with
the build result and exact installation and removal commands.

Experiment names use lowercase letters, digits, and hyphens. The RPM suffix
uses underscores instead of hyphens and includes the short source commit ID.

The repository environment `copr` must contain the complete COPR API
configuration as the `COPR_CONFIG` secret. `COPR_PROJECT` can be set as a
repository variable and defaults to `4lex404/kait2en-kernel-testing` during the
fork rollout. Fork pull requests do not receive the credential and are not
built automatically.

After the workflow is present on the default branch, an experiment can also be
selected manually with `workflow_dispatch`. Its optional `kernel_release`
input overrides the experiment's `kernel-release` file for that run. Leaving
the input empty preserves the experiment setting and then the stable default.
