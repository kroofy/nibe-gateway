# ADR 0108: Optional provisioning file on the boot partition

Date: 2026-10-24 · Status: accepted (owner decision)

## Context
The owner flashes the card on a laptop, is then remote, and cannot read a secret the device generates. The setup
access point (ADR 0060) needs someone physically nearby.

## Decision
- An optional file on the FAT boot partition carries Wi-Fi, admin password, SSH keys and a Tailscale key.
- It is consumed on first boot, applied through the normal code paths, then deleted.

## Why
- Any laptop can write to the FAT partition.
- The normal code paths give the same validation and fallback as the UI.
- Deleting the file removes secrets from a filesystem with no permissions.

## Consequences
- Secrets sit on FAT until the first boot.
- Without the file, the setup access point flow is unchanged.
