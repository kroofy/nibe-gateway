# ADR 0091: A modest scheduling advantage for the UART thread, not realtime

Date: 2026-10-23 · Status: accepted

## Context
On a single core, password hashing and update verification compete with replies to the pump.

## Decision
- Raise only the UART thread's priority, modestly, within the normal scheduler.
- Report when the raise could not be applied.

## Why
- Realtime scheduling can starve housekeeping and needs more privilege.
- A small boost helps under contention, with little risk.

## Consequences
This is not a hard timing guarantee.
