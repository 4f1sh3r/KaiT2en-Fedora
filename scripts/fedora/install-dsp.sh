#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command dnf make python3

# Install all profiles, exactly as in the standalone package. udev and
# WirePlumber choose the model at runtime, not on the installation/build host.
run_step "install DSP dependencies" dnf install -y \
    pipewire pipewire-pulseaudio 'wireplumber >= 0.5.8' \
    pipewire-module-filter-chain-lv2 lv2-bankstown lsp-plugins-lv2

if (( STEP_STATUS == 0 )); then
    run_step "install DSP profiles and integration" make -C "$REPO_ROOT/dsp" install PREFIX=/usr
    if (( STEP_STATUS == 0 )); then
        run_step "migrate legacy DSP configuration" bash /usr/libexec/t2-dsp/package-actions configure
    else
        warn "DSP files were not fully installed. Legacy configuration was left in place"
    fi
else
    warn "DSP dependencies could not be installed. Existing DSP files and configuration were left in place"
fi

info "DSP installation steps finished. Reboot to activate. User audio services were not restarted"
