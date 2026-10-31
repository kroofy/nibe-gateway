# ADR 0117: Alpine Linux base, built up from an explicit package list

Date: 2026-10-25 · Updated: 2026-10-31 · Status: accepted (owner decision) · Numbers:
[image-size-2026-10-25.md](../research/image-size-2026-10-25.md)

## Context
The first OS bundle, a stock Raspberry Pi OS root, was over 2 GB, and every OS update took a quarter of an hour
and wrote all of it to the SD card. Even trimmed down, Raspberry Pi OS stayed around 100 MiB per update.

## Decision
- Use Alpine Linux armhf with OpenRC.
- Build the root from an explicit allowlist: anything not listed is absent.
- The root is a compressed, read-only squashfs. No Python on the device.
- Gateway binaries are statically linked.
- The slot and bundle format and the data partition stayed unchanged, so the move was an ordinary OS update with
  the old slot as fallback.

## Why
- An allowlist puts nothing on the device by accident.
- Alpine targets ARMv6 natively, and it is a maintained distribution with security updates.
- A compressed root cuts update size and write time, and can't be modified by accident.

## Rejected
- Removing packages from Raspberry Pi OS: tried twice. A removal list kept missing things and broke in fragile
  ways.
- Buildroot or Yocto: we would own every package.
- Delta updates: new failure modes for little gain.
- EROFS: no zstd support at the time.

## Consequences
- systemd's sandboxing and journald are gone (ADR 0118).
- Every hardware-verified behaviour had to be re-verified on the new base.
