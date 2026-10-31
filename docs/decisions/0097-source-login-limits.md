# ADR 0097: Login backoff per source address, plus a global cap

Date: 2026-10-23 · Status: accepted

## Context
A global login limit let any LAN client lock the administrator out indefinitely (review finding F8).

## Decision
- Back off per TCP peer address.
- Cap total password-hashing work globally.
- Ignore forwarded headers.

## Why
- An attacker cannot lock out the admin, who logs in from another address.
- The global cap protects the single CPU from hashing load.
- Forwarded headers can be spoofed.

## Consequences
Clients behind a shared proxy share one bucket.
