#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora

OBSOLETE_UNITS=(
	kait2en-t2-smc-charge-limit.service
)

remove_obsolete_apps() {
	local unit reload=0

	info "removing obsolete t2-gpu-switch installation"
	rm -f \
		/usr/local/bin/t2-gpu-switch \
		/usr/local/libexec/t2-gpu-switch-helper \
		/usr/local/share/applications/org.t2gpuswitch.gtk.desktop

	for unit in "${OBSOLETE_UNITS[@]}"; do
		if systemctl list-unit-files "$unit" &>/dev/null; then
			info "removing obsolete $unit"
			systemctl disable --now "$unit" || warn "could not disable $unit"
			reload=1
		fi
		[[ -e "/usr/local/lib/systemd/system/$unit" ]] || continue
		rm -f "/usr/local/lib/systemd/system/$unit"
		reload=1
	done
	[[ "$reload" -eq 0 ]] || systemctl daemon-reload
}

# t2-zone-power replaces t2-cpu-control. Its helper restores the firmware
# power limits, no_turbo, TCC target, maximum frequency and thermald state it
# captured at boot. Every step is skipped when there is nothing left to do,
# so a removed or half removed installation is handled as well.
remove_t2_cpu_control() {
	local helper=/usr/local/libexec/t2-cpu-control-helper path found=0
	local paths=(
		/usr/local/bin/t2-cpu-control
		"$helper"
		/usr/local/libexec/t2-cpu-control-status
		/usr/local/libexec/t2-cpu-kernel-benchmark
		/usr/local/lib/systemd/system/t2-cpu-control.service
		/usr/local/lib/systemd/system-sleep/t2-cpu-control
		/usr/local/share/applications/org.t2cpucontrol.gtk.desktop
		/usr/local/share/icons/hicolor/scalable/apps/org.t2cpucontrol.gtk.svg
		/usr/share/polkit-1/actions/org.t2cpucontrol.policy
		/etc/t2-cpu-control.conf
		/var/lib/t2-cpu-control
		/run/t2-cpu-control
	)

	for path in "${paths[@]}"; do
		[[ -e "$path" ]] && found=1
	done
	[[ "$found" -eq 1 ]] || return 0
	info "removing t2-cpu-control, replaced by t2-zone-power"

	if [[ -x "$helper" && -r /run/t2-cpu-control/firmware-defaults ]]; then
		"$helper" restore || warn "could not restore the firmware CPU limits, they return on the next boot"
	fi
	if systemctl list-unit-files t2-cpu-control.service &>/dev/null; then
		systemctl disable --now t2-cpu-control.service || warn "could not disable t2-cpu-control.service"
	fi
	rm -rf "${paths[@]}"
	systemctl daemon-reload
	gtk-update-icon-cache --force --ignore-theme-index /usr/local/share/icons/hicolor ||
		warn "could not refresh the icon cache"
}

clean_rust_build() {
	local path=$1 target_user=$2
	clean_cargo_build "$path" "$target_user"
}

install_rust_app() {
	local path=$1 name=$2 target_user
	info "building and installing $name"

	target_user="${SUDO_USER:-}"
	[[ -n "$target_user" && "$target_user" != root ]] ||
		fail "$name must be built for the user who invoked sudo"

	if ! sudo -H -u "$target_user" make -C "$path" build; then
		clean_rust_build "$path" "$target_user"
		warn "$name build failed; skipping this app and continuing"
		return 0
	fi
	if ! make -C "$path" install; then
		clean_rust_build "$path" "$target_user"
		warn "$name installation failed; continuing with the remaining apps"
		return 0
	fi
	if [[ "$name" == t2-journal ]]; then
		python3 "$REPO_ROOT/packaging/lifecycle/kait2en-lifecycle.py" t2-journal record-source || warn "could not record t2-journal installation ownership"
	fi
	clean_rust_build "$path" "$target_user"
}

