# ADR 0107: Persistent system state is declared in one list; no encryption at rest

Date: 2026-10-24 · Status: accepted (owner decision)

## Context
The root is read-only (ADR 0031). Wi-Fi profiles, SSH keys and Tailscale state still have to survive reboots and
OS updates.

## Decision
- Persistent system state lives on the data partition and is mounted at boot from one declared list.
- Nothing is encrypted at rest.

## Why
- **One list:** what survives is explicit and reviewable, and nothing persists by accident.
- **No encryption:** with no key outside the card and no secure element on the Pi Zero, encryption would only
  obscure.

## Consequences
- A new persistent feature adds one entry to the list.
- Secrets are protected by file permissions, as on any Linux host.
