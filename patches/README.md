# Patch layout

- `runtime/` contains patches applied by installation and build scripts.
- `testing/` contains named COPR kernel experiments built by GitHub Actions.
- `upstream/` contains complete mail artifacts being prepared or already sent.
- `archived/` contains merged, withdrawn or superseded patches kept for
  reference.

Scripts must only consume patches below `runtime/`. Each runtime patch set
uses a `series` file as the single source for patch order and build identity.
Testing experiments instead use their zero-padded patch filenames as the
default order; an optional `series` file can override that order.
