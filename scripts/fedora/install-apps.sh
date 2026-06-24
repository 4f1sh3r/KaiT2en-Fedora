#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

require_root
require_repo_root
require_fedora
require_command make cargo tar getent cut id

install_rust_app() {
	local path=$1 name=$2
	info "building and installing $name"
	make -C "$path" clean
	make -C "$path" build
	make -C "$path" install
}

has_t2_touchbar_model() {
	local model
	[[ -r /sys/class/dmi/id/product_name ]] || {
		info "DMI product name not found, skipping react-drm"
		return 1
	}

	read -r model </sys/class/dmi/id/product_name
	case "$model" in
		MacBookPro15,1|MacBookPro15,2|MacBookPro15,3|MacBookPro15,4|\
		MacBookPro16,1|MacBookPro16,2|MacBookPro16,3|MacBookPro16,4)
			return 0
			;;
		*)
			info "Model $model has no T2 Touch Bar entry, skipping react-drm"
			return 1
			;;
	esac
}

install_react_drm() {
	local target_user target_home target_group src dst
	if ! has_t2_touchbar_model; then
		return
	fi

	target_user="${SUDO_USER:-}"
	[[ -n "$target_user" && "$target_user" != root ]] ||
		fail "react-drm must be installed for the user who invoked sudo"

	target_home="$(getent passwd "$target_user" | cut -d: -f6)"
	target_group="$(id -gn "$target_user")"
	[[ -n "$target_home" && -d "$target_home" ]] ||
		fail "unable to determine home directory for $target_user"

	src="$REPO_ROOT/apps/react-drm"
	dst="$target_home/react-drm"
	[[ -x "$src/install.sh" ]] || fail "react-drm installer not found"
	if [[ -e "$dst" && ! -f "$dst/package.json" ]]; then
		fail "deployment directory exists but does not look like react-drm: $dst"
	fi

	info "copying react-drm source to $dst"
	rm -rf "$dst"
	install -d -o "$target_user" -g "$target_group" -m 0755 "$dst"
	tar -C "$src" \
		--exclude='.git' \
		--exclude='node_modules' \
		--exclude='dist' \
		--exclude='linux-touchbar-control-center/dist' \
		-cf - . | tar -C "$dst" -xf -
	chown -R "$target_user:$target_group" "$dst"

	info "installing react-drm"
	sudo -u "$target_user" "$dst/install.sh"
}

install_rust_app "$REPO_ROOT/apps/t2-fan-control" "t2-fan-control"
install_rust_app "$REPO_ROOT/apps/t2-smc-control" "t2-smc-control"
install_react_drm

info "apps installed"
