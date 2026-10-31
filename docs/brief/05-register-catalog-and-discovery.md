# Register catalog & discovery

> Source: ORIGINAL_BRIEF.md §7

## 7. Register catalog and discovery

Maintain versioned definitions with register ID, name, description, data type, scaling, unit, access mode, supported models/firmware, source, and verification state. Support:

- Device identity/model/firmware detection where protocol permits.
- Import of manufacturer/community register definitions, including offline upload.
- Passive observation of registers in real traffic.
- **Safe active probing** of candidate *read-only* registers only if the actual protocol supports it; authenticated, rate-limited, cancellable, and never blind address-space scanning.
- Distinct states: catalog-known, observed, confirmed readable, unsupported, unverified, unknown/undecoded.

A register's presence in a catalog must not imply support on this specific pump. Unknown registers must never become writable automatically. Provide an admin register explorer with raw/decoded values, units, timestamps, source, errors, and verification state. Reproduce discovery behavior with protocol fixtures.
