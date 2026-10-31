#!/usr/bin/env python3
"""Build-time helpers for the Alpine appliance root (ADR 0117).

Runs in the builder container (packaging/alpine/build-root.sh) and in the host unit tests
(scripts/tests/test_alpine.py). Never shipped in the image: the appliance has no Python.

Subcommands:
  world FILE                         print the package allowlist, one per line (validated)
  cmdline COUNTRY                    print the slot cmdline template (slot A placeholder disk id)
  prune-modules ROOT KVER            keep MODULE_KEEP plus dependencies, delete the rest
  initramfs-modules ROOT KVER        print the dependency-ordered modules the initramfs loads
  prune-firmware ROOT                keep the Pi Zero W radio chain and the regulatory database only
  check ROOT --uid N --gid N [--profile P]  runtime guard (paths, runlevels, users, fstab, inittab)
  check-elf FILE...                  require static 32-bit ARM executables (no PT_INTERP)
  summary ROOT                       "files=N bytes=N links=N" of regular files and symlinks
  check-dtb DTB                      the Zero W dtb has disable-bt applied and keeps __symbols__
"""
from __future__ import annotations

import argparse
import os
import re
import stat
import struct
import sys
from pathlib import Path, PurePosixPath

# --------------------------------------------------------------------------------------------- world

WORLD_NAME = re.compile(r"^[a-z0-9][a-z0-9+._-]*$")
# Packages the image cannot work without; the world file must name them (directly).
REQUIRED_WORLD = (
    "alpine-base", "linux-firmware-brcm", "linux-firmware-cypress", "wireless-regdb", "kmod",
    "eudev", "networkmanager", "networkmanager-wifi", "wpa_supplicant", "dbus", "polkit", "dnsmasq",
    "openssh-server", "avahi", "tailscale", "sudo", "zstd", "curl", "ca-certificates", "e2fsprogs",
    "dosfstools", "chrony",
)


def parse_world(text: str) -> list[str]:
    """Package names from a world file: `#` comments, blank lines ignored; names validated, no duplicates."""
    names: list[str] = []
    for number, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        if len(line.split()) != 1 or not WORLD_NAME.match(line):
            raise ValueError(f"world line {number}: invalid package name {line!r}")
        if line in names:
            raise ValueError(f"world line {number}: duplicate package {line!r}")
        names.append(line)
    missing = [name for name in REQUIRED_WORLD if name not in names]
    if missing:
        raise ValueError(f"world lacks required packages: {' '.join(missing)}")
    return names


# ------------------------------------------------------------------------------------------- cmdline

def slot_cmdline(country: str, root_fs: str = "squashfs", smoke: bool = False) -> str:
    """The slot cmdline template. `nibe-updater os-install` replaces root= and nibe.slot= for the target slot
    (crates/updater/src/os.rs slot_cmdline) and refuses a rootfstype= that does not match rootfs.img.
    No serial console: the PL011 (ttyAMA0) belongs to the RS485 HAT. No `quiet`: HDMI (tty1) shows the whole boot.
    `smoke`: nibe.smoke=1, a boot that records traces, never commits and reboots by itself (HP-38 step 1)."""
    if not re.fullmatch(r"[A-Z]{2}", country):
        raise ValueError("country must be two uppercase letters")
    if root_fs not in ("squashfs", "ext4"):
        raise ValueError("root_fs must be squashfs or ext4")
    tokens = [
        "console=tty1",
        "root=PARTUUID=00000000-02",
        "rootwait",
        f"cfg80211.ieee80211_regdom={country}",
        "nibe.overlay=1",
        f"rootfstype={root_fs}",
        "panic=10",
    ]
    if smoke:
        tokens.append("nibe.smoke=1")
    tokens.append("nibe.slot=a")
    return " ".join(tokens) + "\n"


def check_cmdline(text: str) -> list[str]:
    errors = []
    if text.count("\n") != 1 or not text.endswith("\n"):
        errors.append("cmdline.txt must be exactly one line")
    tokens = text.split()
    keys = [t.split("=", 1)[0] for t in tokens]
    for key in ("root", "rootfstype", "panic", "nibe.slot", "cfg80211.ieee80211_regdom"):
        if keys.count(key) != 1:
            errors.append(f"cmdline needs exactly one {key}=")
    if not re.search(r"(^| )root=PARTUUID=[0-9a-f]{8}-02( |$)", text):
        errors.append("cmdline root= must be PARTUUID=<disk>-02 (slot A template)")
    if "rootfstype=squashfs" not in tokens:
        errors.append("cmdline must say rootfstype=squashfs")
    if "nibe.slot=a" not in tokens:
        errors.append("cmdline template must be for slot a")
    for token in tokens:
        if token.startswith("console=") and re.search(r"tty(AMA|S)\d|serial\d", token):
            errors.append(f"serial console on the RS485 UART: {token}")
        if token.startswith("init="):
            errors.append("cmdline must not override init=")
        if token == "quiet":
            errors.append("no quiet: the console must show the whole boot")
    return errors


