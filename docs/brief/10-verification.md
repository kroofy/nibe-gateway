# Verification strategy

> Source: ORIGINAL_BRIEF.md §12

## 12. Verification strategy

- **Unit:** frame parsing, register decoding/scaling, validation, state transitions.
- **Property-based:** frame bounds, invalid input, numeric edge cases, invariants.
- **Replay:** sanitized real F730 captures processed deterministically.
- **API contracts:** OpenAPI compatibility, auth, error semantics, command validation.
- **Browser:** Playwright flows for login, readings, register explorer, provisioning, and updates.
- **Fault injection:** serial disconnect, malformed messages, network failures, update failures.
- **Hardware:** RS485 timing, electrical integration, actual readings, approved controls, recovery.

For each important feature provide a repeatable initial state, action, observable result, and assertion. Avoid tautological tests that simply restate implementation. AI coding agents should be able to run and inspect these verification surfaces locally.
