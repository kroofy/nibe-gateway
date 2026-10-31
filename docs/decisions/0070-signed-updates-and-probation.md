# ADR 0070: Signed A/B application updates, an independent updater, and probation that ends on evidence

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
The application must be updatable remotely without site visits, and a bad release must not brick the device. The
gateway runs unprivileged. Many failures appear only after startup, and power can disappear at any step.

## Decision
- Two application slots. Releases are signed, and the private key exists only on the release machine.
- A separate privileged updater accepts a fixed set of operations from the gateway. Every step is journaled, and
  recovery fails closed to the known-good slot.
- After activation the gateway is on probation. Restarts, fatal wire errors, sustained unavailability or an
  interrupted probation roll it back.
- Probation completes on evidence: a short period of continuous health after one complete poll round has been
  answered. A fixed window is the upper bound, and the fallback when no rounds happen.

## Why
- With signatures, a compromised LAN or gateway cannot install code.
- A crashing gateway cannot break its own rollback, and the unprivileged gateway never chooses paths, commands or
  keys.
- An unknown state resolves to the known-good one.
- Talking to the pump is what matters. Across real updates nothing changed after the first complete round, and
  failures that only appear later aren't caught by waiting longer anyway.

## Rejected
- A few quick health probes as the only check: a release passed them and then crashed into a restart loop with no
  rollback.
- A long fixed probation: it dominated update time and proved nothing extra.

## Consequences
The OS needs its own mechanism (ADR 0106). Configuration must stay readable across a rollback (ADR 0095).
