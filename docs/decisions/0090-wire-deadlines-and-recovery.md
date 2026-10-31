# ADR 0090: Tolerate isolated reply misses; bounded UART recovery, then escalate

Date: 2026-10-23 · Status: accepted

## Context
Originally, one missed reply deadline or one UART error ended the wire task for good, while health still said
"ready". The pump was left without its accessory and with alarm 251 (review finding F4).

## Decision
- Tolerate a few consecutive misses.
- Recover from UART errors by reopening the port with bounded backoff.
- When recovery is exhausted, exit with an error so the supervisor restarts the process.
- Health reports the wire state.
- Pump silence alone is degraded, not fatal.
- Timing values are configurable hypotheses.

## Why
- Transient hiccups should not kill the accessory.
- Retrying silently forever hides a broken state, so recovery is bounded and then escalates.
- A late reply is never sent outside its window, because it would be noise on the bus.

## Consequences
The timing values are tuned on hardware.
