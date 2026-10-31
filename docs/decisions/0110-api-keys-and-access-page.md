# ADR 0110: Scoped API keys, separate from the admin account

Date: 2026-10-25 · Updated: 2026-10-31 · Status: accepted (owner decision)

## Context
Scripts, Home Assistant and remote tooling need API access without the admin password.

## Decision
- An admin creates API keys, scoped to either `read` or `control`. A control key may also start reads.
- Keys are shown once and stored hashed.
- Keys never reach admin functions: setup, auth, network, updates, diagnostics or key management.
- Changing the admin password revokes all keys.

## Why
- Automation credentials can leak without exposing the admin account.
- Bearer headers are not exposed to CSRF.
- Reads never change the pump, so a key that may write may also read.

## Rejected
- A slow password hash for keys: random 256-bit tokens don't need one, and it would cost about a second per
  request on the Pi.
- Persisting "last used": an SD write per request. It is kept in RAM only.
