# Security & operational constraints

> Source: ORIGINAL_BRIEF.md §16

## 16. Security and operational constraints

- Local-first; no cloud account or mandatory internet.
- No universal default admin password.
- Secure setup credentials and stored secrets; no secrets in logs/support bundles.
- Authenticated API/admin and authorization for writes, updates and debug.
- No arbitrary register writes; explicit validated command set.
- Minimal exposed services, no public internet exposure by default.
- Reliable serial servicing independent of frontend/API.
- Stale readings and connection failures clearly surfaced.
- Safe service restart, power-loss recovery, network recovery, and update rollback.
