# ADR 0110: Scoped API keys, separate from the admin account

Date: 2026-10-25 · Status: accepted (owner decision); amended 2026-10-25

## Context
Scripts, Home Assistant and remote tooling need API access without the admin password.

## Decision
- An admin creates API keys, scoped to either `read` or `control`.
- Keys are shown once and stored hashed.
- Keys never reach admin functions: setup, auth, network, updates, diagnostics or key management.
- Changing the admin password revokes all keys.

## Why
- Automation credentials can leak without exposing the admin account.
- Random 256-bit tokens don't need a slow hash. A slow one would cost about a second per request on the Pi.
- Bearer headers are not exposed to CSRF.
- "Last used" is kept in RAM only, to avoid an SD write per request.

## Amendment
Control keys may run read probes. Probes only read and are bounded (ADR 0101).
