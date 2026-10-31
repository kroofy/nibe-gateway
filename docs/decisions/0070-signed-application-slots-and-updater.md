# ADR 0070: Signed A/B application slots with an independent, privileged updater

Date: 2026-10-22 · Status: accepted; OS updates in ADR 0106, probation in ADR 0096/0109

## Context
The application must be updatable remotely without site visits. A bad release must not brick the device. The
gateway runs unprivileged.

## Decision
- Two application slots.
- Releases are signed, and the private key exists only on the release machine.
- A separate privileged updater accepts a fixed set of operations from the gateway.
- Every activation is health-checked and rolled back automatically on failure.
- Every step is journaled, and recovery fails closed.

## Why
- **Signatures:** a compromised LAN or gateway cannot install code.
- **Separate updater:**
  - a crashing gateway cannot break its own rollback;
  - the unprivileged gateway never chooses paths, commands or keys.
- **Journal:** power can disappear at any step, and an unknown state must resolve to the known-good slot.

## Consequences
- The OS needs its own mechanism (ADR 0106).
- Configuration has to stay readable across a rollback (ADR 0095).
