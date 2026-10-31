# ADR 0041: Catalog claims, observations and confirmed support stay separate

Date: 2026-10-22 · Status: accepted; hardware read gate relaxed by ADR 0101

## Context
Three kinds of knowledge exist about a register:
- **Catalog claim:** what the sources say.
- **Observation:** what the bus has shown.
- **Confirmed support:** a matching read succeeded.

Blurring them leads to trusting unverified numbers or granting writes on weak evidence.

## Decision
- Keep these as separate, explicit states.
- A catalog import cannot grant write permission.
- The pump's product information, not the catalog, establishes its identity.
- Ambiguous decodings, such as unknown word order, stay raw instead of being guessed.
- Read probes need explicit authorization and are rate-bounded.

## Why
- A plausible but wrong value is worse than a visibly raw one.
- Showing provenance lets the owner judge a value for themselves.

## Consequences
Durable evidence (ADR 0098) and human notes (ADR 0119) are added later as separate layers.
