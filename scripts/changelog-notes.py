#!/usr/bin/env python3
"""Print the GitHub release notes for VERSION: its CHANGELOG.md section plus install hints. Fails without one.
GitHub shows every newline in release notes, so wrapped continuation lines are joined."""
import pathlib
import re
import sys

version = sys.argv[1]
text = (pathlib.Path(__file__).resolve().parent.parent / "CHANGELOG.md").read_text()
match = re.search(rf"^## {re.escape(version)}(?: [^\n]*)?\n(.*?)(?=^## |\Z)", text, re.M | re.S)
body = match.group(1).strip() if match else ""
if not body:
    sys.exit(f"CHANGELOG.md has no section for {version}; add one before publishing")
lines: list[str] = []
for line in body.splitlines():
    continuation = line.startswith("  ") and not line.lstrip().startswith("- ")
    if continuation and lines and lines[-1]:
        lines[-1] += " " + line.strip()
    else:
        lines.append(line)
print("\n".join(lines))
print(
    f"\n**New installation:** download `nibe-gateway-alpine-{version}.img.xz`, check it against the `.sha256` and "
    'write it to an SD card (Raspberry Pi Imager → "Use custom", or `xzcat … | dd`), see '
    f"[flashing and provisioning](https://github.com/kroofy/nibe-gateway/blob/v{version}/docs/dev/image-build.md#flash)."
    f"\n\n**Existing gateway:** install from the Updates page, or upload `nibe-gateway-{version}.tar` there."
)
