# ADR 0099: Deliberate downgrades, browser hardening headers, truthful status

Date: 2026-10-23 · Status: accepted

## Context
The integration review found several issues:
- downgrades happened silently;
- the UI had no framing or content-type protections;
- rollbacks to the same slot were confusing;
- status lacked uptime and addresses.

## Decision
- A downgrade needs explicit confirmation.
- A redundant rollback is rejected.
- Every response carries a strict CSP and anti-framing/sniffing/referrer headers.
- Status reports real uptime and addresses, never invented ones.

## Why
- A downgrade can reintroduce known bugs. It stays possible, but deliberate.
- The UI loads nothing third-party, so strict headers cost nothing and add defence in depth.
