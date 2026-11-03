#!/usr/bin/env bash

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
work=$(mktemp -d "${TMPDIR:-/tmp}/kait2en-input-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
cat >"$work/bin/insmod" <<'STUB'
#!/usr/bin/env bash
module=${1##*/}
printf '%s\n' "${module%.ko}" >>"$KAIT2EN_TEST_LOG"
[[ "$module" != "${KAIT2EN_TEST_FAIL_MODULE:-}.ko" ]]
STUB
chmod 0755 "$work/bin/insmod"

# Ignore modules already loaded on the machine running this test.
sed 's|/proc/modules|/dev/null|g' \
	"$repo_root/auto-installer/initramfs/20-kait2en-input.sh.in" >"$work/input-hook.sh"

# The hook must register all input drivers before the controller can enumerate
# devices. A failed trackpad load must stop the hook before controller startup.
for fail_module in '' t2_precision_trackpad; do
	: >"$work/load.log"
	if PATH="$work/bin:$PATH" KAIT2EN_TEST_LOG="$work/load.log" \
		KAIT2EN_TEST_FAIL_MODULE="$fail_module" \
		sh -c 'info() { :; }; warn() { :; }; . "$1"' sh \
		"$work/input-hook.sh"; then
		[[ -z "$fail_module" ]]
	else
		[[ -n "$fail_module" ]]
	fi
	python3 - "$work/load.log" "$fail_module" <<'PY'
import pathlib
import sys

loaded = pathlib.Path(sys.argv[1]).read_text().splitlines()
assert "t2hid" in loaded
assert "t2_trackpad_actuator" in loaded
assert "t2_precision_trackpad" in loaded
if sys.argv[2]:
    assert "t2bce_core" not in loaded
    assert "t2bce_vhci" not in loaded
else:
    for driver in ("t2hid", "t2_trackpad_actuator", "t2_precision_trackpad"):
        assert loaded.index(driver) < loaded.index("t2bce_core")
    assert loaded.index("t2bce_core") < loaded.index("t2bce_vhci")
PY
done

# Exercise Anaconda's first-boot task with a fake target root, including its
# failure path when dracut omits the trackpad. No Anaconda installation needed.
python3 - "$repo_root" "$work" <<'PY'
import ast
import os
import pathlib
import shutil
import sys
from types import SimpleNamespace

repo, work = map(pathlib.Path, sys.argv[1:])
service = repo / "auto-installer/anaconda-addon/com_kait2en_input/service"
constants = {}
# Read literal settings without importing Anaconda or dasbus.
for node in ast.parse((service / "constants.py.in").read_text()).body:
    if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name):
        if node.targets[0].id in ("MODULES", "KERNEL_RELEASE", "RPM_FILENAME"):
            constants[node.targets[0].id] = ast.literal_eval(node.value)
modules = constants["MODULES"]
assert {"t2_precision_trackpad", "t2_trackpad_actuator"} <= set(modules)
source = work / "installer"
source.mkdir()
(source / constants["RPM_FILENAME"]).touch()
root = work / "target"
module_root = root / "usr/lib/modules" / constants["KERNEL_RELEASE"] / "updates/kait2en"
module_root.mkdir(parents=True)
for module in modules:
    (module_root / (module + ".ko")).touch()

calls = []
listing = "\n".join(module + ".ko" for module in modules)
def redirect(command, args, **kwargs):
    calls.append((command, args, kwargs))

env = dict(constants, os=os, shutil=shutil, Task=object,
           INSTALLER_ROOT=str(source),
           conf=SimpleNamespace(target=SimpleNamespace(system_root=str(root))),
           util=SimpleNamespace(execWithRedirect=redirect,
                                execWithCapture=lambda *a, **kw: listing))
tree = ast.parse((service / "installation.py").read_text())
cls = next(node for node in tree.body if isinstance(node, ast.ClassDef)
           and node.name == "InstallTransitionDriversTask")
exec(compile(ast.Module(body=[cls], type_ignores=[]), str(service / "installation.py"), "exec"), env)
task = env["InstallTransitionDriversTask"]()
task.run()
dracut = next(args for command, args, _ in calls if command == "dracut")
assert "--force-drivers" in dracut
forced = dracut[dracut.index("--force-drivers") + 1].split()
for driver in ("t2hid", "t2_trackpad_actuator", "t2_precision_trackpad"):
    assert forced.index(driver) < forced.index("t2bce_core")
assert not (root / "tmp" / constants["RPM_FILENAME"]).exists()
listing = listing.replace("t2_precision_trackpad.ko", "")
try:
    task.run()
except RuntimeError as error:
    assert "missing from the target initramfs: t2_precision_trackpad" in str(error)
else:
    raise AssertionError("Anaconda accepted an initramfs without the trackpad driver")
PY
printf 'Installer input loading tests passed.\n'
