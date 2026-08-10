#!/usr/bin/env bash
set -euo pipefail
APP_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
source "$APP_DIR/../../scripts/fedora/lib.sh"
require_root
require_fedora
require_command dnf install systemctl

dnf install -y python3-gobject gtk4 libadwaita msr-tools stress-ng polkit
install -d -m 0755 /usr/local/bin /usr/local/libexec /usr/local/share/applications \
    /usr/local/share/icons/hicolor/scalable/apps /usr/local/lib/systemd/system \
    /usr/local/lib/systemd/system-sleep /usr/share/polkit-1/actions
install -m 0755 "$APP_DIR/t2-cpu-control.py" /usr/local/bin/t2-cpu-control
install -m 0755 "$APP_DIR/t2-cpu-control-helper" /usr/local/libexec/t2-cpu-control-helper
install -m 0755 "$APP_DIR/t2-cpu-control-status" /usr/local/libexec/t2-cpu-control-status
install -m 0755 "$APP_DIR/t2-cpu-control-resume" /usr/local/lib/systemd/system-sleep/t2-cpu-control
install -m 0644 "$APP_DIR/t2-cpu-control.service" /usr/local/lib/systemd/system/
install -m 0644 "$APP_DIR/org.t2cpucontrol.policy" /usr/share/polkit-1/actions/
install -m 0644 "$APP_DIR/org.t2cpucontrol.gtk.desktop" /usr/local/share/applications/
install -m 0644 "$APP_DIR/org.t2cpucontrol.gtk.svg" /usr/local/share/icons/hicolor/scalable/apps/
systemctl daemon-reload
systemctl enable --now t2-cpu-control.service
gtk-update-icon-cache --force --ignore-theme-index /usr/local/share/icons/hicolor
info "t2-cpu-control installed"
