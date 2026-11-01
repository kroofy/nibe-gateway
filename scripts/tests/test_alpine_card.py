"""Fresh-card image layout (packaging/alpine/card_image.py): the short image (p1, p2 = the squashfs) and the
card after its first boot (partition table equal to the owner's card), the slot A cmdline as os-install writes it,
header and file checks. The full build and QEMU boot are
scripts/build-alpine-image.sh and scripts/qemu-boot-alpine-image.sh (Docker)."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("card_image", REPO / "packaging/alpine/card_image.py")
card = importlib.util.module_from_spec(spec)
spec.loader.exec_module(card)

# The owner's card, read 2026-10-25 from /sys/block/mmcblk0/mmcblk0pN/{start,size} and the MBR.
CARD = [(1, 16384, 1048576, 0x0C), (2, 1064960, 4046848, 0x83), (3, 5111808, 1048576, 0x83), (4, 6160384, 4046848, 0x83)]


SQUASHFS_BYTES = 45_000_000  # 43 MiB, like 0.1.31: p2 = 43 MiB = 88064 sectors


def squashfs_header(used=SQUASHFS_BYTES):
    sb = bytearray(96)
    sb[:4] = b"hsqs"
    struct.pack_into("<Q", sb, 40, used)
    return bytes(sb)


def fake_image(path: Path, disk_id=0xECB05100, hidden=16384, p2=None, size=None, squashfs=None):
    """The image as build-card.sh writes it: p1, p2 = the squashfs rounded up to 1 MiB, nothing after."""
    p2 = p2 or card.p2_sectors(SQUASHFS_BYTES)
    with open(path, "wb") as f:
        f.truncate(size or card.image_bytes(p2))
    card.write_mbr(path, disk_id, card.image_layout(p2))
    write_p1_p2(path, hidden, squashfs or squashfs_header())


def fake_card(path: Path, disk_id=0xECB05100, p4=b"", label=b"nibe-data"):
    """A card after its first boot: the full layout, p3 ext4 labelled nibe-data."""
    with open(path, "wb") as f:
        f.truncate(card.LAYOUT_BYTES)
    card.write_mbr(path, disk_id)
    write_p1_p2(path, 16384, squashfs_header())
    with open(path, "r+b") as f:
        sb = bytearray(2048)
        sb[1024 + 56:1024 + 58] = b"\x53\xef"
        sb[1024 + 120:1024 + 120 + len(label)] = label
        f.seek(5111808 * 512)
        f.write(sb)
        f.seek(6160384 * 512)
        f.write(p4)


def write_p1_p2(path: Path, hidden: int, squashfs: bytes):
    with open(path, "r+b") as f:
        boot = bytearray(512)
        struct.pack_into("<I", boot, 28, hidden)
        boot[0x47:0x52] = b"bootfs     "
        boot[0x52:0x5A] = b"FAT32   "
        f.seek(16384 * 512)
        f.write(boot)
        f.seek(1064960 * 512)
        f.write(squashfs)


class LayoutTest(unittest.TestCase):
    def test_layout_is_the_owners_card(self):
        self.assertEqual(list(card.LAYOUT), CARD)
        self.assertEqual(card.LAYOUT_BYTES, (6160384 + 4046848) * 512)

    def test_image_is_cut_after_the_squashfs(self):
        self.assertEqual(card.p2_sectors(1), 2048)
        self.assertEqual(card.p2_sectors(1 << 20), 2048)
        self.assertEqual(card.p2_sectors((1 << 20) + 1), 4096)
        self.assertEqual(card.p2_sectors(SQUASHFS_BYTES), 88064)
        self.assertEqual(card.image_layout(88064), [CARD[0], (2, 1064960, 88064, 0x83), (3, 0, 0, 0), (4, 0, 0, 0)])
        self.assertEqual(card.image_bytes(88064), (1064960 + 88064) * 512)  # 563 MiB instead of 4.9 GiB
        with self.assertRaises(ValueError):
            card.p2_sectors(4046848 * 512 + 1)

    def test_short_mbr_round_trip_keeps_entries_3_and_4_empty(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            path.write_bytes(bytes(512))
            card.write_mbr(path, 0x2E3CF059, card.image_layout(88064))
            head = path.read_bytes()
            self.assertEqual(head[446 + 32:510], bytes(32))
            self.assertEqual(card.read_mbr(head)[1], card.image_layout(88064))

    def test_mbr_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            path.write_bytes(bytes(512))
            card.write_mbr(path, 0x2E3CF059)
            disk_id, parts = card.read_mbr(path.read_bytes())
            self.assertEqual((disk_id, parts), (0x2E3CF059, CARD))
            with self.assertRaises(ValueError):
                card.write_mbr(path, 0)

    def test_slot_cmdline_matches_what_os_install_writes(self):
        template = "console=tty1 root=PARTUUID=00000000-02 rootwait cfg80211.ieee80211_regdom=SE nibe.overlay=1 " \
                   "rootfstype=squashfs panic=10 nibe.slot=a\n"
        # Exactly the cmdline on the owner's slot-a/ (written by nibe-updater os-install), disk id swapped.
        self.assertEqual(card.slot_cmdline(template, 0x2E3CF059),
                         "console=tty1 rootwait cfg80211.ieee80211_regdom=SE nibe.overlay=1 rootfstype=squashfs "
                         "panic=10 root=PARTUUID=2e3cf059-02 nibe.slot=a\n")


def reader(path: Path):
    f = open(path, "rb")

    def read(offset, length):
        f.seek(offset)
        return f.read(length)
    return f, read


class HeaderTest(unittest.TestCase):
    def check(self, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            fake_image(path, **kwargs)
            f, read = reader(path)
            with f:
                return card.check_headers(read, path.stat().st_size)

    def check_card(self, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            fake_card(path, **kwargs)
            f, read = reader(path)
            with f:
                return card.check_card_headers(read)

    def test_good_image(self):
        self.assertEqual(self.check(), (0xECB05100, []))

    def test_image_faults(self):
        self.assertTrue(any("hidden sectors" in e for e in self.check(hidden=0)[1]))
        self.assertTrue(any("image layout" in e for e in self.check(p2=4046848)[1]), "p2 must be cut to the squashfs")
        self.assertTrue(any("ends with p2" in e for e in self.check(size=card.LAYOUT_BYTES)[1]))
        self.assertTrue(any("squashfs" in e for e in self.check(squashfs=b"\0" * 96)[1]))

    def test_full_layout_is_not_an_image(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            fake_card(path)
            f, read = reader(path)
            with f:
                errors = card.check_headers(read, path.stat().st_size)[1]
        self.assertTrue(any("image layout" in e for e in errors))

    def test_card_after_first_boot(self):
        self.assertEqual(self.check_card(), (0xECB05100, []))
        self.assertTrue(any("p4" in e for e in self.check_card(p4=b"hsqs")[1]), "slot B must be wiped")
        self.assertTrue(any("p3" in e for e in self.check_card(label=b"rootfs")[1]))


class FileCheckTest(unittest.TestCase):
    CONFIG = (REPO / "packaging/alpine/boot/config.txt").read_text()
    NAMES = {*card.FIRMWARE, "config.txt", "nibe-provision.json.template", "README.txt", "slot-b", "nibe-first-boot",
             *(f"slot-a/{n}" for n in card.SLOT_FILES)}
    CMDLINE = "console=tty1 rootwait rootfstype=squashfs panic=10 root=PARTUUID=ecb05100-02 nibe.slot=a\n"

    def test_shipped_config_selects_slot_a(self):
        self.assertEqual(card.check_boot_files(self.NAMES, self.CONFIG, self.CMDLINE, 0xECB05100), [])

    def test_first_boot_marker(self):
        missing = card.check_boot_files(self.NAMES - {"nibe-first-boot"}, self.CONFIG, self.CMDLINE, 0xECB05100)
        self.assertTrue(any("nibe-first-boot" in e for e in missing), "the image must carry the marker")
        after = card.check_boot_files(self.NAMES - {"nibe-first-boot"}, self.CONFIG, self.CMDLINE, 0xECB05100,
                                      fresh=False)
        self.assertEqual(after, [])
        left = card.check_boot_files(self.NAMES, self.CONFIG, self.CMDLINE, 0xECB05100, fresh=False)
        self.assertTrue(any("after the first boot" in e for e in left))

    def test_faults(self):
        errors = card.check_boot_files(self.NAMES - {"start.elf"} | {"nibe-provision.json"},
                                       self.CONFIG.replace("os_prefix=slot-a/", "os_prefix=slot-b/"),
                                       self.CMDLINE.replace("ecb05100", "deadbeef") + " nibe.smoke=1", 0xECB05100)
        for needle in ("start.elf", "only the template", "slot-a/ in the managed", "root=PARTUUID=ecb05100-02",
                       "non-smoke"):
            self.assertTrue(any(needle in e for e in errors), needle)

    def test_data_listing(self):
        ok = {p: "Inode: 12  Type: directory\nUser:   995   Group:   992" for p in (
            "/releases/slot-a/nibe-gateway", "/releases/slot-a/manifest.json", "/releases/slot-a/admin/index.html",
            "/update-state/state.json", "/state", "/system/ssh", "/system/connections", "/system/NetworkManager")}
        ok["/current"] = "releases/slot-a"
        self.assertEqual(card.check_data_listing(ok), [])
        bad = dict(ok, **{"/state": "User:     0   Group:     0", "/current": "releases/slot-b"})
        self.assertEqual(len(card.check_data_listing(bad)), 2)


if __name__ == "__main__":
    unittest.main()
