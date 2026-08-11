#!/usr/bin/env bash
set -euo pipefail
APP_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
source "$APP_DIR/../../scripts/fedora/lib.sh"
require_root
require_fedora
require_command dnf install systemctl

dnf install -y \
    "kernel-devel-$(uname -r)" \
    cpio curl elfutils-libelf-devel gcc git-core gtk4 libadwaita make msr-tools \
    patch polkit python3-gobject rpm-build xz
install -d -m 0755 /usr/local/bin /usr/local/libexec /usr/local/share/applications \
    /usr/local/share/icons/hicolor/scalable/apps /usr/local/lib/systemd/system \
    /usr/local/lib/systemd/system-sleep /usr/share/polkit-1/actions
install -m 0755 "$APP_DIR/t2-cpu-control.py" /usr/local/bin/t2-cpu-control
install -m 0755 "$APP_DIR/t2-cpu-control-helper" /usr/local/libexec/t2-cpu-control-helper
install -m 0755 "$APP_DIR/t2-cpu-control-status" /usr/local/libexec/t2-cpu-control-status
install -m 0755 "$APP_DIR/t2-cpu-kernel-benchmark" /usr/local/libexec/t2-cpu-kernel-benchmark
install -m 0755 "$APP_DIR/t2-cpu-control-resume" /usr/local/lib/systemd/system-sleep/t2-cpu-control
install -m 0644 "$APP_DIR/t2-cpu-control.service" /usr/local/lib/systemd/system/
install -m 0644 "$APP_DIR/org.t2cpucontrol.policy" /usr/share/polkit-1/actions/
install -m 0644 "$APP_DIR/org.t2cpucontrol.gtk.desktop" /usr/local/share/applications/
install -m 0644 "$APP_DIR/org.t2cpucontrol.gtk.svg" /usr/local/share/icons/hicolor/scalable/apps/
if systemctl is-enabled --quiet t2-cpu-control-status.service; then
    systemctl disable --now t2-cpu-control-status.service
elif systemctl is-active --quiet t2-cpu-control-status.service; then
    systemctl stop t2-cpu-control-status.service
fi
rm -f /usr/local/lib/systemd/system/t2-cpu-control-status.service
systemctl daemon-reload
systemctl enable t2-cpu-control.service
systemctl restart t2-cpu-control.service
gtk-update-icon-cache --force --ignore-theme-index /usr/local/share/icons/hicolor
info "t2-cpu-control installed"
