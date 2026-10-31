# ADR 0103: Read-only WebSocket stream of readings for integrations

Date: 2026-10-24 · Status: accepted

## Context
Scripts and home automation want readings pushed to them. The UI's server-sent events only signal "refetch".

## Decision
- An authenticated, read-only WebSocket of readings.
- The browser origin is checked when one is present.
- SSE stays for the UI.

## Why
- WebSockets are what integration clients commonly expect.
- Browsers don't apply CORS to WebSockets, so the origin check prevents cross-site use of a session.
- Read-only keeps the stream out of the write path.

## Consequences
One more dependency (WebSocket support in axum).
