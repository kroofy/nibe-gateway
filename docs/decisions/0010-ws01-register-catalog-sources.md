# ADR 0010: Register catalog from NibePi and NIBE's export; conflicts recorded, not applied

Date: 2026-10-22 · Status: accepted

## Context
We needed an F730 register catalog before any hardware access. Sources: NibePi (MIT, what the owner ran), NIBE's
ModbusManager export (manufacturer data), yozik04/nibe (GPL-3.0, community corrections), openHAB (no F730).

## Decision
Base the catalog on NibePi and cross-check it against the manufacturer export. Record community corrections as
visible conflicts instead of applying them. Nothing in the catalog counts as verified for this pump.

## Why
- NibePi matched the installation that was running and is license-clean.
- Applying unverified corrections silently would hide the fact that sources disagree. Only the pump's display can
  settle a disagreement.
- A catalog is a claim about F730s in general, not evidence about this pump.

## Rejected
- Copying yozik04/nibe's data: it would tie the project to the GPL.

## Consequences
- Conflicts are settled against the pump display.
- Knowledge about one installation lives outside the shared catalog (ADR 0041).
- Text derived from NIBE's export may have unclear redistribution terms. Review it before publishing the catalog.
