# ADR 0041: Catalog claims, evidence and notes about registers stay in separate layers

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted

## Context
Three kinds of knowledge exist about a register:
- **Catalog:** what the sources claim, shared by every F730.
- **Evidence:** what this gateway has seen or read on this pump.
- **Notes:** human judgement about what a register means here, for example "not used here" or "checked against
  the display".

Most registers that answer are constant or irrelevant on the owner's pump. Blurring the layers leads to trusting
unverified numbers.

## Decision
- The catalog, per-installation evidence and per-installation notes are separate stores.
- Evidence persists, is written only when it changes, and a corrupt evidence file stops startup instead of being
  reset.
- Only an admin edits notes. The gateway ships none.
- Neither evidence nor notes affect what may be written, polled or published.
- The pump's product information, not the catalog, establishes its identity.
- Ambiguous decodings, such as an unknown word order, stay raw instead of being guessed.

## Why
- A plausible but wrong value is worse than a visibly raw one, and provenance lets the owner judge a value.
- Notes in the catalog would make shared data specific to one installation. Notes in the evidence would mix
  judgement with automatic facts.
- Evidence and notes can mislead a reader, but can never change the pump.
- Evidence gathered over months is valuable: losing it silently is worse than failing loudly. Writing only on
  change spares the SD card.

## Consequences
Fixes that are true for every F730 go into the catalog. A power cut can lose the most recent evidence change.
