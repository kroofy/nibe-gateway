# ADR 0104: Writes only through named, individually verified and enabled controls

Date: 2026-10-24 · Status: accepted (owner decision); relaxes ADR 0011

## Context
The owner wants to adjust the pump. ADR 0011 forbade all writes.

## Decision
A write goes through a named control for a single register, and only when:
- the register has been verified on hardware;
- it is explicitly enabled in the config;
- the value is inside the catalog's range.

A write is sent once, never retried, and read back. There is never a generic register write.

## Why
- A named control encodes intent and limits. A register-number write would let any bug or client change any setting.
- A timed-out write has an unknown outcome, so retrying could apply it twice or at the wrong time.
- The read-back is the evidence.

## Consequences
Every new control costs a supervised hardware test.
