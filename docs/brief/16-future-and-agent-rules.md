# Future integrations & instructions for implementing agents

> Source: ORIGINAL_BRIEF.md §19, §20

## 19. Future optional integrations

- Matter bridge exposing appropriate thermostat/temperature capabilities to Apple Home; verify supported device types and commissioning on the constrained Pi before committing.
- Additional NIBE F-series models via adapters/catalogs.
- Optional MQTT, Home Assistant, Prometheus, external history and energy analytics.
- Full OS OTA only with tested boot-level recovery.

None are mandatory to core operation.

## 20. Instructions for implementing agent

1. **Start at M0.** Inspect NibePi rather than inventing protocol behavior.
2. Distinguish verified hardware/protocol facts from hypotheses.
3. Build small runnable vertical slices with observable acceptance tests.
4. Keep local simulation working throughout development.
5. Share domain validation and API logic across all execution modes.
6. Verify UI in a real browser and API via actual HTTP calls.
7. Do not deploy experimental writes to the physical heat pump without approval.
8. Prefer minimal dependencies and maintainable interfaces over unnecessary abstraction.
9. Document decisions, failure modes, test commands, and deployment/rollback procedures.
10. Keep the existing functioning NibePi installation recoverable during migration.
