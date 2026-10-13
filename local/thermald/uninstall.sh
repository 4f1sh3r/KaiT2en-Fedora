#!/bin/bash
set -euo pipefail
rm -f /etc/thermald/thermal-conf.xml.in
rm -f /etc/thermald/thermal-conf.xml
rm -f /usr/local/bin/t2-thermald-genconf
rm -f /etc/systemd/system/thermald.service.d/kait2en-manual.conf
systemctl daemon-reload
systemctl restart thermald
