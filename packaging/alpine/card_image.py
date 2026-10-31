#!/usr/bin/env python3
"""Fresh-card image layout and mount-free inspection (ADR 0106 layout, ADR 0117 Alpine).

The layout is the owner's card as it runs today (read 2026-10-25 from /sys/block/mmcblk0, 512-byte sectors):
  p1 FAT32 (type 0x0c) `bootfs`    start 16384   size 1048576  (8 MiB, 512 MiB)  firmware, config.txt, slot-a/, slot-b/
  p2 Linux (0x83)      slot A      start 1064960 size 4046848  (1976 MiB)        squashfs root
  p3 Linux (0x83)      `nibe-data` start 5111808 size 1048576  (512 MiB)         ext4, persistent state
  p4 Linux (0x83)      slot B      start 6160384 size 4046848  (1976 MiB)        empty until the first OS update
MBR disk id: random per image; PARTUUIDs are <disk id>-0N, slot A's cmdline says root=PARTUUID=<disk id>-02.

Subcommands (used by packaging/alpine/build-card.sh inside the builder container):
  mbr IMAGE DISKID            write the partition table (the image file must already have its final size)
  cmdline TEMPLATE DISKID     slot A cmdline from a bundle template, exactly as nibe-updater os-install writes it
  inspect IMAGE               verify partition table, filesystem headers and (with mtools/debugfs) the files
"""
from __future__ import annotations

import os
import re
import struct
import subprocess
import sys
from pathlib import Path

SECTOR = 512
# (number, start sector, size in sectors, MBR type)
LAYOUT = (
    (1, 16384, 1048576, 0x0C),
    (2, 1064960, 4046848, 0x83),
    (3, 5111808, 1048576, 0x83),
    (4, 6160384, 4046848, 0x83),
)
IMAGE_BYTES = (LAYOUT[-1][1] + LAYOUT[-1][2]) * SECTOR
BOOT_LABEL, DATA_LABEL = "bootfs", "nibe-data"
FIRMWARE = ("bootcode.bin", "start.elf", "fixup.dat")  # what a Pi Zero W boots with; the rest is shipped too
SLOT_FILES = ("kernel.img", "initramfs", "bcm2708-rpi-zero-w.dtb", "cmdline.txt", "overlays/disable-bt.dtbo",
              "overlays/vc4-kms-v3d.dtbo", "overlays/overlay_map.dtb")
MARKER = "# nibe-slot (managed by nibe-updater; do not edit)"


def write_mbr(path: Path, disk_id: int) -> None:
    if not 0 < disk_id < 2**32:
        raise ValueError("disk id must be a non-zero 32-bit value")
    entries = b""
    for _number, start, size, kind in LAYOUT:
        # Status 0 (not bootable; the Pi firmware ignores it), CHS fields unused (0xFE 0xFF 0xFF = "use LBA").
        entries += struct.pack("<B3sB3sII", 0, b"\xfe\xff\xff", kind, b"\xfe\xff\xff", start, size)
    with open(path, "r+b") as f:
        f.seek(440)
        f.write(struct.pack("<I", disk_id) + b"\0\0" + entries + b"\x55\xaa")


def read_mbr(head: bytes) -> tuple[int, list[tuple[int, int, int, int]]]:
    if head[510:512] != b"\x55\xaa":
        raise ValueError("no MBR signature")
    disk_id, = struct.unpack_from("<I", head, 440)
    parts = []
    for i in range(4):
        _status, _chs, kind, _chs2, start, size = struct.unpack_from("<B3sB3sII", head, 446 + 16 * i)
        parts.append((i + 1, start, size, kind))
    return disk_id, parts


def partuuid(disk_id: int, number: int) -> str:
    return f"{disk_id:08x}-{number:02d}"


def slot_cmdline(template: str, disk_id: int, slot: str = "a") -> str:
    """Same transformation as crates/updater/src/os.rs slot_cmdline: drop root= and nibe.slot=, append both."""
    tokens = [t for t in template.split() if not t.startswith(("root=", "nibe.slot="))]
    tokens += [f"root=PARTUUID={partuuid(disk_id, 2 if slot == 'a' else 4)}", f"nibe.slot={slot}"]
    return " ".join(tokens) + "\n"


def fat_label(boot_sector: bytes) -> tuple[str, str]:
    """(volume label, filesystem type) of a FAT32 boot sector."""
    return boot_sector[0x47:0x52].decode("ascii", "replace").strip(), boot_sector[0x52:0x5A].decode("ascii", "replace").strip()


def ext4_label(superblock_area: bytes) -> str | None:
    """Label of an ext2/3/4 filesystem given its first 2 KiB, or None if there is none."""
    if superblock_area[1024 + 56:1024 + 58] != b"\x53\xef":
        return None
    return superblock_area[1024 + 120:1024 + 136].rstrip(b"\0").decode("utf-8", "replace")


def check_headers(read) -> tuple[int, list[str]]:
    """`read(offset, length)` reads the image. Returns (disk id, errors) for partition table and filesystem headers."""
    errors = []
    disk_id, parts = read_mbr(read(0, 512))
    if disk_id == 0:
        errors.append("MBR disk id is zero (must be random per image)")
    if parts != list(LAYOUT):
        errors.append(f"partition table {parts} differs from the card layout {list(LAYOUT)}")
    start = {n: s * SECTOR for n, s, _size, _kind in LAYOUT}
    boot_sector = read(start[1], 512)
    label, kind = fat_label(boot_sector)
    hidden, = struct.unpack_from("<I", boot_sector, 28)
    if hidden != LAYOUT[0][1]:
        errors.append(f"p1 FAT hidden sectors {hidden} != partition start {LAYOUT[0][1]} (the card's mkdosfs value)")
    if (label, kind) != (BOOT_LABEL.upper(), "FAT32") and (label, kind) != (BOOT_LABEL, "FAT32"):
        errors.append(f"p1 is not FAT32 labelled {BOOT_LABEL} (found {label!r}, {kind!r})")
    if read(start[2], 4) != b"hsqs":
        errors.append("p2 does not start with a squashfs")
    if ext4_label(read(start[3], 2048)) != DATA_LABEL:
        errors.append(f"p3 is not ext4 labelled {DATA_LABEL}")
    if read(start[4], 1 << 20).strip(b"\0"):
        errors.append("p4 (slot B) must be empty")
    return disk_id, errors


