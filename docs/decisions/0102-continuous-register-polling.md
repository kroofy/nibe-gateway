# ADR 0102: Continuous polling on free read tokens

Date: 2026-10-24 · Status: accepted; builds on ADR 0101

## Context
Live data needs read requests (ADR 0101), and the pump offers only about one read opportunity per second.

## Decision
- Poll a configured, bounded list of registers round-robin.
- Interactive probes and write read-backs go first.
- A read that times out is skipped, not retried.

## Why
- User actions should not wait behind background polling.
- One silent register must not stall the rest.
- A bounded list keeps the refresh rate predictable.

## Consequences
The refresh interval grows with the list, so staleness thresholds must match it.