# ------------------------------------------------------------------------------------------- modules

# Kept under /lib/modules/<kver>/ (prefix match), plus everything they depend on (modules.dep). The kernel is the
# Raspberry Pi OS 6.12.109+rpt-rpi-v6 (packaging/alpine/kernel.lock); built in there (its config and modules.builtin):
# MMC/SD (bcm2835), ext4, fat/vfat with nls_cp437/nls_ascii, af_packet, PL011, bcm2835 watchdog and hwrng. Entries
# that are built in on a kernel simply match nothing.
MODULE_KEEP = (
    "kernel/drivers/net/wireless/broadcom/brcm80211/",  # brcmfmac + brcmutil: Zero W BCM43430A1 radio
    "kernel/net/wireless/",                             # cfg80211
    "kernel/net/rfkill/",
    "kernel/net/packet/",                               # af_packet: NM internal DHCP client, dnsmasq DHCP server
    "kernel/net/ipv6/",                                 # IPv6 is a module here (avahi, NM, tailscale)
    "kernel/net/ipv4/netfilter/",
    "kernel/net/ipv6/netfilter/",
    "kernel/net/netfilter/",                            # tailscaled (nftables mode) firewall rules
    "kernel/drivers/net/tun.ko",                        # tailscaled
    "kernel/fs/overlayfs/",                             # initramfs + runtime
    "kernel/fs/squashfs/",
    "kernel/fs/nls/",                                   # vfat iocharsets for /boot/firmware
    "kernel/kernel/configs.ko",                         # /proc/config.gz for diagnostics (IKCONFIG=m)
)
# The initramfs needs these (and vfat/nls for its trace on the boot partition); built-in ones are skipped.
INITRAMFS_MODULES = ("kernel/fs/squashfs/squashfs.ko", "kernel/fs/overlayfs/overlay.ko", "kernel/fs/fat/fat.ko",
                     "kernel/fs/fat/vfat.ko", "kernel/fs/nls/nls_cp437.ko", "kernel/fs/nls/nls_ascii.ko")
# Present as a module or built in, or the image cannot do its job.
REQUIRED_MODULES = (
    "kernel/drivers/net/wireless/broadcom/brcm80211/brcmfmac/brcmfmac.ko", "kernel/net/wireless/cfg80211.ko",
    "kernel/net/ipv6/ipv6.ko", "kernel/net/packet/af_packet.ko", "kernel/drivers/net/tun.ko",
    "kernel/fs/overlayfs/overlay.ko", "kernel/fs/squashfs/squashfs.ko", "kernel/fs/fat/vfat.ko",
    "kernel/fs/nls/nls_cp437.ko",
)


def parse_builtin(text: str) -> set[str]:
    return {MODULE_SUFFIX.sub("", line.strip()) for line in text.splitlines() if line.strip()}
MODULE_SUFFIX = re.compile(r"\.ko(\.(xz|gz|zst))?$")


def parse_modules_dep(text: str) -> dict[str, list[str]]:
    deps: dict[str, list[str]] = {}
    for line in text.splitlines():
        if ":" not in line:
            continue
        module, rest = line.split(":", 1)
        deps[module.strip()] = rest.split()
    return deps


def module_closure(deps: dict[str, list[str]], keep=MODULE_KEEP) -> set[str]:
    wanted = {m for m in deps if any(m == k or (k.endswith("/") and m.startswith(k)) or
                                      MODULE_SUFFIX.sub("", m) == MODULE_SUFFIX.sub("", k) for k in keep)}
    todo = list(wanted)
    while todo:
        for dep in deps.get(todo.pop(), []):
            if dep not in wanted:
                if dep not in deps:
                    raise ValueError(f"modules.dep names unknown module {dep}")
                wanted.add(dep)
                todo.append(dep)
    return wanted


