#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
launcher="$repo_root/packaging/installer/runtime/kait2en-install"
work=$(mktemp -d "${TMPDIR:-/tmp}/kait2en-install-test.XXXXXX")
trap 'rm -rf "$work"' EXIT

target=7.1.3-200.fc44.x86_64
fake_home="$work/home"
fake_repo="$work/repository"
fake_state="$work/state"
fake_bin="$work/bin"
log="$work/commands.log"
mkdir -p "$fake_home" "$fake_repo/.git" "$fake_state" "$fake_bin"
printf 'phase=reboot_pending\ntarget_kernel=%s\n' "$target" >"$fake_state/state"

cat >"$fake_bin/flock" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$fake_bin/uname" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$KAIT2EN_TEST_TARGET"
EOF
cat >"$fake_bin/git" <<'EOF'
#!/usr/bin/env bash
case "$*" in
	"-C $KAIT2EN_TEST_REPOSITORY status --porcelain") exit 0 ;;
	"-C $KAIT2EN_TEST_REPOSITORY remote get-url origin")
		printf '%s\n' 'https://github.com/kaiT2en/KaiT2en-Fedora.git'
		;;
	"ls-remote --exit-code "*) exit 0 ;;
	*) printf 'git %s\n' "$*" >>"$KAIT2EN_TEST_LOG" ;;
esac
EOF
cat >"$fake_bin/sudo" <<'EOF'
#!/usr/bin/env bash
printf 'sudo cwd=%s command=%s\n' "$PWD" "$*" >>"$KAIT2EN_TEST_LOG"
EOF
chmod 0755 "$fake_bin"/*

printf 'n\n' |
	env \
		HOME="$fake_home" \
		XDG_RUNTIME_DIR="$work" \
		PATH="$fake_bin:/usr/bin:/bin" \
		KAIT2EN_STATE_DIR="$fake_state" \
		KAIT2EN_REPOSITORY="$fake_repo" \
		KAIT2EN_TEST_TARGET="$target" \
		KAIT2EN_TEST_REPOSITORY="$fake_repo" \
		KAIT2EN_TEST_LOG="$log" \
		bash "$launcher" >/dev/null

grep -Fq "git -C $fake_repo pull --ff-only origin main" "$log"
grep -Fq "sudo cwd=$fake_repo command=bash ./scripts/fedora/install.sh" "$log"
grep -Fq 'command=/usr/local/bin/kait2en-prepare --complete' "$log"

: >"$log"
printf 'phase=complete\ntarget_kernel=%s\n' "$target" >"$fake_state/state"
printf 'n\n' |
	env \
		HOME="$fake_home" \
		XDG_RUNTIME_DIR="$work" \
		PATH="$fake_bin:/usr/bin:/bin" \
		KAIT2EN_STATE_DIR="$fake_state" \
		KAIT2EN_REPOSITORY="$fake_repo" \
		KAIT2EN_TEST_TARGET="$target" \
		KAIT2EN_TEST_REPOSITORY="$fake_repo" \
		KAIT2EN_TEST_LOG="$log" \
		bash "$launcher" >/dev/null
grep -Fq "git -C $fake_repo pull --ff-only origin main" "$log"
grep -Fq "sudo cwd=$fake_repo command=bash ./scripts/fedora/install.sh" "$log"
! grep -Fq 'kait2en-prepare --complete' "$log"

mkdir -p "$fake_repo/packaging/installer/runtime"
cat >"$fake_repo/packaging/installer/runtime/kait2en-install" <<'EOF'
#!/usr/bin/env bash
printf 'delegated to repository installer\n' >>"$KAIT2EN_TEST_LOG"
EOF
: >"$log"
env \
	HOME="$fake_home" \
	XDG_RUNTIME_DIR="$work" \
	PATH="$fake_bin:/usr/bin:/bin" \
	KAIT2EN_STATE_DIR="$fake_state" \
	KAIT2EN_REPOSITORY="$fake_repo" \
	KAIT2EN_TEST_TARGET="$target" \
	KAIT2EN_TEST_REPOSITORY="$fake_repo" \
	KAIT2EN_TEST_LOG="$log" \
	bash "$launcher" >/dev/null
grep -Fxq 'delegated to repository installer' "$log"
