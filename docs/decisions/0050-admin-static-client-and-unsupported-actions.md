# ADR 0050: Small same-origin admin UI that never overstates what it knows

Date: 2026-10-22 · Status: accepted

## Context
The UI is served by a Pi Zero, used from phones on a home network that may have no internet, and shows the state
of a heat pump.

## Decision
- A static React/TypeScript build, served by the gateway from the same origin.
- No CDN or cloud services, and only minimal runtime dependencies.
- API types are generated from the gateway's OpenAPI.
- No secrets in browser storage.
- Stale data is shown as stale, and unavailable features as unavailable.

## Why
- Same origin keeps authentication simple: cookies, no CORS.
- With no external assets, the UI works offline.
- A stale value shown as current, or a failure shown as zero, could lead to a wrong decision about the heating.

## Consequences
Browser tests run against the real simulated gateway.