install_gpu_control() {
	local model

	[[ -r /sys/class/dmi/id/product_name ]] || {
		info "DMI product name not found, skipping GPU control"
		return
	}
	read -r model </sys/class/dmi/id/product_name

	case "$model" in
		MacBookPro15,1|MacBookPro16,1|MacBookPro16,4)
			info "installing hybrid graphics support for $model"
			if ! make -C "$REPO_ROOT/apps/t2-dgpu-control" uninstall; then
				warn "unable to remove the inactive t2-dgpu-control app; continuing"
			fi
			if ! "$REPO_ROOT/apps/t2-hybrid-gpu-control/install.sh"; then
				warn "t2-hybrid-gpu-control installation failed; continuing"
			fi
			;;
		MacBookPro15,3)
			if ! make -C "$REPO_ROOT/apps/t2-hybrid-gpu-control" uninstall; then
				warn "unable to remove the inactive t2-hybrid-gpu-control app; continuing"
			fi
			if ! "$REPO_ROOT/apps/t2-dgpu-control/install.sh"; then
				warn "t2-dgpu-control installation failed; continuing"
			fi
			;;
		*)
			info "Model $model has no supported switchable AMD dGPU"
			if ! make -C "$REPO_ROOT/apps/t2-hybrid-gpu-control" uninstall; then
				warn "unable to remove t2-hybrid-gpu-control; continuing"
			fi
			if ! make -C "$REPO_ROOT/apps/t2-dgpu-control" uninstall; then
				warn "unable to remove t2-dgpu-control; continuing"
			fi
			;;
	esac
}

install_app_assets() {
	install -d -o root -g root -m 0755 /usr/local/share/kait2en
	install -o root -g root -m 0644 \
		"$REPO_ROOT/assets/kait2en-app-logo.png" \
		/usr/local/share/kait2en/kait2en-wordmark.png
	install_kait2en_fonts
}
run_step "install app assets" install_app_assets
run_step "remove obsolete apps" remove_obsolete_apps
run_step "t2-fan-control" install_rust_app "$REPO_ROOT/apps/t2-fan-control" "t2-fan-control"
run_step "t2-smc-control" install_rust_app "$REPO_ROOT/apps/t2-smc-control" "t2-smc-control"
run_step "t2-power-explorer" install_rust_app "$REPO_ROOT/apps/t2-power-explorer" "t2-power-explorer"
run_step "t2-force-click" install_rust_app "$REPO_ROOT/apps/t2-force-click" "t2-force-click"
if ! has_touch_bar; then
	info "no Touch Bar on this model; skipping kait2en-touchbar"
elif [[ "${KAIT2EN_INSTALL_TOUCHBAR:-1}" == 1 ]]; then
	run_step "kait2en-touchbar" "$REPO_ROOT/apps/t2-touchbar/install.sh"
elif [[ -x /usr/local/bin/kait2en-touchbar ]]; then
	info "kait2en-touchbar not selected; the existing installation is left unchanged"
	info "remove it with: sudo $REPO_ROOT/apps/t2-touchbar/uninstall.sh"
else
	info "kait2en-touchbar not selected; the native Touch Bar row stays active"
fi
run_step "t2-journal" install_rust_app "$REPO_ROOT/t2-services/t2-journal" "t2-journal"
run_step "GPU control" install_gpu_control
run_step "remove t2-cpu-control" remove_t2_cpu_control
run_step "t2-zone-power" install_rust_app "$REPO_ROOT/apps/t2-zone-power" "t2-zone-power"
if ! "$REPO_ROOT/apps/t2-kernel-builder/install.sh"; then
	warn "t2-kernel-builder installation failed; continuing because it is optional"
	warn "retry it later with: sudo $REPO_ROOT/apps/t2-kernel-builder/install.sh"
fi
if ! "$REPO_ROOT/apps/t2-power-tune/install.sh"; then
	warn "t2-power-tune installation failed; continuing"
fi

info "apps installed"
