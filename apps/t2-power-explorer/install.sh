#!/usr/bin/env bash
set -euo pipefail

APP_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
source "$APP_DIR/../../scripts/fedora/lib.sh"
require_root
require_fedora
require_command dnf make

dnf install -y cargo gcc gtk4-devel libadwaita-devel
make -C "$APP_DIR" install
info "t2-power-explorer installed"
