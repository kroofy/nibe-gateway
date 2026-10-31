"""Alpine image build logic (ADR 0117) that runs without Docker: world parsing, cmdline, module/firmware pruning,
the runtime guard, the initramfs root resolver and the OpenRC scripts. The container build itself is
scripts/build-alpine-os-bundle.sh (and HP-38 on the Pi)."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
ALPINE = REPO / "packaging/alpine"
spec = importlib.util.spec_from_file_location("alpine_image", ALPINE / "alpine_image.py")
image = importlib.util.module_from_spec(spec)
spec.loader.exec_module(image)


class WorldTest(unittest.TestCase):
    def test_real_world_file_parses_and_lists_the_required_packages(self):
        names = image.parse_world((ALPINE / "world").read_text())
        for name in image.REQUIRED_WORLD:
            self.assertIn(name, names)
        self.assertNotIn("raspberrypi-bootloader", names, "shared firmware is never in an OS bundle (ADR 0106)")
        self.assertNotIn("python3", names, "no Python on the appliance (ADR 0116)")

    def test_every_package_is_justified_by_a_comment_above_it(self):
        lines = (ALPINE / "world").read_text().splitlines()
        for index, line in enumerate(lines):
            if line.strip() and not line.startswith("#"):
                block = index
                while block > 0 and lines[block - 1].strip() and not lines[block - 1].startswith("#"):
                    block -= 1
                self.assertTrue(block > 0 and lines[block - 1].startswith("#"), f"{line} has no comment")

    def test_comments_blanks_and_validation(self):
        base = "\n".join(image.REQUIRED_WORLD)
        self.assertEqual(image.parse_world(f"# c\n\n{base}\nfoo  # why\n")[-1], "foo")
        for bad in ("Foo", "foo bar", "../x", "foo;rm"):
            with self.assertRaises(ValueError):
                image.parse_world(f"{base}\n{bad}\n")
        with self.assertRaises(ValueError):
            image.parse_world(f"{base}\nzstd\n")
        with self.assertRaisesRegex(ValueError, "dnsmasq"):
            image.parse_world(base.replace("dnsmasq\n", ""))


class CmdlineTest(unittest.TestCase):
    def test_template_matches_the_updater_contract(self):
        text = image.slot_cmdline("SE")
        self.assertEqual(image.check_cmdline(text), [])
        tokens = text.split()
        self.assertIn("root=PARTUUID=00000000-02", tokens)
        self.assertIn("rootfstype=squashfs", tokens)
        self.assertIn("panic=10", tokens)
        self.assertIn("nibe.slot=a", tokens)
        self.assertIn("cfg80211.ieee80211_regdom=SE", tokens)
        self.assertEqual(tokens[-1], "nibe.slot=a")
        # os-install drops root= and nibe.slot= and appends its own (crates/updater/src/os.rs slot_cmdline).
        kept = [t for t in tokens if not t.startswith(("root=", "nibe.slot="))]
        slot_b = " ".join(kept + ["root=PARTUUID=15be6592-04", "nibe.slot=b"])
        self.assertIn("rootfstype=squashfs", slot_b)

    def test_no_quiet_and_smoke_flag(self):
        self.assertNotIn("quiet", image.slot_cmdline("SE").split(), "HDMI must show the whole boot")
        smoke = image.slot_cmdline("SE", smoke=True)
        self.assertEqual(image.check_cmdline(smoke), [])
        self.assertIn("nibe.smoke=1", smoke.split())
        self.assertNotIn("nibe.smoke=1", image.slot_cmdline("SE").split())
        self.assertTrue(image.check_cmdline(image.slot_cmdline("SE").replace("rootwait", "rootwait quiet")))

    def test_rejects_bad_country_and_serial_console(self):
        with self.assertRaises(ValueError):
            image.slot_cmdline("se")
        bad = image.slot_cmdline("SE").replace("console=tty1", "console=ttyAMA0,115200")
        self.assertTrue(any("serial console" in e for e in image.check_cmdline(bad)))
        self.assertTrue(image.check_cmdline(image.slot_cmdline("SE").replace("\n", " init=/bin/sh\n")))
        self.assertTrue(image.check_cmdline(image.slot_cmdline("SE", "ext4")))


MODULES_DEP = """\
kernel/fs/squashfs/squashfs.ko.xz:
kernel/fs/overlayfs/overlay.ko.xz:
kernel/net/wireless/cfg80211.ko.xz: kernel/net/rfkill/rfkill.ko.xz
kernel/net/rfkill/rfkill.ko.xz:
kernel/drivers/net/wireless/broadcom/brcm80211/brcmfmac/brcmfmac.ko.xz: kernel/drivers/net/wireless/broadcom/brcm80211/brcmutil/brcmutil.ko.xz kernel/net/wireless/cfg80211.ko.xz
kernel/drivers/net/wireless/broadcom/brcm80211/brcmutil/brcmutil.ko.xz:
kernel/drivers/net/tun.ko.xz:
kernel/lib/crc-thing.ko.xz:
kernel/net/netfilter/nf_tables.ko.xz: kernel/lib/crc-thing.ko.xz
kernel/sound/snd.ko.xz:
kernel/drivers/gpu/drm/vc4/vc4.ko.xz: kernel/sound/snd.ko.xz
"""


class ModulesTest(unittest.TestCase):
    def test_closure_keeps_dependencies_and_drops_the_rest(self):
        deps = image.parse_modules_dep(MODULES_DEP)
        kept = image.module_closure(deps)
        self.assertIn("kernel/net/rfkill/rfkill.ko.xz", kept)
        self.assertIn("kernel/lib/crc-thing.ko.xz", kept, "dependency outside the allowlist")
        self.assertNotIn("kernel/sound/snd.ko.xz", kept)
        self.assertNotIn("kernel/drivers/gpu/drm/vc4/vc4.ko.xz", kept)

    def test_initramfs_load_order(self):
        deps = image.parse_modules_dep(MODULES_DEP)
        builtin = image.parse_builtin("kernel/fs/fat/fat.ko\nkernel/fs/fat/vfat.ko\nkernel/fs/nls/nls_cp437.ko\n"
                                      "kernel/fs/nls/nls_ascii.ko\n")
        self.assertEqual(image.ordered_load(deps, image.INITRAMFS_MODULES, builtin),
                         ["kernel/fs/squashfs/squashfs.ko.xz", "kernel/fs/overlayfs/overlay.ko.xz"])
        with self.assertRaises(ValueError):
            image.ordered_load(deps, image.INITRAMFS_MODULES)  # vfat neither a module nor built in
        order = image.ordered_load(deps, ["kernel/drivers/net/wireless/broadcom/brcm80211/brcmfmac/brcmfmac.ko"])
        self.assertLess(order.index("kernel/net/rfkill/rfkill.ko.xz"), order.index("kernel/net/wireless/cfg80211.ko.xz"))
        with self.assertRaises(ValueError):
            image.ordered_load(deps, ["kernel/fs/nope.ko"])

    def test_prune_modules_on_disk(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "lib/modules/6.18.58-0-rpi"
            deps = image.parse_modules_dep(MODULES_DEP)
            for module in deps:
                (base / module).parent.mkdir(parents=True, exist_ok=True)
                (base / module).write_text("ko")
            (base / "modules.dep").write_text(MODULES_DEP)
            kept, removed = image.prune_modules(root, "6.18.58-0-rpi")
            self.assertEqual((kept, removed), (9, 2))
            self.assertFalse((base / "kernel/sound").exists(), "empty directories go too")
            self.assertTrue((base / "kernel/drivers/net/tun.ko.xz").exists())


class FirmwareTest(unittest.TestCase):
    def test_keeps_the_zero_w_chain_and_regdb_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fw = root / "lib/firmware"
            (fw / "brcm").mkdir(parents=True)
            (fw / "cypress").mkdir()
            (fw / "synaptics").mkdir()
            for name in ("cypress/cyfmac43430-sdio.bin.zst", "cypress/cyfmac43430-sdio.rpiltd.bin.zst",
                         "cypress/cyfmac43430-sdio.clm_blob.zst", "cypress/cyfmac43455-sdio.bin.zst",
                         "brcm/brcmfmac43430-sdio.txt.zst", "brcm/BCM43430A1.hcd.zst", "synaptics/x.zst",
                         "regulatory.db", "regulatory.db.p7s"):
                if name.endswith(".zst"):
                    subprocess.run(["zstd", "-q", "-o", str(fw / name), "-"], input=f"fw {name}".encode(), check=True)
                else:
                    (fw / name).write_text("fw")
            links = {
                "brcm/brcmfmac43430-sdio.bin.zst": "../cypress/cyfmac43430-sdio.bin.zst",
                "brcm/brcmfmac43430-sdio.clm_blob.zst": "../cypress/cyfmac43430-sdio.clm_blob.zst",
                "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.bin.zst": "../cypress/cyfmac43430-sdio.rpiltd.bin.zst",
                "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.clm_blob.zst": "../cypress/cyfmac43430-sdio.clm_blob.zst",
                "brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.txt.zst": "brcmfmac43430-sdio.txt.zst",
                "brcm/brcmfmac43455-sdio.bin.zst": "../cypress/cyfmac43455-sdio.bin.zst",
            }
            for name, target in links.items():
                os.symlink(target, fw / name)
            kept, removed = image.prune_firmware(root)
            self.assertEqual(removed, 4)
            self.assertEqual(image.decompress_firmware(root), 4)
            for name in image.FIRMWARE_KEEP:
                self.assertIsNotNone(image.resolve(root, f"lib/firmware/{name}"), name)
            self.assertEqual(list(fw.rglob("*.zst")), [], "the Raspberry Pi OS kernel cannot read .zst firmware")
            self.assertEqual(image.resolve(root, "lib/firmware/brcm/brcmfmac43430-sdio.raspberrypi,model-zero-w.bin")
                             .read_text(), "fw cypress/cyfmac43430-sdio.rpiltd.bin.zst")
            self.assertFalse((fw / "synaptics").exists())
            self.assertFalse((fw / "cypress/cyfmac43455-sdio.bin.zst").exists())


# Alpine's avahi-daemon.conf as the build leaves it (defaults + allow-interfaces=wlan0).
AVAHI_CONF = """[server]
use-ipv4=yes
use-ipv6=yes
allow-interfaces=wlan0
#deny-interfaces=eth1
ratelimit-interval-usec=1000000
ratelimit-burst=1000

