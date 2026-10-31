# ADR 0105: Dedicated alarm reset, and automatic reset of alarm 251 only

Date: 2026-10-24 · Status: accepted (owner decision); amended 2026-10-25

## Context
The pump raises alarm 251 (lost communication with the accessory) after any accessory outage, including our own
restarts and updates, and keeps it until someone resets it. NibePi reset it automatically.

## Decision
- Alarm reset is its own path, separate from register writes.
- An optional automatic reset applies only to 251, and is rate-limited.
- Every other alarm is left to the owner.

## Why
- **Only 251:** the gateway causes 251 itself, so clearing it is cleaning up after ourselves. Other alarms signal
  real problems a human must see.
- **Separate path:** the reset register is too sensitive for the generic control allowlist.

## Amendments 2026-10-25
On hardware, resets were accepted without clearing a latched 251.
- **Retry interval:** the reset is now retried at a shorter interval while 251 persists.
- **Reset as a 0 → 1 change:** the reset is sent as a 0 → 1 change instead of a single 1. Hypothesis: the pump
  acts on the change, not on the value. The first trial passed (HP-31).

## Consequences
This is the one exception to "no automation" (ADR 0113).
