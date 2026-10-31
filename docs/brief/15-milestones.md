# Delivery milestones & acceptance

> Source: ORIGINAL_BRIEF.md §18

## 18. Delivery milestones and acceptance

### M0 — Research and feasibility
Inspect NibePi source, F730 protocol, register maps, RS485 hardware, ARMv6 toolchain, pi-gen. Document proven facts, assumptions, unknowns, and safe migration plan. **Accept:** feasibility report with explicit evidence and blockers.

### M1 — Local foundation
Rust app, simulated F730, API/OpenAPI, basic React admin UI, deterministic scenarios, automated tests. **Accept:** clone, run, inspect UI/API, and verify without hardware.

### M2 — Protocol and register system
Real parser/encoder, catalog, F730 adapter, capture replay, explorer, safe discovery framework. **Accept:** recorded F730 frames decode into verified device state and pass repeatable tests.

### M3 — Provisioning and administration
Simulated and NetworkManager-backed Wi-Fi workflows, admin auth, persistence, diagnostics. **Accept:** provisioning/recovery tests locally and AP/station tests on hardware.

### M4 — Pi appliance
ARMv6 cross-build, systemd, serial setup, read-only root/persistent storage, pi-gen image. **Accept:** fresh flash boots into discoverable gateway and provisioning works.

### M5 — OTA and dev workflow
SSH deployment, logs/debug/capture, signed A/B app releases and rollback. **Accept:** deploy, debug, update, recover, and roll back without reflashing.

### M6 — Real F730 integration
Physical communication, validated readings, disconnection recovery, approved write operations. **Accept:** stable operation against F730; control behavior explicitly verified and safe.

### M7 — Stabilization and release
Soak tests, power-loss/recovery, docs, CI/release packaging. **Accept:** non-developer can flash, provision, use, and update the appliance.