[wide-area]
enable-wide-area=yes

[publish]
publish-hinfo=no
publish-workstation=no
"""


def fixture_root(root: Path, uid=995, gid=992):
    """A root that passes the guard: the real rootfs overlay plus placeholders for package files."""
    shutil.copytree(ALPINE / "rootfs", root, symlinks=True)
    for need in image.REQUIRED:
        path = root / need.replace("*", "6.18.58-0-rpi")
        if need.endswith("ko*"):
            path = path.with_name(path.name[:-1] + ".xz")
        if path.exists():
            continue
        if need in ("boot/firmware", "var/lib/nibe-gateway", "etc/ssh", "var/lib/NetworkManager",
                    "etc/NetworkManager/system-connections", "var/lib/tailscale"):
            path.mkdir(parents=True, exist_ok=True)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("x")
    kernel = root / "lib/modules/6.12.109+rpt-rpi-v6"
    for leftover in (root / "lib/modules").glob("*"):
        if leftover != kernel:
            shutil.rmtree(leftover)
    kernel.mkdir(parents=True, exist_ok=True)
    modular = [m for m in image.REQUIRED_MODULES if "/fat/" not in m and "/nls/" not in m and "packet" not in m]
    for module in modular:
        (kernel / (module + ".xz")).parent.mkdir(parents=True, exist_ok=True)
        (kernel / (module + ".xz")).write_text("ko")
    (kernel / "modules.dep").write_text("".join(f"{m}.xz:\n" for m in modular))
    (kernel / "modules.builtin").write_text("kernel/fs/fat/vfat.ko\nkernel/fs/nls/nls_cp437.ko\nkernel/net/packet/af_packet.ko\n")
    (root / "etc/avahi").mkdir(parents=True, exist_ok=True)
    (root / "etc/avahi/avahi-daemon.conf").write_text(AVAHI_CONF)
    (root / "usr/share/dbus-1/system-services/org.freedesktop.PolicyKit1.service").write_text(
        "[D-BUS Service]\nName=org.freedesktop.PolicyKit1\nExec=/usr/lib/polkit-1/polkitd --no-debug\nUser=root\n"
        "SystemdService=polkit.service\n")
    (root / "etc/avahi/services").mkdir(parents=True, exist_ok=True)
    (root / "etc/avahi/services/nibe.service").write_text("<service-group/>")
    for level, services in image.RUNLEVELS.items():
        (root / "etc/runlevels" / level).mkdir(parents=True)
        for service in services:
            script = root / "etc/init.d" / service
            if not script.exists():
                script.write_text("#!/sbin/openrc-run\n")
                script.chmod(0o755)
            os.symlink(f"/etc/init.d/{service}", root / "etc/runlevels" / level / service)
    (root / "etc/passwd").write_text(f"root:x:0:0:root:/root:/bin/sh\nnibe-gateway:x:{uid}:{gid}::/x:/sbin/nologin\n"
                                     "nibe-admin:x:1000:1000::/home/nibe-admin:/bin/sh\n")
    (root / "etc/group").write_text(f"root:x:0:root\ndialout:x:20:root,nibe-gateway\nnibe-gateway:x:{gid}:nibe-gateway\n"
                                    "nibe-admin:x:1000:\n")
    (root / "etc/shadow").write_text("root:*::0:::::\nnibe-gateway:!:1:0:99999:7:::\nnibe-admin:*:1:0:99999:7:::\n")
    (root / "etc/sudoers.d/nibe-admin").chmod(0o440)


class GuardTest(unittest.TestCase):
    def check(self, mutate=None, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "root"
            fixture_root(root)
            if mutate:
                mutate(root)
            return image.check_root(root, kwargs.get("uid", 995), kwargs.get("gid", 992))

    def test_fixture_passes(self):
        self.assertEqual(self.check(), [])

    def test_missing_required_file(self):
        errors = self.check(lambda r: (r / "usr/sbin/NetworkManager").unlink())
        self.assertIn("required usr/sbin/NetworkManager is missing", errors)

    def test_dangling_or_extra_runlevel_link(self):
        def dangling(r):
            (r / "etc/init.d/nibe-gateway").unlink()
        self.assertTrue(any("nibe-gateway" in e for e in self.check(dangling)))
        def extra(r):
            (r / "etc/init.d/mdev").write_text("#!/sbin/openrc-run\n")
            os.symlink("/etc/init.d/mdev", r / "etc/runlevels/sysinit/mdev")
        errors = self.check(extra)
        self.assertTrue(any("undeclared service mdev" in e for e in errors))
        self.assertTrue(any("must not be enabled" in e for e in errors))
        self.assertTrue(any("must be removed" in e for e in errors))

    def test_serial_getty_and_fstab(self):
        errors = self.check(lambda r: (r / "etc/inittab").write_text(
            (r / "etc/inittab").read_text() + "ttyAMA0::respawn:/sbin/getty -L 115200 ttyAMA0 vt100\n"))
        self.assertTrue(any("RS485 UART" in e for e in errors))
        errors = self.check(lambda r: (r / "etc/fstab").write_text(
            (r / "etc/fstab").read_text().replace("LABEL=nibe-data", "PARTUUID=deadbeef-03")))
        self.assertTrue(any("LABEL only" in e for e in errors))
        errors = self.check(lambda r: (r / "etc/fstab").write_text(
            (r / "etc/fstab").read_text().replace("defaults,ro ", "defaults ")))
        self.assertTrue(any("read-only" in e for e in errors))

    def test_avahi_config(self):
        conf = lambda text: (lambda r: (r / "etc/avahi/avahi-daemon.conf").write_text(text))
        errors = self.check(conf(AVAHI_CONF.replace("allow-interfaces=wlan0", "#allow-interfaces=eth0")))
        self.assertTrue(any("wlan0 only" in e for e in errors))
        errors = self.check(conf(AVAHI_CONF.replace("[publish]", "[publish]\nenable-chroot=no")))
        self.assertEqual(errors, [], "enable-chroot outside [server] is ignored")
        errors = self.check(conf(AVAHI_CONF.replace("[server]", "[server]\nenable-chroot=no")))
        self.assertTrue(any("chroot" in e for e in errors))
        errors = self.check(conf(AVAHI_CONF.replace("publish-workstation=no", "publish-workstation=yes")))
        self.assertTrue(any("publish-workstation" in e for e in errors))

    def test_polkit_is_dbus_activated_not_an_openrc_service(self):
        def enable(r):
            (r / "etc/init.d/polkit").write_text("#!/sbin/openrc-run\n")
            os.symlink("/etc/init.d/polkit", r / "etc/runlevels/default/polkit")
        self.assertTrue(any("polkit must not be enabled" in e or "service polkit must not be enabled" in e
                            for e in self.check(enable)))
        broken = lambda r: (r / "usr/lib/polkit-1/polkitd").unlink()
        self.assertTrue(any("activation Exec" in e for e in self.check(broken)))

    def test_one_shots_have_no_pidfile(self):
        add = lambda r: (r / "etc/init.d/nibe-os-confirm").write_text("#!/sbin/openrc-run\npidfile=/run/x.pid\n")
        self.assertTrue(any("pidfile" in e for e in self.check(add)))

    def test_accounts(self):
        self.assertTrue(any("uid 1000" in e for e in self.check(uid=1000)))
        errors = self.check(lambda r: (r / "etc/shadow").write_text(
            (r / "etc/shadow").read_text().replace("nibe-admin:*", "nibe-admin:!")))
        self.assertTrue(any("sshd rejects keys" in e for e in errors))
        errors = self.check(lambda r: (r / "etc/shadow").write_text("root::0:::::\nnibe-admin:*:1::::::\n"))
        self.assertTrue(any("empty password" in e for e in errors))
        errors = self.check(lambda r: (r / "etc/group").write_text(
            (r / "etc/group").read_text().replace("root,nibe-gateway", "root")))
        self.assertTrue(any("dialout" in e for e in errors))
        self.assertTrue(any("systemd" in e for e in self.check(lambda r: (r / "run/systemd/system").mkdir(parents=True))))


class ElfTest(unittest.TestCase):
    def elf(self, machine=40, interp=False):
        header = bytearray(52)
        header[:6] = b"\x7fELF\x01\x01"
        header[16:20] = (2).to_bytes(2, "little") + machine.to_bytes(2, "little")
        header[28:32] = (52).to_bytes(4, "little")
        header[42:46] = (32).to_bytes(2, "little") + (1).to_bytes(2, "little")
        phdr = (3 if interp else 1).to_bytes(4, "little") + bytes(28)
        return bytes(header) + phdr

    def test_static_arm_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bin"
            path.write_bytes(self.elf())
            self.assertIsNone(image.check_elf(path))
            path.write_bytes(self.elf(interp=True))
            self.assertIn("dynamically linked", image.check_elf(path))
            path.write_bytes(self.elf(machine=62))
            self.assertIn("not ARM", image.check_elf(path))
            path.write_text("#!/bin/sh\n")
            self.assertEqual(image.check_elf(path), "not an ELF file")


def build_fdt(tree):
    """Minimal FDT encoder for tests: tree = {name: {prop: bytes | dict(child)}}."""
    import struct
    strings, block = bytearray(), bytearray()

    def name_off(name):
        key = name.encode() + b"\0"
        at = strings.find(key)
        if at < 0 or (at > 0 and strings[at - 1] != 0):
            at = len(strings)
            strings.extend(key)
        return at

    def node(name, content):
        block.extend(struct.pack(">I", 1) + name.encode() + b"\0")
        while len(block) % 4:
            block.append(0)
        for key, value in content.items():
            if isinstance(value, dict):
                continue
            block.extend(struct.pack(">III", 3, len(value), name_off(key)) + value)
            while len(block) % 4:
                block.append(0)
        for key, value in content.items():
            if isinstance(value, dict):
                node(key, value)
        block.extend(struct.pack(">I", 2))

    node("", tree)
    block.extend(struct.pack(">I", 9))
    off_struct = 40 + 16
    off_strings = off_struct + len(block)
    total = off_strings + len(strings)
    header = struct.pack(">10I", 0xD00DFEED, total, off_struct, off_strings, 40, 17, 16, 0, len(strings), len(block))
    return header + bytes(16) + bytes(block) + bytes(strings)


class DtbTest(unittest.TestCase):
    def tree(self, serial0, bt):
        return {
            "aliases": {"serial0": serial0, "serial1": b"/soc/serial@7e215040\0"},
            "soc": {"serial@7e201000": {"status": b"okay\0", "bluetooth": {"status": bt}},
                    "serial@7e215040": {"status": b"disabled\0"}},
            "__symbols__": {"uart0": b"/soc/serial@7e201000\0"},
        }

    def test_disable_bt_applied(self):
        good = build_fdt(self.tree(b"/soc/serial@7e201000\0", b"disabled\0"))
        self.assertEqual(image.check_dtb(good), [])
        nodes = image.parse_fdt(good)
        self.assertEqual(image.fdt_string(nodes, "/soc/serial@7e201000/bluetooth", "status"), "disabled")

    def test_stock_zero_w_tree_is_rejected(self):
        # Without disable-bt: serial0 is the mini UART and Bluetooth owns the PL011 (what alpine.4-smoke showed).
        stock = build_fdt(self.tree(b"/soc/serial@7e215040\0", b"okay\0"))
        errors = image.check_dtb(stock)
        self.assertTrue(any("serial0" in e for e in errors) and any("Bluetooth" in e for e in errors))
        with self.assertRaises(ValueError):
            image.parse_fdt(b"\0" * 64)


class InitramfsTest(unittest.TestCase):
    INIT = ALPINE / "initramfs/init"

    def resolve(self, spec, disks):
        with tempfile.TemporaryDirectory() as tmp:
            sys_dir, dev = Path(tmp) / "sys", Path(tmp) / "dev"
            (sys_dir / "block").mkdir(parents=True)
            dev.mkdir()
            for name, signature in disks.items():
                (sys_dir / "block" / name).mkdir()
                (dev / name).write_bytes(bytes(440) + bytes.fromhex(signature)[::-1] + bytes(72))
            result = subprocess.run(["sh", "-c", f'. "{self.INIT}" --lib; resolve_root "$1"', "sh", spec],
                                    env={**os.environ, "SYS": str(sys_dir), "DEV": str(dev)},
                                    capture_output=True, text=True)
            return result.returncode, result.stdout.strip().replace(str(dev), "/dev")

    def test_partuuid_resolution(self):
        disks = {"mmcblk0": "15be6592", "sda": "deadbeef"}
        self.assertEqual(self.resolve("PARTUUID=15be6592-02", disks), (0, "/dev/mmcblk0p2"))
        self.assertEqual(self.resolve("PARTUUID=15be6592-04", disks), (0, "/dev/mmcblk0p4"))
        self.assertEqual(self.resolve("PARTUUID=deadbeef-10", disks), (0, "/dev/sda10"))
        self.assertEqual(self.resolve("/dev/mmcblk0p2", disks), (0, "/dev/mmcblk0p2"))
        self.assertNotEqual(self.resolve("PARTUUID=00000000-02", disks)[0], 0)
        self.assertNotEqual(self.resolve("PARTUUID=15BE6592-02", disks)[0], 0)
        self.assertNotEqual(self.resolve("LABEL=rootfs", disks)[0], 0)

    def trace_names(self, existing, slot="b"):
        with tempfile.TemporaryDirectory() as tmp:
            for name in existing:
                (Path(tmp) / name).write_text("x")
            result = subprocess.run(["sh", "-c", f'. "{self.INIT}" --lib; trace_name "$1" "$2"', "sh", tmp, slot],
                                    capture_output=True, text=True, check=True)
            return result.stdout.strip(), sorted(p.name for p in Path(tmp).iterdir())

    def test_trace_file_rotation(self):
        self.assertEqual(self.trace_names([]), ("b-1.log", []))
        name, left = self.trace_names(["b-1.log", "b-2.log", "a-7.log", "b-x.log"])
        self.assertEqual(name, "b-3.log")
        self.assertIn("a-7.log", left, "the other slot's traces are not touched")
        name, left = self.trace_names([f"b-{n}.log" for n in range(1, 9)])
        self.assertEqual(name, "b-9.log")
        self.assertEqual([n for n in left if n.startswith("b-")], ["b-5.log", "b-6.log", "b-7.log", "b-8.log"],
                         "with the new file, 5 per slot remain")

    def test_trace_is_bounded_and_only_on_boots_that_need_it(self):
        text = self.INIT.read_text()
        self.assertIn("TRACE_MAX=65536", text)
        self.assertIn("TRACE_ENABLED=auto", text)
        self.assertIn("ro,noload", text, "p3 is read without replaying its journal")
        self.assertIn('tail -n 150 >> "$TRACE"', text, "dmesg tail on failure")
        self.assertEqual(text.count("\nflush\n"), 2, "two copies per boot: after the root lookup, before hand-over")

    def test_init_shape(self):
        text = self.INIT.read_text()
        self.assertTrue(text.startswith("#!/bin/busybox sh\n"))
        self.assertIn("OVERLAY_SIZE=96M", text)
        self.assertIn('exec switch_root /newroot "$init"', text)
        # busybox --install -s puts od/tr (used by resolve_root) into /usr/bin (found booting it under QEMU).
        self.assertIn("/usr/bin", re.search(r"/bin/busybox mkdir -p ([^\n]*)", text).group(1))
        self.assertIn("export PATH=/bin:/sbin:/usr/bin:/usr/sbin", text)
        self.assertEqual(subprocess.run(["sh", "-n", str(self.INIT)]).returncode, 0)


class OpenRcScriptsTest(unittest.TestCase):
    INIT_D = ALPINE / "rootfs/etc/init.d"

    def depend(self, name):
        text = (self.INIT_D / name).read_text()
        body = re.search(r"depend\(\) \{(.*?)\n\}", text, re.S).group(1)
        result = {}
        for line in body.strip().splitlines():
            words = line.split()
            result.setdefault(words[0], []).extend(words[1:])
        return result

    def test_scripts_are_executable_openrc_and_parse(self):
        for script in self.INIT_D.iterdir():
            self.assertTrue(script.read_text().startswith("#!/sbin/openrc-run\n"), script.name)
            self.assertTrue(script.stat().st_mode & stat.S_IXUSR, script.name)
            self.assertEqual(subprocess.run(["sh", "-n", str(script)]).returncode, 0, script.name)
        for helper in ("nibe-boot-guard", "nibe-tailscale-up", "nibe-os-confirm", "nibe-trace", "nibe-reboot",
                       "nibe-detach", "nibe-wifi-fallback"):
            path = ALPINE / "rootfs/usr/lib/nibe-gateway" / helper
            self.assertTrue(path.stat().st_mode & stat.S_IXUSR)
            self.assertEqual(subprocess.run(["sh", "-n", str(path)]).returncode, 0)

    def test_ordering_matches_the_systemd_units(self):
        persist = self.depend("nibe-persist")
        self.assertIn("localmount", persist["need"])
        for later in ("networkmanager", "sshd", "tailscale", "nibe-provision", "nibe-gateway", "nibe-system"):
            self.assertIn(later, persist["before"])
        provision = self.depend("nibe-provision")
        self.assertIn("nibe-persist", provision["need"])
        for later in ("nibe-gateway", "sshd", "tailscale", "nibe-tailscale-up"):
            self.assertIn(later, provision["before"])
        gateway = self.depend("nibe-gateway")
        self.assertIn("localmount", gateway["need"])
        for earlier in ("networkmanager", "nibe-updater"):
            self.assertIn(earlier, gateway["after"])
        self.assertIn("nibe-gateway", self.depend("nibe-os-confirm")["after"])
        self.assertIn("nibe-persist", self.depend("nibe-system")["need"])
        self.assertIn("tailscale", self.depend("nibe-tailscale-up")["need"])

    def test_gateway_service_contract(self):
        text = (self.INIT_D / "nibe-gateway").read_text()
        # The arguments of the retired systemd unit (packaging/systemd/nibe-gateway.service).
        args = "--data-dir /var/lib/nibe-gateway/state --config /etc/nibe-gateway/hardware.json " \
               "--admin-dir /var/lib/nibe-gateway/current/admin"
        conf = (ALPINE / "rootfs/etc/conf.d/nibe-gateway").read_text()
        self.assertIn(f'nibe_gateway_args="{args}"', conf)
        self.assertIn("supervisor=supervise-daemon", text)
        self.assertIn('command_user="nibe-gateway:nibe-gateway"', text)
        self.assertIn("export NIBE_HEALTH_URL=http://127.0.0.1:8080/health", text)
        self.assertIn("unset NOTIFY_SOCKET", text)
        self.assertIn('rc_ulimit="-e 25"', text)
        retry = re.search(r'retry="TERM/(\d+)/KILL/(\d+)"', text)
        self.assertLessEqual(int(retry.group(1)) + int(retry.group(2)), 60, "updater stop bound is 60 s")
        self.assertIn("error_logger=", text)

    def test_ssh_and_tailscale_are_gated_by_the_provisioning_markers(self):
        self.assertIn("keys=/etc/ssh/authorized_keys/nibe-admin", (self.INIT_D / "sshd").read_text())
        self.assertIn("/var/lib/tailscale/enabled", (self.INIT_D / "tailscale").read_text())
        config = (ALPINE / "rootfs/usr/lib/nibe-gateway/sshd_config").read_text()
        for line in ("AuthorizedKeysFile /etc/ssh/authorized_keys/%u", "PasswordAuthentication no",
                     "PermitRootLogin no", "AllowUsers nibe-admin"):
            self.assertIn(line, config)

    def test_boot_guard_reboots_only_an_uncommitted_trial_of_this_slot(self):
        guard = ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-boot-guard"
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            state = work / "os-state.json"
            log = work / "calls"
            script = guard.read_text().replace("/proc/cmdline", str(work / "cmdline")) \
                .replace("data=/var/lib/nibe-gateway", f"data={work}") \
                .replace("state=$data/update-state/os-state.json", f"state={state}") \
                .replace("/usr/lib/nibe-gateway/nibe-reboot", f"echo reboot >> {log}; exit 0; :")
            (work / "guard").write_text(script)
            bin_dir = work / "bin"
            bin_dir.mkdir()
            for name, body in (("mountpoint", "exit 0"), ("logger", "exit 0"), ("sleep", "exit 0"), ("sync", "exit 0")):
                (bin_dir / name).write_text(f"#!/bin/sh\n{body}\n")
                (bin_dir / name).chmod(0o755)
            env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"}

            def run(cmdline, json):
                (work / "cmdline").write_text(cmdline)
                state.write_text(json)
                log.unlink(missing_ok=True)
                subprocess.run(["sh", str(work / "guard")], env=env, check=True)
                return log.exists()

            trying_a = '{"phase":"trying","from":"b","to":"a","version":"1.0.0","at":1,"detail":null}'
            self.assertTrue(run("root=PARTUUID=x-02 nibe.slot=a", trying_a))
            self.assertFalse(run("root=PARTUUID=x-04 nibe.slot=b", trying_a), "old slot: os-confirm marks failed")
            self.assertFalse(run("nibe.slot=a", trying_a.replace("trying", "committed")))
            def needed(cmdline, json):
                (work / "cmdline").write_text(cmdline)
                state.write_text(json)
                return subprocess.run(["sh", str(work / "guard"), "--needed"], env=env).returncode
            self.assertEqual(needed("nibe.slot=a", trying_a), 0)
            self.assertEqual(needed("nibe.slot=b", trying_a), 1, "normal boot: the init script does not start it")
            self.assertEqual(needed("nibe.slot=b nibe.smoke=1", "{}"), 0, "smoke boots are always guarded")
            # Smoke: default runlevel never reached -> reboot (sleep is stubbed, so the 300 s pass at once).
            self.assertTrue(run("nibe.slot=a nibe.smoke=1", "{}"))


class DetachTest(unittest.TestCase):
    def test_runs_detached_and_logs_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            bin_dir = work / "bin"
            bin_dir.mkdir()
            (bin_dir / "logger").write_text(f'#!/bin/sh\necho "$@" >> {work}/args; cat >> {work}/log\n')
            (bin_dir / "logger").chmod(0o755)
            env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"}
            start = __import__("time").monotonic()
            subprocess.run(["sh", str(ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-detach"), "nibe-test",
                            "sh", "-c", "sleep 1; echo out; echo err >&2"], env=env, check=True)
            self.assertLess(__import__("time").monotonic() - start, 0.9, "returns at once")
            for _ in range(50):
                if (work / "log").exists() and "err" in (work / "log").read_text():
                    break
                __import__("time").sleep(0.1)
            self.assertEqual(sorted((work / "log").read_text().split()), ["err", "out"])
            self.assertIn("-t nibe-test", (work / "args").read_text())


class WifiFallbackTest(unittest.TestCase):
    """nibe-wifi-fallback: activates the saved station profile as root only when nothing is connected and the
    gateway is not healthy (ADR 0092 amendment)."""

    def run_fallback(self, active="", health='{"ready":false}', saved="", up_fails=()):
        script = (ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-wifi-fallback").read_text()
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "reached").write_text("20.0")
            calls = work / "calls"
            script = script.replace("reached=/run/nibe-trace/default-reached", f"reached={work}/reached") \
                .replace("/usr/lib/nibe-gateway/nibe-trace", f"{work}/bin/nibe-trace")
            (work / "fallback").write_text(script)
            bin_dir = work / "bin"
            bin_dir.mkdir()
            (work / "active").write_text(active)
            (work / "saved").write_text(saved)
            fails = " ".join(f'"{n}"' for n in up_fails)
            (bin_dir / "nmcli").write_text(f"""#!/bin/sh