def check_boot_files(names: set[str], config: str, cmdline: str, disk_id: int) -> list[str]:
    errors = [f"p1 lacks {n}" for n in FIRMWARE + ("config.txt", "nibe-provision.json.template", "README.txt")
              if n not in names]
    errors += [f"p1 lacks slot-a/{n}" for n in SLOT_FILES if f"slot-a/{n}" not in names]
    if "slot-b" not in names:
        errors.append("p1 lacks the empty slot-b/ directory")
    if "nibe-provision.json" in names:
        errors.append("a fresh card must not carry nibe-provision.json (only the template)")
    lines = config.splitlines()
    if MARKER not in lines or lines[lines.index(MARKER) + 1:lines.index(MARKER) + 2] != ["os_prefix=slot-a/"]:
        errors.append("config.txt must select slot-a/ in the managed nibe-slot section")
    for need in ("auto_initramfs=1", "enable_uart=1", "dtoverlay=disable-bt"):
        if need not in lines:
            errors.append(f"config.txt lacks {need}")
    tokens = cmdline.split()
    if f"root=PARTUUID={partuuid(disk_id, 2)}" not in tokens or "nibe.slot=a" not in tokens:
        errors.append(f"slot-a/cmdline.txt must name root=PARTUUID={partuuid(disk_id, 2)} and nibe.slot=a")
    if "rootfstype=squashfs" not in tokens or "nibe.smoke=1" in tokens:
        errors.append("slot-a/cmdline.txt must be a real (non-smoke) squashfs slot")
    return errors


def check_data_listing(listing: dict[str, str]) -> list[str]:
    """`listing`: path -> `debugfs stat`-style text (mode/owner) or symlink target, for the paths below."""
    errors = []
    if listing.get("/current") != "releases/slot-a":
        errors.append("p3 current must point at releases/slot-a")
    for path in ("/releases/slot-a/nibe-gateway", "/releases/slot-a/manifest.json", "/releases/slot-a/admin/index.html",
                 "/update-state/state.json", "/state", "/system/ssh", "/system/connections", "/system/NetworkManager"):
        if path not in listing:
            errors.append(f"p3 lacks {path}")
    if "/state" in listing and not re.search(r"User:\s+995\s+Group:\s+992", listing["/state"]):
        errors.append("p3 state/ must be owned by 995:992 (nibe-gateway)")
    return errors


def _mtools(image: Path, offset: int, *args: str) -> str:
    env = {**os.environ, "MTOOLS_SKIP_CHECK": "1"}
    return subprocess.run([args[0], "-i", f"{image}@@{offset}", *args[1:]], env=env, check=True,
                          capture_output=True, text=True).stdout


def inspect(image: Path) -> list[str]:
    with open(image, "rb") as f:
        def read(offset, length):
            f.seek(offset)
            return f.read(length)
        disk_id, errors = check_headers(read)
        if errors:
            return errors
    p1 = LAYOUT[0][1] * SECTOR
    names = set()
    for line in _mtools(image, p1, "mdir", "-/", "-b", "::/").splitlines():
        name = line.strip().removeprefix("::/").rstrip("/")
        if name:
            names.add(name)
    if any(n.startswith("slot-b/") for n in names) or "slot-b" not in names:
        names.add("slot-b")  # mdir -b lists an empty directory itself
    config = _mtools(image, p1, "mtype", "::/config.txt")
    cmdline = _mtools(image, p1, "mtype", "::/slot-a/cmdline.txt")
    errors = check_boot_files(names, config, cmdline, disk_id)
    # p3 through debugfs on a byte range copy-free: debugfs reads the image at an offset via "file?offset=".
    p3 = LAYOUT[2][1] * SECTOR
    listing = {}
    for path in ("/current", "/releases/slot-a/nibe-gateway", "/releases/slot-a/manifest.json",
                 "/releases/slot-a/admin/index.html", "/update-state/state.json", "/state", "/system/ssh",
                 "/system/connections", "/system/NetworkManager"):
        out = subprocess.run(["debugfs", "-R", f"stat {path}", f"{image}?offset={p3}"], capture_output=True,
                             text=True).stdout
        if "Inode:" not in out:
            continue
        link = re.search(r'Fast link dest: "([^"]+)"', out)
        listing[path] = link.group(1) if link else out
    errors += check_data_listing(listing)
    print(f"card image: disk id {disk_id:08x}, {len(names)} entries on p1, slot A = PARTUUID {partuuid(disk_id, 2)}")
    return errors


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[0] == "mbr":
        write_mbr(Path(argv[1]), int(argv[2], 16))
    elif len(argv) == 3 and argv[0] == "cmdline":
        sys.stdout.write(slot_cmdline(Path(argv[1]).read_text(), int(argv[2], 16)))
    elif len(argv) == 2 and argv[0] == "inspect":
        errors = inspect(Path(argv[1]))
        for error in errors:
            print(f"card image: {error}", file=sys.stderr)
        if errors:
            return 1
        print("card image: layout, labels, firmware, config.txt, slot A and p3 seed verified")
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
