# ADR 0119: Per-installation register notes, separate from the catalog and the evidence

Date: 2026-10-26 · Status: accepted

## Context
Three kinds of register knowledge exist:
- **Catalog:** generic F730 claims, shared by every installation.
- **Evidence:** automatic records of what this gateway saw or read (ADR 0098).
- **Meaning on this pump:** human judgement, for example "not used here", "checked against the display",
  "suspicious".

Most of the 576 registers that answer are constant or irrelevant on the owner's pump.

## Decision
- Keep human notes in a third, per-installation store.
- Only an admin edits them.
- They never affect write permission, polling or integrations.

## Why
- Notes in the catalog would make shared data specific to one installation.
- Notes in the evidence store would mix human judgement with automatic facts.
- Keeping them inert means a wrong note can mislead a reader, but cannot change the pump.

## Consequences
- Fixes that are true for every F730 still go into the catalog.
- The gateway ships no notes. The owner's set is generated from research data and applied once.