case "$*" in
*"--active"*) cat {work}/active ;;
*"TIMESTAMP,NAME"*) cat {work}/saved ;;
*"con up id"*) name=$(eval echo \\${{$#}}); echo "up $name" >> {calls}
    for f in {fails}; do [ "$name" = "$f" ] && exit 4; done; exit 0 ;;
esac
""")
            (bin_dir / "curl").write_text(f"#!/bin/sh\necho '{health}'\n")
            (bin_dir / "nibe-trace").write_text(f'#!/bin/sh\necho "$*" >> {calls}\n')
            (bin_dir / "sleep").write_text("#!/bin/sh\nexit 0\n")
            for tool in bin_dir.iterdir():
                tool.chmod(0o755)
            subprocess.run(["sh", str(work / "fallback")], capture_output=True, text=True, check=True,
                           env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"})
            return calls.read_text().split("\n") if calls.exists() else []

    SAVED = "1792900000:nibe-gateway-station-Old\n1792960000:nibe-gateway-station-Rikard:IoT\n0:tailscale0\n"

    def test_activates_the_most_recent_station_profile_when_nothing_works(self):
        calls = self.run_fallback(saved=self.SAVED)
        self.assertIn("up nibe-gateway-station-Rikard:IoT", calls)
        self.assertNotIn("up nibe-gateway-station-Old", calls)
        self.assertIn("event wifi-fallback-activated", calls)

    def test_falls_back_to_the_next_profile(self):
        calls = self.run_fallback(saved=self.SAVED, up_fails=("nibe-gateway-station-Rikard:IoT",))
        self.assertEqual([c for c in calls if c.startswith("up ")],
                         ["up nibe-gateway-station-Rikard:IoT", "up nibe-gateway-station-Old"])

    def test_leaves_a_working_system_alone(self):
        self.assertFalse(any(c.startswith("up ") for c in self.run_fallback(active="802-11-wireless\n", saved=self.SAVED)))
        calls = self.run_fallback(health='{"status":"ok","ready":true}', saved=self.SAVED)
        self.assertFalse(any(c.startswith("up ") for c in calls))
        self.assertIn("event wifi-fallback-not-needed", calls)
        self.assertIn("event wifi-fallback-no-profile", self.run_fallback(saved="0:tailscale0\n"))

    def test_service_is_a_detached_one_shot_after_networkmanager(self):
        text = (ALPINE / "rootfs/etc/init.d/nibe-wifi-fallback").read_text()
        self.assertIn("need networkmanager", text)
        self.assertIn("nibe-detach nibe-wifi-fallback", text)
        self.assertNotIn("pidfile", text)
        self.assertIn("nibe-wifi-fallback", image.RUNLEVELS["default"])


class TrialNetworkGateTest(unittest.TestCase):
    """nibe-os-confirm wrapper: a trial of this slot with station profiles is confirmed only once online."""

    def run_gate(self, cmdline, phase_to, nm_state, profiles=True):
        script = (ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-os-confirm").read_text()
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "cmdline").write_text(cmdline)
            (work / "state.json").write_text('{"phase":"trying","from":"x","to":"%s","at":1}' % phase_to)
            (work / "conns").mkdir()
            if profiles:
                (work / "conns/station.nmconnection").write_text("x")
            calls = work / "calls"
            script = script.replace("/proc/cmdline", str(work / "cmdline")) \
                .replace("state=/var/lib/nibe-gateway/update-state/os-state.json", f"state={work}/state.json") \
                .replace("connections=/etc/NetworkManager/system-connections", f"connections={work}/conns") \
                .replace('/usr/lib/nibe-gateway/nibe-trace snapshot "network gate failed"', ":") \
                .replace("exec /usr/lib/nibe-gateway/nibe-reboot", f"echo reboot >> {calls}; exit 1; :") \
                .replace("exec /usr/lib/nibe-gateway/nibe-updater", f"echo confirm >> {calls}; : ")
            (work / "gate").write_text(script)
            bin_dir = work / "bin"
            bin_dir.mkdir()
            (bin_dir / "nmcli").write_text(f"#!/bin/sh\necho '{nm_state}'\n")
            (bin_dir / "sleep").write_text("#!/bin/sh\nexit 0\n")
            for tool in ("nmcli", "sleep"):
                (bin_dir / tool).chmod(0o755)
            subprocess.run(["sh", str(work / "gate")], capture_output=True,
                           env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "NIBE_TRIAL_NETWORK_WAIT": "30"})
            return calls.read_text().split() if calls.exists() else []

    def test_gate(self):
        self.assertEqual(self.run_gate("nibe.slot=a", "a", "connected"), ["confirm"])
        self.assertEqual(self.run_gate("nibe.slot=a", "a", "disconnected"), ["reboot"])
        self.assertEqual(self.run_gate("nibe.slot=a", "a", "connected (local only)"), ["reboot"])
        self.assertEqual(self.run_gate("nibe.slot=a", "a", "disconnected", profiles=False), ["confirm"])
        self.assertEqual(self.run_gate("nibe.slot=b", "a", "disconnected"), ["confirm"], "old slot records the failure")
        self.assertEqual(self.run_gate("nibe.slot=a nibe.smoke=1", "a", "connected"), [], "smoke boots never confirm")


class TraceTest(unittest.TestCase):
    """/usr/lib/nibe-gateway/nibe-trace: mode detection, boot directories and rotation on p3."""

    def prepare(self, work, cmdline, state="{}"):
        script = (ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-trace").read_text()
        (work / "cmdline").write_text(cmdline)
        (work / "state.json").write_text(state)
        script = script.replace("/proc/cmdline", str(work / "cmdline")) \
            .replace("base=/var/lib/nibe-gateway/system/boot-logs", f"base={work}/boot-logs") \
            .replace("state=/var/lib/nibe-gateway/update-state/os-state.json", f"state={work}/state.json") \
            .replace("run=/run/nibe-trace", f"run={work}/run")
        (work / "trace").write_text(script)
        bin_dir = work / "bin"
        bin_dir.mkdir(exist_ok=True)
        for tool in ("mountpoint", "nmcli", "rc-status", "curl", "dmesg", "sync"):
            (bin_dir / tool).write_text("#!/bin/sh\nexit 0\n")
            (bin_dir / tool).chmod(0o755)
        return {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}"}

    def run_trace(self, work, env, *args):
        return subprocess.run(["sh", str(work / "trace"), *args], env=env, capture_output=True, text=True, check=True)

    def test_modes(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            env = self.prepare(work, "nibe.slot=a nibe.smoke=1")
            self.assertEqual(self.run_trace(work, env, "mode").stdout.strip(), "smoke")
            env = self.prepare(work, "nibe.slot=a", '{"phase":"trying","from":"b","to":"a"}')
            self.assertEqual(self.run_trace(work, env, "mode").stdout.strip(), "trial")
            env = self.prepare(work, "nibe.slot=b", '{"phase":"trying","from":"b","to":"a"}')
            self.assertEqual(self.run_trace(work, env, "mode").stdout.strip(), "normal")

    def test_boot_directories_rotate_and_snapshot_writes_a_timeline(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            env = self.prepare(work, "nibe.slot=a nibe.smoke=1")
            for _ in range(7):
                self.run_trace(work, env, "start")
            dirs = sorted(p.name for p in (work / "boot-logs").iterdir())
            self.assertEqual(dirs, [f"{n:04d}-a-smoke" for n in range(3, 8)], "the last 5 boots are kept")
            self.run_trace(work, env, "snapshot", "test")
            latest = work / "boot-logs/0007-a-smoke"
            self.assertIn("mode: smoke", (latest / "meta.txt").read_text())
            self.assertRegex((latest / "timeline.txt").read_text(), r"up=\S+ test default=no nm=.* health=\d+ ready=no")
            for name in ("dmesg.txt", "rc-status.txt", "ip-addr.txt", "nmcli.txt", "uart.txt"):
                self.assertTrue((latest / name).exists(), name)
            self.assertIn("disable-bt applied:", (latest / "uart.txt").read_text())
            self.assertIn("NAME,TYPE,AUTOCONNECT,ACTIVE", (ALPINE / "rootfs/usr/lib/nibe-gateway/nibe-trace").read_text(),
                          "all profiles are listed, not only active ones")


class BundleScriptTest(unittest.TestCase):
    SCRIPT = REPO / "scripts/build-alpine-os-bundle.sh"

    def test_usage_and_input_validation(self):
        self.assertEqual(subprocess.run(["bash", str(self.SCRIPT)], capture_output=True).returncode, 2)
        with tempfile.TemporaryDirectory() as tmp:
            key = Path(tmp) / "release.key"
            key.write_bytes(bytes(32))
            result = subprocess.run(["bash", str(self.SCRIPT), str(key), "not-semver", tmp], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            result = subprocess.run(["bash", str(self.SCRIPT), str(key), "1.0.0", tmp], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("public key", result.stderr)
            (Path(tmp) / "release.pub").write_bytes(bytes(32))
            result = subprocess.run(["bash", str(self.SCRIPT), str(key), "1.0.0", tmp], capture_output=True, text=True,
                                    env={**os.environ, "NIBE_WIFI_COUNTRY": "sweden"})
            self.assertEqual(result.returncode, 2)
            # A smoke bundle must say so in its version, and only a smoke bundle may.
            for smoke, version in (("1", "1.0.0"), ("0", "1.0.0-smoke.1")):
                result = subprocess.run(["bash", str(self.SCRIPT), str(key), version, tmp], capture_output=True,
                                        text=True, env={**os.environ, "NIBE_SMOKE": smoke})
                self.assertEqual(result.returncode, 2, version)
                self.assertIn("smoke", result.stderr)

    def test_kernel_is_pinned(self):
        lock = (ALPINE / "kernel.lock").read_text()
        self.assertRegex(lock, r"RPI_KERNEL_SHA256='[0-9a-f]{64}'")
        self.assertIn("RPI_KERNEL_VERSION='6.12.109+rpt-rpi-v6'", lock)
        self.assertNotIn("linux-rpi", image.parse_world((ALPINE / "world").read_text()))

    def report(self, trial_dir):
        """The remote part of alpine-trial-report.sh, run locally against a fixture tree."""
        text = (REPO / "scripts/alpine-trial-report.sh").read_text()
        remote = text.split("<<'REMOTE'\n", 1)[1].split("\nREMOTE\n", 1)[0]
        remote = remote.replace("/boot/firmware/nibe-trial", str(trial_dir / "p1")) \
            .replace("/var/lib/nibe-gateway/system/boot-logs", str(trial_dir / "p3"))
        return subprocess.run(["sh", "-c", remote], capture_output=True, text=True).stdout

    def test_report_summarises_old_and_new_traces(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "p1").mkdir()
            (work / "p1/b-1.log").write_text("3.53 kernel started\n4.57 handing over to /sbin/init\n")
            old = work / "p3/0001-b-smoke"
            old.mkdir(parents=True)
            (old / "meta.txt").write_text("mode: smoke\n")
            (old / "timeline.txt").write_text("19:49:27Z up=150.07 smoke-final default=yes nm=\"disconnected\"\n")
            (old / "dmesg.txt").write_text("[ 3.27] 20201000.serial: ttyAMA1 at MMIO 0x20201000 (irq = 81) is a PL011 rev2\n")
            new = work / "p3/0002-b-smoke"
            new.mkdir()
            (new / "meta.txt").write_text("mode: smoke\n")
            (new / "timeline.txt").write_text("20:10:00Z up=96.50 smoke-final default=yes nm=\"connected\"\n")
            (new / "events.txt").write_text("default-runlevel up=31.20\nnm-connected up=58.10\nsmoke-reboot up=96.60\n")
            (new / "uart.txt").write_text("disable-bt applied: yes\naliases/serial0: /soc/serial@7e201000\n"
                                          "crw-rw----    1 root     dialout   204,  64 Oct 25 20:09 /dev/ttyAMA0\n")
            (new / "nmcli.txt").write_text("STATE\nconnected\n\nnibe-gateway-station-Rikard-IoT:802-11-wireless:yes:yes\n")
            out = self.report(work)
            self.assertIn("handing over", out)
            self.assertIn("disable-bt applied: no (from dmesg: PL011 is ttyAMA1", out)
            self.assertIn("Pi unreachable on the network for the whole Alpine boot: ~190 s", out)
            self.assertIn("disable-bt applied: yes (serial0=/soc/serial@7e201000, /dev/ttyAMA0", out)
            self.assertIn("NetworkManager first connected 58.10", out)
            self.assertIn("default runlevel 31.20", out)
            self.assertIn("nibe-gateway-station-Rikard-IoT:802-11-wireless:yes:yes", out)

    def test_report_script_usage(self):
        script = REPO / "scripts/alpine-trial-report.sh"
        result = subprocess.run(["bash", str(script)], capture_output=True, text=True,
                                env={k: v for k, v in os.environ.items() if k != "NIBE_SSH_HOST"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("NIBE_SSH_HOST", result.stderr)


if __name__ == "__main__":
    unittest.main()
