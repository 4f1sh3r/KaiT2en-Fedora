import os
import shutil

from pyanaconda.core import util
from pyanaconda.core.configuration.anaconda import conf
from pyanaconda.modules.common.constants.objects import BOOTLOADER
from pyanaconda.modules.common.constants.services import STORAGE
from pyanaconda.modules.common.task import Task

from com_kait2en_input.service.constants import (
    KERNEL_ARGUMENTS,
    KERNEL_RELEASE,
    MODULES,
    RPM_FILENAME,
)


INSTALLER_ROOT = "/usr/share/kait2en-installer"
PREPARE_LAUNCHER = os.path.join(INSTALLER_ROOT, "kait2en-prepare")
INSTALL_LAUNCHER = os.path.join(INSTALLER_ROOT, "kait2en-install")
TERMINAL_LAUNCHER = os.path.join(INSTALLER_ROOT, "kait2en-launch-terminal")
INSTALL_DESKTOP = os.path.join(INSTALLER_ROOT, "kait2en-install.desktop")
TRANSITION_SOURCE = os.path.join(INSTALLER_ROOT, "transition-source")


class InstallWifiFirmwareTask(Task):
    """Install user-provided Apple Wi-Fi firmware into the target system."""

    @property
    def name(self):
        return "Install T2 Mac Wi-Fi firmware"

    def run(self):
        source_dir = "/run/kait2en/apple-firmware"
        helper = os.path.join(INSTALLER_ROOT, "install-wifi-firmware.sh")
        if not os.path.isdir(source_dir):
            raise RuntimeError(
                "The required user-provided Wi-Fi firmware is missing from the installer"
            )
        if not os.path.isfile(helper):
            raise RuntimeError("The KaiT2en Wi-Fi firmware installer is missing")

        sysroot = conf.target.system_root
        util.execWithRedirect(
            "bash",
            [helper, "--source", source_dir, "--root", sysroot],
        )

        firmware_root = os.path.join(sysroot, "usr", "lib", "firmware", "brcm")
        installed = []
        if os.path.isdir(firmware_root):
            installed = [
                entry
                for entry in os.listdir(firmware_root)
                if entry.startswith("brcmfmac") and ".apple," in entry
            ]
        if len(installed) < 4:
            raise RuntimeError(
                "KaiT2en installed fewer Wi-Fi firmware files than expected"
            )


class InstallGuidedInstallerTask(Task):
    """Install the guided installer without an embedded Git checkout."""

    @property
    def name(self):
        return "Install the guided KaiT2en installer"

    def run(self):
        for path in (
            PREPARE_LAUNCHER,
            INSTALL_LAUNCHER,
            TERMINAL_LAUNCHER,
            INSTALL_DESKTOP,
        ):
            if not os.path.isfile(path):
                raise RuntimeError("The KaiT2en installer file is missing: " + path)
        if not os.path.isdir(TRANSITION_SOURCE):
            raise RuntimeError("The KaiT2en transition source is missing")

        sysroot = conf.target.system_root
        bin_dir = os.path.join(sysroot, "usr", "local", "bin")
        target_data = os.path.join(sysroot, "usr", "share", "kait2en-installer")
        target_transition = os.path.join(target_data, "transition-source")
        state_dir = os.path.join(sysroot, "var", "lib", "kait2en-installer")
        state_file = os.path.join(state_dir, "state")
        autostart_dir = os.path.join(sysroot, "etc", "xdg", "autostart")
        autostart_file = os.path.join(autostart_dir, "kait2en-install.desktop")

        os.makedirs(bin_dir, exist_ok=True)
        for source, name in (
            (PREPARE_LAUNCHER, "kait2en-prepare"),
            (INSTALL_LAUNCHER, "kait2en-install"),
            (TERMINAL_LAUNCHER, "kait2en-launch-terminal"),
        ):
            destination = os.path.join(bin_dir, name)
            shutil.copy2(source, destination)
            os.chmod(destination, 0o755)

        os.makedirs(target_data, exist_ok=True)
        if os.path.exists(target_transition):
            shutil.rmtree(target_transition)
        shutil.copytree(TRANSITION_SOURCE, target_transition)

        os.makedirs(state_dir, mode=0o755, exist_ok=True)
        with open(state_file, "w", encoding="utf-8") as state:
            state.write("phase=pending\n")
        os.chmod(state_file, 0o644)

        os.makedirs(autostart_dir, mode=0o755, exist_ok=True)
        shutil.copy2(INSTALL_DESKTOP, autostart_file)
        os.chmod(autostart_file, 0o644)


class InstallTransitionDriversTask(Task):
    """Install and verify the temporary input modules in the target system."""

    @property
    def name(self):
        return "Install KaiT2en input drivers for the first boot"

    def run(self):
        sysroot = conf.target.system_root
        source_rpm = os.path.join(INSTALLER_ROOT, RPM_FILENAME)
        target_rpm = os.path.join(sysroot, "tmp", RPM_FILENAME)

        if not os.path.isfile(source_rpm):
            raise RuntimeError("KaiT2en transition RPM is missing from updates.img")

        os.makedirs(os.path.dirname(target_rpm), exist_ok=True)
        shutil.copy2(source_rpm, target_rpm)

        try:
            util.execWithRedirect(
                "rpm",
                ["--install", "--replacepkgs", os.path.join("/tmp", RPM_FILENAME)],
                root=sysroot,
            )
        finally:
            if os.path.exists(target_rpm):
                os.unlink(target_rpm)

        module_root = os.path.join(
            sysroot, "usr", "lib", "modules", KERNEL_RELEASE, "updates", "kait2en"
        )
        for module in MODULES:
            path = os.path.join(module_root, module + ".ko")
            if not os.path.isfile(path):
                raise RuntimeError("Installed KaiT2en module is missing: " + path)

        util.execWithRedirect("depmod", ["-a", KERNEL_RELEASE], root=sysroot)
        initramfs = "/boot/initramfs-{}.img".format(KERNEL_RELEASE)
        util.execWithRedirect(
            "dracut",
            [
                "--force",
                "--add-drivers",
                " ".join(MODULES),
                initramfs,
                KERNEL_RELEASE,
            ],
            root=sysroot,
        )

        listing = util.execWithCapture("lsinitrd", [initramfs], root=sysroot)
        for module in MODULES:
            if module + ".ko" not in listing:
                raise RuntimeError(
                    "KaiT2en module is missing from the target initramfs: " + module
                )


class ConfigureKernelArgumentsTask(Task):
    """Add the T2 input conflict arguments through Anaconda's bootloader API."""

    @property
    def name(self):
        return "Configure KaiT2en input driver kernel arguments"

    def run(self):
        bootloader = STORAGE.get_proxy(BOOTLOADER)
        arguments = list(bootloader.ExtraArguments)

        for new_argument in KERNEL_ARGUMENTS:
            key = new_argument.split("=", 1)[0] + "="
            arguments = [argument for argument in arguments if not argument.startswith(key)]
            arguments.append(new_argument)

        bootloader.ExtraArguments = arguments