def ordered_load(deps: dict[str, list[str]], roots, builtin: set[str] = frozenset()) -> list[str]:
    """Dependencies first, each module once (insmod order). Roots built into the kernel are skipped."""
    order: list[str] = []

    def visit(module: str, path: tuple[str, ...]):
        if module in order:
            return
        if module in path:
            raise ValueError(f"module dependency cycle at {module}")
        for dep in deps.get(module, []):
            visit(dep, path + (module,))
        order.append(module)

    for root in roots:
        if MODULE_SUFFIX.sub("", root) in builtin:
            continue
        matches = [m for m in deps if MODULE_SUFFIX.sub("", m) == MODULE_SUFFIX.sub("", root)]
        if len(matches) != 1:
            raise ValueError(f"initramfs module {root} not found in modules.dep")
        visit(matches[0], ())
    return order


def prune_modules(root: Path, kver: str) -> tuple[int, int]:
    """Deletes every module outside the closure; returns (kept, removed). The caller re-runs depmod."""
    base = root / "lib/modules" / kver
    deps = parse_modules_dep((base / "modules.dep").read_text())
    keep = module_closure(deps)
    removed = 0
    for module in deps:
        if module not in keep:
            (base / module).unlink(missing_ok=True)
            removed += 1
    for path in sorted((base / "kernel").rglob("*"), key=lambda p: len(p.parts), reverse=True):
        if path.is_dir() and not any(path.iterdir()):
            path.rmdir()
    return len(keep), removed


# ------------------------------------------------------------------------------------------ firmware

# Names as the kernel requests them. Alpine ships them zstd-compressed (`.zst`); the Raspberry Pi OS kernel has no
# CONFIG_FW_LOADER_COMPRESS, so the build decompresses the kept chain (decompress_firmware).
FIRMWARE_KEEP = (
    # Pi Zero W radio (BCM43430A1, brcmfmac): board-specific names first, then the generic fallbacks the driver tries.
    "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.bin",
    "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.clm_blob",
    "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.txt",
    "brcm/brcmfmac43430-sdio.bin",
    "brcm/brcmfmac43430-sdio.clm_blob",
    "brcm/brcmfmac43430-sdio.txt",
    "regulatory.db",
    "regulatory.db.p7s",
)


def resolve(root: Path, path: str | PurePosixPath, hops: int = 16) -> Path | None:
    """`path` inside the image root, following symlinks (absolute targets are relative to the image root).
    None if missing, dangling, looping or escaping the root."""
    root = Path(os.path.abspath(root))
    current = Path(os.path.normpath(root / str(path).lstrip("/")))
    for _ in range(hops):
        if current != root and root not in current.parents:
            return None
        if current.is_symlink():
            target = os.readlink(current)
            current = Path(os.path.normpath(root / target.lstrip("/") if target.startswith("/")
                                            else current.parent / target))
            continue
        return current if current.exists() else None
    return None


def firmware_chain(root: Path, fw: Path, name: str) -> list[str]:
    """`name` and every link target it passes through, relative to fw."""
    chain, current = [], fw / name
    for _ in range(16):
        if not (current.exists() or current.is_symlink()):
            break
        if fw in current.parents:
            chain.append(str(current.relative_to(fw)))
        if not current.is_symlink():
            break
        target = os.readlink(current)
        current = Path(os.path.normpath(root / target.lstrip("/") if target.startswith("/")
                                        else current.parent / target))
    return chain


def prune_firmware(root: Path) -> tuple[list[str], int]:
    fw = root / "lib/firmware"
    keep: set[str] = set()
    for name in FIRMWARE_KEEP:
        keep.update(firmware_chain(root, fw, name))
        keep.update(firmware_chain(root, fw, name + ".zst"))
    removed = 0
    for path in sorted(fw.rglob("*")):
        if (path.is_file() or path.is_symlink()) and str(path.relative_to(fw)) not in keep:
            path.unlink()
            removed += 1
    for path in sorted(fw.rglob("*"), key=lambda p: len(p.parts), reverse=True):
        if path.is_dir() and not path.is_symlink() and not any(path.iterdir()):
            path.rmdir()
    return sorted(keep), removed


def decompress_firmware(root: Path) -> int:
    """Replaces every `.zst` firmware file by its decompressed content and every `.zst` link by a link without the
    suffix (pointing at the target without it). Needs the `zstd` program. Returns the number of files decompressed."""
    import subprocess
    fw = root / "lib/firmware"
    count = 0
    for path in sorted(fw.rglob("*.zst")):
        if path.is_symlink():
            target = os.readlink(path)
            plain = path.with_name(path.name[:-4])
            plain.unlink(missing_ok=True)
            os.symlink(target[:-4] if target.endswith(".zst") else target, plain)
            path.unlink()
        elif path.is_file():
            subprocess.run(["zstd", "-q", "-d", "-f", "--rm", str(path), "-o", str(path.with_name(path.name[:-4]))],
                           check=True)
            count += 1
    return count


