# ADR 0112: Slim the Raspberry Pi OS root

Date: 2026-10-25 · Status: superseded by ADR 0117

## Context
The first OS bundle was 2.15 GB, mostly a stock root built for every board and every use. Each OS update took
15 minutes and wrote 2 GB to the SD card.

## Decision
- Remove packages the appliance doesn't use, and support ARMv6 boards only.
- Commit an OS update only when remote access (SSH, Tailscale) still works.

## Why
- Smaller updates mean less time and less SD wear.
- The remote-access check guarantees that slimming which breaks access rolls back on its own.

## Superseded
Removing packages from a full distribution proved fragile. ADR 0117 builds the root from an allowlist instead.
The remote-access commit check remains.
