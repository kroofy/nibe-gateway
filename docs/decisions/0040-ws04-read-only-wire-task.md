# ADR 0040: The wire task is isolated, bounded and prefers replies over data

Date: 2026-10-22 · Status: accepted; recovery refined by ADR 0090, priority by ADR 0091, writes added by ADR 0104

## Context
The pump expects an accessory reply within a tight window. On a single-core Pi, HTTP, password hashing, disk and
decoding all compete for the CPU.

## Decision
- Serial handling runs on its own thread, independent of the application runtime.
- The reply is sent before observations are published.
- Under load, observations may be dropped.
- At first, no write-request encoder existed at all.
- Replayed captures never treat recorded transmissions as permission to transmit.
- Wire failures are visible.

## Why
- A lost observation is harmless. A missed reply raises an alarm on the pump.
- Without a write encoder, an accidental write was impossible rather than merely disallowed.
- Replay must not become a way to send bytes to a real bus.

## Consequences
The protocol details live in `crates/nibe-protocol` and `docs/research/protocol-summary-for-implementers.md`.
