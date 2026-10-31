# ADR 0114: On-device history in SQLite, committed every minute

Date: 2026-10-25 · Status: accepted (owner decision) · Numbers: [docs/research/history-storage.md](../research/history-storage.md)

## Context
The owner wants graphs covering at least 1–2 years on the Pi itself. The Pi loses power whenever the pump is
switched off, and NibePi lost up to an hour of data on every power cut. The Pi has no real-time clock.

## Decision
- History lives in SQLite on the data partition.
- Raw samples are kept briefly, aggregates for years.
- Writes are batched into short commits.
- Writing never blocks the wire thread.
- No samples are stored before the clock can be trusted.
- An older binary leaves a newer schema alone.

## Why
- **SQLite:** its crash safety, queries and retention are proven. Custom files would need their own recovery,
  rollups and retention code.
- **Short commits:** they bound what a power cut loses, at an acceptable SD write cost.
- **Trusted clock:** wrong timestamps would corrupt history permanently.
- **Older binary skips newer schema:** rollbacks must not damage data.

## Rejected
- Custom append-only files: more code to make crash-safe.
- Longer flush intervals: they save little and lose more on every pump shutdown.

## Consequences
- A C dependency.
- More SD writes than custom files.
