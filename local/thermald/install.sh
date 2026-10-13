#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
install -m 0644 thermal-conf.xml.in /etc/thermald/thermal-conf.xml.in
install -m 0644 thermal-conf.xml /etc/thermald/thermal-conf.xml
install -m 0755 t2-thermald-genconf /usr/local/bin/t2-thermald-genconf
install -d /etc/systemd/system/thermald.service.d
install -m 0644 kait2en-manual.conf /etc/systemd/system/thermald.service.d/kait2en-manual.conf
systemctl daemon-reload
systemctl restart thermald
