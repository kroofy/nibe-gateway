# ADR 0031: Read-only root, separate persistent data, health-gated supervision

Date: 2026-10-22 · Status: accepted; partition layout extended by ADR 0106, base OS replaced by ADR 0117

## Context
The Pi loses power whenever the heat pump is switched off, and SD cards corrupt when written during power loss.
A disconnected pump is a normal condition, not an application failure.

## Decision
- The root filesystem is read-only, with a RAM overlay.
- Everything that must persist lives on a dedicated data partition.
- Supervision judges the gateway by its actual HTTP health.
- A pump disconnection is reported as degraded, not as unhealthy.

## Why
- A read-only root survives power cuts and makes OS updates whole-image swaps.
- Separate data survives OS updates.
- Restarting the API because a cable is unplugged fixes nothing and hides the real fault.

## Consequences
Every persistent path must be declared (ADR 0107). The model outlived the change of base OS.
