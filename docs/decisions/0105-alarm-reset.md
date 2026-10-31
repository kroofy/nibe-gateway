# ADR 0105: Dedicated alarm reset, and automatic reset of alarm 251 only

Date: 2026-10-24 · Updated: 2026-10-31 · Status: accepted (owner decision)

## Context
The pump raises alarm 251 (lost communication with the accessory) after any accessory outage, including the
gateway's own restarts and updates, and keeps it until someone resets it. NibePi reset it automatically.

## Decision
- Alarm reset is its own control, separate from the named register controls.
- An optional automatic reset applies only to 251. It is rate-limited and repeated while 251 persists.
- Every other alarm is left to the owner.

## Why
- The gateway causes 251 itself, so clearing it is cleaning up after itself. Other alarms signal real problems
  that a human must see.
- On hardware, a single reset did not always clear a latched 251.

## Rejected
- Resetting every alarm automatically, as NibePi did: it hides real faults.

## Consequences
This is one of the exceptions to "no automation" (ADR 0113).
