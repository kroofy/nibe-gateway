# ADR 0098: Durable per-installation register evidence

Date: 2026-10-23 · Status: accepted

## Context
What had been seen or read on this pump was lost at every restart (review finding F10).

## Decision
- Persist that evidence separately from the catalog.
- Write only when something changes.
- A corrupt file stops startup instead of being reset.
- Evidence never affects write permission.

## Why
- Knowledge gathered over months is valuable.
- Writing on change spares the SD card.
- Losing evidence silently is worse than failing loudly.
- Evidence of reading says nothing about whether writing is safe.

## Consequences
A power cut can lose the most recent change.
