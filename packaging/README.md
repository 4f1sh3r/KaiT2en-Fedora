# Binary package infrastructure

This directory contains shared infrastructure for the independently installed
RPM and Debian packages. Component recipes remain with their owners:

- `dsp/packaging/`
- `t2-services/{shared,t2-touchid,t2-ave,t2-journal}/packaging/`

`lifecycle/` supplies owned migration and removal logic and its tests. Each
consumer packages its own runtime copy where appropriate.
