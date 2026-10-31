# ADR 0021: Deterministic simulation; simulated controls can never become real ones

Date: 2026-10-22 · Status: accepted

## Context
Most development happens without hardware. Controls have to be testable without any risk to the pump.

## Decision
- A deterministic simulator with an injected clock and checked-in scenarios.
- Development endpoints exist only in a dev build.
- Simulated controls need an explicit opt-in, and that opt-in can never authorize hardware or replay writes.
- There is no generic register write.

## Why
- Deterministic scenarios make tests repeatable.
- A production build contains nothing that manipulates time or state.
- A flag meant for the simulator must not be able to enable a real write by mistake.

## Consequences
- Hardware controls needed their own reviewed gate (ADR 0104).
- Limits used by the simulator are UI fixtures, not claims about what is safe on an F730.
