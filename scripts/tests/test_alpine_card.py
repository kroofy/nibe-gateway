"""Fresh-card image layout (packaging/alpine/card_image.py): partition table equal to the owner's card, the slot A
cmdline as os-install writes it, header and file checks. The full build and QEMU boot are
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


def fake_image(path: Path, disk_id=0xECB05100, hidden=16384, p4=b""):
    with open(path, "wb") as f:
        f.truncate(card.IMAGE_BYTES)
    card.write_mbr(path, disk_id)
    with open(path, "r+b") as f:
        boot = bytearray(512)
        struct.pack_into("<I", boot, 28, hidden)
        boot[0x47:0x52] = b"bootfs     "
        boot[0x52:0x5A] = b"FAT32   "
        f.seek(16384 * 512)
        f.write(boot)
        f.seek(1064960 * 512)
        f.write(b"hsqs")
        sb = bytearray(2048)
        sb[1024 + 56:1024 + 58] = b"\x53\xef"
        sb[1024 + 120:1024 + 129] = b"nibe-data"
        f.seek(5111808 * 512)
        f.write(sb)
        f.seek(6160384 * 512)
        f.write(p4)


class LayoutTest(unittest.TestCase):
    def test_layout_is_the_owners_card(self):
        self.assertEqual(list(card.LAYOUT), CARD)
        self.assertEqual(card.IMAGE_BYTES, (6160384 + 4046848) * 512)

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


class HeaderTest(unittest.TestCase):
    def check(self, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "card.img"
            fake_image(path, **kwargs)
            with open(path, "rb") as f:
                def read(offset, length):
                    f.seek(offset)
                    return f.read(length)
                return card.check_headers(read)

    def test_good_image(self):
        self.assertEqual(self.check(), (0xECB05100, []))

    def test_faults(self):
        self.assertTrue(any("hidden sectors" in e for e in self.check(hidden=0)[1]))
        self.assertTrue(any("p4" in e for e in self.check(p4=b"hsqs")[1]), "slot B must be empty")


class FileCheckTest(unittest.TestCase):
    CONFIG = (REPO / "packaging/alpine/boot/config.txt").read_text()
    NAMES = {*card.FIRMWARE, "config.txt", "nibe-provision.json.template", "README.txt", "slot-b",
             *(f"slot-a/{n}" for n in card.SLOT_FILES)}
    CMDLINE = "console=tty1 rootwait rootfstype=squashfs panic=10 root=PARTUUID=ecb05100-02 nibe.slot=a\n"

    def test_shipped_config_selects_slot_a(self):
        self.assertEqual(card.check_boot_files(self.NAMES, self.CONFIG, self.CMDLINE, 0xECB05100), [])

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
