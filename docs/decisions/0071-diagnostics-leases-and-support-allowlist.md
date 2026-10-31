# ADR 0071: Expiring diagnostic leases and allowlisted support reports

Date: 2026-10-22 · Status: accepted

## Context
Support needs diagnostics. Raw serial data and logs can contain sensitive information, and verbose logging left
on wears the SD card.

## Decision
- An admin enables verbose diagnostics through a time-limited lease.
- The support report is built from an allowlist of non-sensitive fields.
- Raw captures are a separate, explicit export.

## Why
- An allowlist is safe by construction. Redaction is only as good as its patterns.
- Expiry prevents a forgotten debug mode.
- Keeping raw data separate makes sharing it a conscious choice.

## Consequences
The support report is small. Deeper investigation goes through SSH.
