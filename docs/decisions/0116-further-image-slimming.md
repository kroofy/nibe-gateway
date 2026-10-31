# ADR 0116: Further slimming of Raspberry Pi OS, squashfs root, no Python

Date: 2026-10-25 · Status: superseded by ADR 0117 (squashfs root and no Python carried over)
Numbers: [docs/research/image-size-2026-10-25.md](../research/image-size-2026-10-25.md)

## Context
After ADR 0112, OS updates were still about 225 MiB compressed and took minutes to write on the Pi Zero.

## Decision
- More package removal.
- A compressed squashfs root.
- Python's two scripts rewritten in Rust, and Python removed.

## Why
- **Squashfs:** a compressed, inherently read-only root cuts update size and write time, and can't be modified by
  accident.
- **No Python:** dropping it saved RAM and image size.

## Rejected at the time
- Delta updates: new failure modes for little gain.
- EROFS: no zstd support on this OS.
- Buildroot/Yocto: owning a whole distribution.

## Superseded
ADR 0117 reconsidered the base itself.
