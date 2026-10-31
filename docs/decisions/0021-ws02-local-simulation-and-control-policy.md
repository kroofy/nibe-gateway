# ADR 0021: Deterministic simulation; simulated controls can never become real ones

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted

## Context
Most development happens without hardware. Controls have to be testable without any risk to the pump.

## Decision
- A deterministic simulator with an injected clock and checked-in scenarios.
- Development endpoints exist only in a dev build.
- Simulated controls need an explicit opt-in, and that opt-in can never authorize a write on hardware or during
  replay.

## Why
- Deterministic scenarios make tests repeatable.
- A production build contains nothing that manipulates time or state.
- A flag meant for the simulator must not be able to enable a real write by mistake.

## Consequences
- Hardware writes have their own switches and rules (ADR 0125).
- Limits used by the simulator are UI fixtures, not claims about what is safe on an F730.
