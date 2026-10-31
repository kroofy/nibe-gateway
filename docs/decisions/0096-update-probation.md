# ADR 0096: Probation after activation, with conservative rollback

Date: 2026-10-23 · Status: accepted; completion rule superseded by ADR 0109

## Context
A release could pass three quick health probes and then crash, leaving a restart loop with no rollback (review
finding F7).

## Decision
After activation, the gateway is on probation. Any of these rolls it back:
- restarts;
- fatal wire errors;
- sustained unavailability;
- an interrupted probation.

## Why
- Many failures appear only after startup.
- An interrupted update is an unknown state, and an unknown state should return to known-good.