# ------------------------------------------------------------------------------------------- guards

# Services per runlevel. Ordering inside a runlevel comes from each script's depend() (see the init.d files).
RUNLEVELS = {
    # The hardware watchdog starts as early as /dev exists, so a hang anywhere after sysinit resets the Pi.
    "sysinit": ("devfs", "dmesg", "udev", "udev-trigger", "udev-settle", "watchdog"),
    "boot": ("modules", "sysctl", "hostname", "bootmisc", "syslog", "klogd", "swclock", "localmount", "loopback",
             "seedrng", "nibe-persist", "nibe-trace", "nibe-boot-guard"),
    # polkit is not listed: D-Bus activates polkitd on the first authorization request (as on Raspberry Pi OS). An
    # OpenRC polkit service raced that activation and lost the bus name ("respawned too many times", alpine.5).
    "default": ("dbus", "networkmanager", "avahi-daemon", "chronyd", "sshd", "tailscale",
                "nibe-updater", "nibe-system", "nibe-provision", "nibe-gateway", "nibe-tailscale-up",
                "nibe-os-confirm", "nibe-wifi-fallback", "nibe-smoke"),
    "shutdown": ("killprocs", "savecache", "mount-ro"),
}
# Must never be enabled: mdev competes with udev; hwclock has no RTC to read; networking is ifupdown.
FORBIDDEN_SERVICES = ("mdev", "hwdrivers", "hwclock", "networking", "wpa_supplicant", "dnsmasq", "ntpd", "polkit")

NIBE_SCRIPTS = ("nibe-gateway", "nibe-updater", "nibe-system", "nibe-os-confirm", "nibe-persist", "nibe-provision",
                "nibe-tailscale-up", "nibe-boot-guard", "nibe-trace", "nibe-smoke", "nibe-wifi-fallback", "sshd",
                "tailscale")

# Absolute paths the appliance needs at runtime (resolved through symlinks inside the root). `*` globs must match.
REQUIRED = (
    # init and service management (our binaries exec /sbin/rc-service, /sbin/rc-update, /sbin/reboot, /bin/sync)
    "sbin/init", "sbin/openrc", "sbin/openrc-run", "sbin/rc-service", "sbin/rc-update", "sbin/supervise-daemon",
    "sbin/start-stop-daemon", "sbin/reboot", "bin/sync", "bin/mount", "bin/mountpoint", "usr/bin/logger",
    "sbin/syslogd", "sbin/klogd", "sbin/watchdog", "sbin/switch_root", "sbin/modprobe", "sbin/udevd",
    # networking, discovery, remote access, privilege
    "usr/sbin/NetworkManager", "usr/bin/nmcli", "sbin/wpa_supplicant", "usr/sbin/dnsmasq", "usr/bin/dbus-daemon",
    "usr/lib/polkit-1/polkitd", "usr/sbin/sshd", "usr/bin/ssh-keygen", "usr/lib/ssh/sshd-session",
    "usr/sbin/avahi-daemon", "usr/bin/sudo", "usr/sbin/tailscaled", "usr/bin/tailscale",
    "usr/share/dbus-1/system-services/fi.w1.wpa_supplicant1.service",
    # D-Bus activation of polkitd (not an OpenRC service) and the setuid helper the system bus activates with.
    "usr/share/dbus-1/system-services/org.freedesktop.PolicyKit1.service", "usr/libexec/dbus-daemon-launch-helper",
    # time, storage, tooling
    "usr/lib/udev/rules.d/60-persistent-storage.rules",  # /dev/disk/by-partuuid (nibe-updater os-install)
    "usr/sbin/chronyd", "sbin/fsck", "sbin/fsck.ext4", "sbin/e2fsck", "sbin/fsck.vfat", "usr/bin/zstd",
    "sbin/mke2fs",  # the first boot of a fresh card creates p3 with it (ADR 0129)
    "usr/bin/curl", "etc/ssl/certs/ca-certificates.crt",
    # radio firmware (BCM43430A1) and regulatory database
    *(f"lib/firmware/{name}" for name in FIRMWARE_KEEP),
    "lib/modules/*/modules.dep", "lib/modules/*/modules.builtin",
    # NIBE components and configuration
    "usr/lib/nibe-gateway/nibe-updater", "usr/lib/nibe-gateway/nibe-system", "usr/lib/nibe-gateway/nibe-persist",
    "usr/lib/nibe-gateway/persistent-state.conf", "usr/lib/nibe-gateway/sshd_config",
    "usr/lib/nibe-gateway/nibe-boot-guard", "usr/lib/nibe-gateway/nibe-tailscale-up",
    "usr/lib/nibe-gateway/nibe-os-confirm", "usr/lib/nibe-gateway/nibe-trace", "usr/lib/nibe-gateway/nibe-reboot",
    "usr/lib/nibe-gateway/nibe-detach", "usr/lib/nibe-gateway/nibe-wifi-fallback", "etc/avahi/avahi-daemon.conf",
    "etc/nibe-gateway/release.pub", "etc/nibe-gateway/hardware.json",
    "opt/nibe-gateway/factory/nibe-gateway", "opt/nibe-gateway/factory/manifest.json",
    "opt/nibe-gateway/factory/admin/index.html",
    "etc/polkit-1/rules.d/49-nibe-gateway-network.rules", "etc/sudoers.d/nibe-admin",
    "etc/NetworkManager/conf.d/nibe.conf", "etc/conf.d/nibe-gateway", "etc/modprobe.d/nibe.conf",
    "var/lib/misc/openrc-shutdowntime",
    *(f"etc/init.d/{name}" for name in NIBE_SCRIPTS),
    # mount points that exist in the image (the overlay root cannot create them before nibe-persist runs)
    "boot/firmware", "var/lib/nibe-gateway", "etc/ssh", "var/lib/NetworkManager",
    "etc/NetworkManager/system-connections", "var/lib/tailscale",
)


