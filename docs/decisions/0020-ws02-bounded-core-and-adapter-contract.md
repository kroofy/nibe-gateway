# ADR 0020: Hardware-independent core, cached state, isolated device adapters

Date: 2026-10-22 · Status: accepted

## Context
The same API has to work with the simulator, with capture replay and with the real pump. The pump expects replies
within a tight window. HTTP clients are slow, numerous and unpredictable. The device has 512 MB of RAM.

## Decision
- One core serves every device mode, and adapters sit behind a common interface.
- HTTP serves cached state.
- Work the pump timing depends on never runs inside request-driven or cancellable code.
- Every queue is bounded. A full queue answers "busy" at once instead of waiting.

## Why
- A slow client, a lagging subscriber or a cancelled request must never delay a reply to the pump or grow memory.
- Cached reads make HTTP cost independent of the bus.
- A mode that isn't implemented fails explicitly rather than being simulated quietly.

## Consequences
- Freshness is explicit: old data shows as stale, never as current or zero.
- A write that times out has an unknown outcome, so it is never retried blindly.
- Clients refetch after reconnecting.
