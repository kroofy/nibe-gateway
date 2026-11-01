#!/usr/bin/env python3
"""Reject ARMv7, soft float, and non-Pi-Zero artifacts.

gnu (default): dynamically linked against glibc <= Bookworm 2.36 via /lib/ld-linux-armhf.so.3.
musl: statically linked, no program interpreter and no shared-library dependencies.
Both: ELF32 ARM, EABI5 hard float, Tag_CPU_arch v6 (the build attribute is the highest architecture
of every linked object, so any ARMv7-only object, Rust or C, raises it).
"""
import os
import re
import subprocess
import sys


def check(path, libc="gnu"):
    def read(*flags):
        return subprocess.check_output(["readelf", *flags, path], text=True, env={**os.environ, "LC_ALL": "C"})

    header, attributes, program, versions, dynamic = (read(flag) for flag in ("-h", "-A", "-l", "-V", "-d"))
    checks = {
        "ELF32 ARM": "ELF32" in header and re.search(r"Machine:\s+ARM\s*$", header, re.M),
        "ARMv6 attributes": re.search(r"Tag_CPU_arch:\s+v6\s*$", attributes, re.M),
        "EABI5 hard float": "Version5 EABI, hard-float ABI" in header,
    }
    required = {tuple(map(int, value.split("."))) for value in re.findall(r"GLIBC_([0-9.]+)", versions)}
    if libc == "gnu":
        checks["armhf interpreter"] = "/lib/ld-linux-armhf.so.3" in program
        checks["glibc <= Bookworm 2.36"] = bool(required) and max(required) <= (2, 36)
    else:
        checks["static (no interpreter)"] = "INTERP" not in program and "program interpreter" not in program
        checks["no shared libraries"] = "(NEEDED)" not in dynamic
        checks["no glibc symbol versions"] = not required
    for name, passed in checks.items():
        print(f"{'PASS' if passed else 'FAIL'} {name}")
    if required:
        print("Highest required glibc: " + ".".join(map(str, max(required))))
    return all(checks.values())


if __name__ == "__main__":
    args = sys.argv[1:]
    libc = "gnu"
    if args[:1] == ["--libc"] and len(args) >= 2:
        libc, args = args[1], args[2:]
    if len(args) != 1 or libc not in ("gnu", "musl"):
        sys.exit("usage: check-pi-elf.py [--libc gnu|musl] BINARY")
    sys.exit(0 if check(args[0], libc) else 1)
