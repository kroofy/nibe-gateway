# ADR 0117: Alpine Linux base, built up from an explicit package list

Date: 2026-10-25 · Status: accepted, live on hardware (HP-38); supersedes ADR 0112/0116's base

## Context
Even trimmed down, Raspberry Pi OS was 85–100 MiB per OS update, and removing packages from it was fragile.

## Decision
- Use Alpine Linux armhf with OpenRC.
- Build the root from an explicit allowlist: anything not listed is absent.
- Gateway binaries are statically linked.
- The slot and bundle format and the data partition stay unchanged, so the migration was an ordinary OS update
  with the old slot as fallback.

## Why
- An allowlist puts nothing on the device by accident. A removal list misses things.
- Alpine targets ARMv6 natively.
- It is a maintained distribution with security updates. With Buildroot or Yocto, we would own every package.
- Static binaries ran on both bases during the migration.

## Consequences
- The systemd sandboxing and journald were lost (see ADR 0118).
- Every hardware-verified behaviour had to be re-verified on the new base.
- How the image is built: `docs/dev/image-build.md`.
