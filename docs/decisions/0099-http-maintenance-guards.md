# ADR 0099: Strict browser headers and deliberate downgrades

Date: 2026-10-23 · Updated: 2026-10-31 · Status: accepted

## Context
The integration review found that downgrades happened silently and that the UI had no framing or content-type
protections.

## Decision
- A downgrade needs explicit confirmation.
- Every response carries a strict content security policy and anti-framing, anti-sniffing and referrer headers.

## Why
- A downgrade can reintroduce known bugs. It stays possible, but deliberate.
- The UI loads nothing third-party (ADR 0050), so strict headers cost nothing and add defence in depth.

## Consequences
A UI change that needs an external resource or inline code must loosen the policy deliberately.
