# Patch layout

- `runtime/` contains patches applied by installation and build scripts.
- `upstream/` contains complete mail artifacts being prepared or already sent.
- `archived/` contains merged, withdrawn or superseded patches kept for
  reference.

Scripts must only consume patches below `runtime/`. Each runtime patch set
uses a `series` file as the single source for patch order and build identity.