def _passwd(root: Path, name: str) -> dict[str, list[str]]:
    path = root / "etc" / name
    if not path.exists():
        return {}
    return {line.split(":")[0]: line.split(":") for line in path.read_text().splitlines() if line.strip()}


def ini_section(text: str, section: str) -> dict[str, str]:
    """key=value pairs of one [section] of an INI-style file (comments and other sections ignored)."""
    values, current = {}, None
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1]
        elif current == section and "=" in line:
            key, value = line.split("=", 1)
            values[key.strip()] = value.strip()
    return values


def check_avahi_conf(root: Path) -> list[str]:
    path = root / "etc/avahi/avahi-daemon.conf"
    if not path.exists():
        return ["etc/avahi/avahi-daemon.conf missing"]
    text = path.read_text()
    server, publish = ini_section(text, "server"), ini_section(text, "publish")
    errors = []
    if server.get("allow-interfaces") != "wlan0":
        errors.append("avahi must use wlan0 only (allow-interfaces=wlan0): on lo/tailscale0 it renames itself -2")
    if server.get("enable-chroot", "yes") != "yes":
        errors.append("avahi chroot must stay enabled")
    for key, want in (("publish-hinfo", "no"), ("publish-workstation", "no")):
        if publish.get(key) != want:
            errors.append(f"avahi {key} must be {want} (as on Raspberry Pi OS)")
    return errors


def check_dbus_activation(root: Path) -> list[str]:
    """polkitd is D-Bus activated: the activation file must name an existing binary, run as root."""
    path = root / "usr/share/dbus-1/system-services/org.freedesktop.PolicyKit1.service"
    if not path.exists():
        return ["D-Bus activation file for polkit missing"]
    service = ini_section(path.read_text(), "D-BUS Service")
    errors = []
    exe = service.get("Exec", "").split()
    if not exe or resolve(root, exe[0]) is None:
        errors.append(f"polkit activation Exec {exe[:1]} does not exist")
    if service.get("User") != "root" or service.get("Name") != "org.freedesktop.PolicyKit1":
        errors.append("polkit activation file must name org.freedesktop.PolicyKit1 and run as root")
    return errors


