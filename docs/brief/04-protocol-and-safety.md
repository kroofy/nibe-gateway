# Heat-pump protocol & safety rules

> Source: ORIGINAL_BRIEF.md §6

## 6. Heat-pump protocol and safety

- Start with read-only monitoring. Verify each measurement against known captures and the F730's own display.
- Detect disconnection, malformed frames, stale readings, and communication errors; show freshness explicitly.
- Implement writable controls only after register identity, type, scaling, range, acknowledgement, and failure semantics have been verified on the actual device.
- No generic unaudited write-register endpoint in the public API. Do not bypass heat-pump safety mechanisms.
- HTTP traffic, slow browser clients, or diagnostics must never starve serial communication.
- Never perform experimental writes against the physical pump without explicit approval and a verified safety plan.
