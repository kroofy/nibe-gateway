# ADR 0106: Full-system A/B slots with firmware tryboot

Date: 2026-10-24 · Status: accepted (owner decision), verified on hardware (HP-29)

## Context
Application slots cannot update the OS, the kernel or the updater itself. The Pi sits inside the heat pump, and
the owner must never need a site visit for an OS update.

## Decision
- Two OS root partitions, each with its own boot files.
- A new slot is tested with the firmware's one-time "tryboot".
- The slot is committed only after it confirms it is healthy. Otherwise the next boot returns to the old slot.

## Why
- Rollback happens at firmware level, so it works even when the new kernel never boots.
- An in-place package upgrade cannot be rolled back, and a broken one would mean a site visit.

## Consequences
- Root storage is doubled.
- The shared firmware is never updated over the air and needs a reflash.
- FAT's non-atomic rename leaves a tiny window at commit.