def check_root(root: Path, uid: int, gid: int, profile: str = "hardware") -> list[str]:
    """Runtime guard on a finished root. Returns error strings (empty = pass)."""
    errors: list[str] = []
    for need in REQUIRED:
        if "*" in need:
            if not any(resolve(root, p.relative_to(root)) for p in root.glob(need)):
                errors.append(f"required {need} is missing")
        elif resolve(root, need) is None:
            errors.append(f"required {need} is missing")
    kernels = sorted(p for p in (root / "lib/modules").glob("*") if (p / "modules.dep").exists()) \
        if (root / "lib/modules").is_dir() else []
    if len(kernels) != 1:
        errors.append(f"expected exactly one kernel in lib/modules, found {len(kernels)}")
    else:
        deps = parse_modules_dep((kernels[0] / "modules.dep").read_text())
        builtin = parse_builtin((kernels[0] / "modules.builtin").read_text()) \
            if (kernels[0] / "modules.builtin").exists() else set()
        present = {MODULE_SUFFIX.sub("", m) for m in deps if (kernels[0] / m).exists()}
        for module in REQUIRED_MODULES:
            if MODULE_SUFFIX.sub("", module) not in present | builtin:
                errors.append(f"kernel module {module} is neither installed nor built in")
    if any((root / "lib/firmware").rglob("*.zst")):
        errors.append("compressed .zst firmware left in lib/firmware (the kernel cannot read it)")
    errors.extend(check_avahi_conf(root))
    errors.extend(check_dbus_activation(root))
    for name in NIBE_SCRIPTS:
        script = resolve(root, f"etc/init.d/{name}")
        if script and "pidfile" in script.read_text():
            errors.append(f"etc/init.d/{name} uses a pidfile (finished one-shots would show as crashed)")
    avahi = "etc/avahi/services/nibe.service" + (".disabled" if profile == "simulated" else "")
    if resolve(root, avahi) is None:
        errors.append(f"required {avahi} is missing")

    # Runlevels: exactly the declared services, every link resolving to an init script.
    for level, services in RUNLEVELS.items():
        directory = root / "etc/runlevels" / level
        present = sorted(p.name for p in directory.iterdir()) if directory.is_dir() else []
        for service in services:
            if service not in present:
                errors.append(f"runlevel {level} lacks {service}")
        for name in present:
            if name not in services:
                errors.append(f"runlevel {level} has undeclared service {name}")
            link = directory / name
            if not link.is_symlink() or resolve(root, f"etc/runlevels/{level}/{name}") is None:
                errors.append(f"runlevel link {level}/{name} does not resolve to an init script")
            if name in FORBIDDEN_SERVICES:
                errors.append(f"service {name} must not be enabled")
    for name in ("mdev", "hwdrivers"):
        if (root / "etc/init.d" / name).exists():
            errors.append(f"etc/init.d/{name} must be removed (udev-trigger alone provides dev)")
    if (root / "run/systemd/system").exists():
        errors.append("run/systemd/system must not exist (our binaries would pick systemd)")
    for name in NIBE_SCRIPTS:
        script = resolve(root, f"etc/init.d/{name}")
        if script and not script.stat().st_mode & stat.S_IXUSR:
            errors.append(f"etc/init.d/{name} is not executable")
        if script and not script.read_text().startswith("#!/sbin/openrc-run"):
            errors.append(f"etc/init.d/{name} is not an openrc-run script")

    # Console: never a getty on the RS485 UART.
    inittab = (root / "etc/inittab").read_text() if (root / "etc/inittab").exists() else ""
    for line in inittab.splitlines():
        if line.lstrip().startswith("#"):
            continue
        if re.search(r"tty(AMA|S)\d|serial\d", line):
            errors.append(f"inittab uses the RS485 UART: {line}")
    for need in ("::sysinit:/sbin/openrc sysinit", "::sysinit:/sbin/openrc boot", "::wait:/sbin/openrc default",
                 "::shutdown:/sbin/openrc shutdown"):
        if need not in inittab:
            errors.append(f"inittab lacks {need}")

    # fstab: label mounts only (bundles boot on other cards), boot partition read-only, no "/" line.
    fstab = (root / "etc/fstab").read_text() if (root / "etc/fstab").exists() else ""
    entries = [line.split() for line in fstab.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    by_target = {e[1]: e for e in entries if len(e) >= 4}
    if "PARTUUID=" in fstab or "UUID=" in fstab.replace("PARTUUID=", ""):
        errors.append("fstab must mount by LABEL only (ADR 0106)")
    data = by_target.get("/var/lib/nibe-gateway")
    if not data or data[0] != "LABEL=nibe-data" or data[2] != "ext4":
        errors.append("fstab must mount LABEL=nibe-data on /var/lib/nibe-gateway as ext4")
    boot = by_target.get("/boot/firmware")
    if not boot or boot[0] != "LABEL=bootfs" or boot[2] != "vfat" or "ro" not in boot[3].split(","):
        errors.append("fstab must mount LABEL=bootfs on /boot/firmware as vfat, read-only")
    if "/" in by_target:
        errors.append("fstab must not mount / (the initramfs mounts the slot root)")

    # Accounts: p3 state is owned by the Raspberry Pi OS uid/gid, sshd refuses "!"-locked accounts even for keys.
    passwd, group, shadow = _passwd(root, "passwd"), _passwd(root, "group"), _passwd(root, "shadow")
    gw = passwd.get("nibe-gateway")
    if not gw or gw[2] != str(uid) or gw[3] != str(gid):
        errors.append(f"nibe-gateway must be uid {uid} gid {gid}")
    if not group.get("nibe-gateway") or group["nibe-gateway"][2] != str(gid):
        errors.append(f"group nibe-gateway must be gid {gid}")
    if "nibe-gateway" not in (group.get("dialout") or ["", "", "", ""])[3].split(","):
        errors.append("nibe-gateway must be in dialout (/dev/ttyAMA0)")
    if not passwd.get("nibe-admin"):
        errors.append("nibe-admin account missing")
    elif (shadow.get("nibe-admin") or ["", ""])[1] != "*":
        errors.append("nibe-admin password field must be '*' (no password, not '!'-locked: sshd rejects keys)")
    for name, fields in shadow.items():
        if len(fields) > 1 and fields[1] == "":
            errors.append(f"account {name} has an empty password")
    if (shadow.get("root") or ["", ""])[1] not in ("*", "!"):
        errors.append("root must have no usable password")
    sudoers = root / "etc/sudoers.d/nibe-admin"
    if sudoers.exists() and stat.S_IMODE(sudoers.stat().st_mode) != 0o440:
        errors.append("etc/sudoers.d/nibe-admin must be mode 0440")
    return errors


# --------------------------------------------------------------------------------------------- ELF

def check_elf(path: Path) -> str | None:
    """None if `path` is a static 32-bit little-endian ARM executable, else the reason."""
    data = path.read_bytes()
    if data[:4] != b"\x7fELF":
        return "not an ELF file"
    if data[4] != 1 or data[5] != 1:
        return "not a 32-bit little-endian ELF"
    e_type, e_machine = struct.unpack_from("<HH", data, 16)
    if e_machine != 40:
        return f"not ARM (e_machine {e_machine})"
    if e_type not in (2, 3):
        return "not an executable"
    e_phoff, = struct.unpack_from("<I", data, 28)
    e_phentsize, e_phnum = struct.unpack_from("<HH", data, 42)
    for index in range(e_phnum):
        p_type, = struct.unpack_from("<I", data, e_phoff + index * e_phentsize)
        if p_type == 3:  # PT_INTERP
            return "dynamically linked (has PT_INTERP); Alpine has no glibc, build the static musl target"
    return None


# --------------------------------------------------------------------------------------------- dtb

def parse_fdt(data: bytes) -> dict[str, dict[str, bytes]]:
    """Flattened device tree -> {node path: {property: raw value}} (spec v17 structure block)."""
    magic, _total, off_struct, off_strings, *_ = struct.unpack_from(">10I", data, 0)
    if magic != 0xD00DFEED:
        raise ValueError("not a flattened device tree")
    nodes: dict[str, dict[str, bytes]] = {}
    path: list[str] = []
    pos = off_struct
    while True:
        token, = struct.unpack_from(">I", data, pos)
        pos += 4
        if token == 1:  # FDT_BEGIN_NODE
            end = data.index(b"\0", pos)
            name = data[pos:end].decode()
            pos = (end + 4) & ~3
            path.append(name)
            nodes.setdefault("/" + "/".join(p for p in path if p), {})
        elif token == 2:  # FDT_END_NODE
            path.pop()
        elif token == 3:  # FDT_PROP
            length, name_off = struct.unpack_from(">II", data, pos)
            pos += 8
            value = data[pos:pos + length]
            pos = (pos + length + 3) & ~3
            name = data[off_strings + name_off:data.index(b"\0", off_strings + name_off)].decode()
            nodes["/" + "/".join(p for p in path if p)][name] = value
        elif token == 4:  # FDT_NOP
            continue
        elif token == 9:  # FDT_END
            return nodes
        else:
            raise ValueError(f"bad FDT token {token}")


def fdt_string(nodes, node: str, prop: str) -> str | None:
    value = nodes.get(node, {}).get(prop)
    return value.rstrip(b"\0").decode() if value is not None else None


# Pi Zero W: PL011 (uart0) and the mini UART (uart1); the Bluetooth serdev node sits under the PL011.
PL011 = "/soc/serial@7e201000"
MINI_UART = "/soc/serial@7e215040"


def check_dtb(data: bytes) -> list[str]:
    """The shipped Zero W dtb has disable-bt applied: serial0 = PL011 (ttyAMA0 for the RS485 HAT), Bluetooth off,
    and keeps __symbols__ so the firmware can still apply config.txt's overlays."""
    nodes = parse_fdt(data)
    errors = []
    if fdt_string(nodes, "/aliases", "serial0") != PL011:
        errors.append(f"aliases/serial0 is {fdt_string(nodes, '/aliases', 'serial0')}, not the PL011 {PL011}")
    if fdt_string(nodes, "/aliases", "serial1") != MINI_UART:
        errors.append("aliases/serial1 must be the mini UART")
    if fdt_string(nodes, PL011 + "/bluetooth", "status") != "disabled":
        errors.append("the Bluetooth node under the PL011 is not disabled")
    if fdt_string(nodes, PL011, "status") != "okay":
        errors.append("the PL011 is not enabled")
    if "/__symbols__" not in nodes:
        errors.append("dtb lost __symbols__ (the firmware could no longer apply overlays)")
    return errors


# ----------------------------------------------------------------------------------------- summary

def tree_summary(root: Path) -> str:
    files = size = links = 0
    for directory, dirnames, filenames in os.walk(root):
        for name in filenames + [d for d in dirnames if os.path.islink(os.path.join(directory, d))]:
            path = os.path.join(directory, name)
            info = os.lstat(path)
            if stat.S_ISLNK(info.st_mode):
                links += 1
            elif stat.S_ISREG(info.st_mode):
                files += 1
                size += info.st_size
    return f"files={files} bytes={size} links={links}"


# ------------------------------------------------------------------------------------------------ cli

def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("world").add_argument("file", type=Path)
    cmd = sub.add_parser("cmdline")
    cmd.add_argument("country")
    cmd.add_argument("--smoke", action="store_true")
    for name in ("prune-modules", "initramfs-modules"):
        p = sub.add_parser(name)
        p.add_argument("root", type=Path)
        p.add_argument("kver")
    sub.add_parser("prune-firmware").add_argument("root", type=Path)
    check = sub.add_parser("check")
    check.add_argument("root", type=Path)
    check.add_argument("--uid", type=int, required=True)
    check.add_argument("--gid", type=int, required=True)
    check.add_argument("--profile", choices=("hardware", "simulated"), default="hardware")
    sub.add_parser("check-elf").add_argument("files", type=Path, nargs="+")
    sub.add_parser("summary").add_argument("root", type=Path)
    sub.add_parser("check-dtb").add_argument("dtb", type=Path)
    args = parser.parse_args(argv)

    try:
        if args.command == "world":
            print("\n".join(parse_world(args.file.read_text())))
        elif args.command == "cmdline":
            text = slot_cmdline(args.country, smoke=args.smoke)
            errors = check_cmdline(text)
            if errors:
                raise ValueError("; ".join(errors))
            sys.stdout.write(text)
        elif args.command == "prune-modules":
            kept, removed = prune_modules(args.root, args.kver)
            print(f"modules: kept {kept}, removed {removed}")
        elif args.command == "initramfs-modules":
            base = args.root / "lib/modules" / args.kver
            deps = parse_modules_dep((base / "modules.dep").read_text())
            builtin = parse_builtin((base / "modules.builtin").read_text())
            print("\n".join(ordered_load(deps, INITRAMFS_MODULES, builtin)))
        elif args.command == "prune-firmware":
            kept, removed = prune_firmware(args.root)
            count = decompress_firmware(args.root)
            print(f"firmware: kept {len(kept)}, removed {removed}, decompressed {count}")
        elif args.command == "check":
            errors = check_root(args.root, args.uid, args.gid, args.profile)
            for error in errors:
                print(f"alpine guard: {error}", file=sys.stderr)
            if errors:
                return 1
            print(f"alpine guard: {len(REQUIRED)} required paths, {sum(map(len, RUNLEVELS.values()))} "
                  "runlevel services, accounts, fstab and inittab pass")
        elif args.command == "check-elf":
            bad = [(f, check_elf(f)) for f in args.files]
            bad = [(f, reason) for f, reason in bad if reason]
            for f, reason in bad:
                print(f"{f}: {reason}", file=sys.stderr)
            return 1 if bad else 0
        elif args.command == "check-dtb":
            errors = check_dtb(args.dtb.read_bytes())
            for error in errors:
                print(f"dtb: {error}", file=sys.stderr)
            if errors:
                return 1
            print("dtb: disable-bt applied (serial0 = PL011, Bluetooth disabled), __symbols__ kept")
        elif args.command == "summary":
            print(tree_summary(args.root))
    except (ValueError, OSError) as error:
        print(f"alpine_image: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
