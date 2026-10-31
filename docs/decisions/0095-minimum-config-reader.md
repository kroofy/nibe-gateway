# ADR 0095: Config declares the oldest version that may read it

Date: 2026-10-23 · Status: accepted

## Context
After a rollback, the older binary reads a config written by the newer one. If it fails closed on the unknown
schema, the rollback target dies too (review finding F6).

## Decision
- The config carries a minimum-reader version.
- Additive changes keep the minimum, and unknown fields are preserved.
- Incompatible changes raise the minimum and cannot ship as rollback-safe application updates.

## Why
- Rollback keeps working without the updater ever touching credentials.
- The compatibility promise becomes explicit instead of implied.

## Rejected
Having the updater snapshot the config. It would need access to credentials, and NetworkManager's state lives
elsewhere anyway.
