# NIBE Gateway — Complete Project Brief

**Version:** 0.1 · **Date:** 2026-10-22 · **Status:** Proposed architecture  
**Initial target:** NIBE F730 with Raspberry Pi Zero WH and existing NibePi RS485 interface

## 1. Vision

Build a lightweight, open-source, local-first NIBE heat-pump gateway in Rust. Replace the legacy NibePi runtime (Node-RED/Node.js and associated services) with a purpose-built appliance exposing a secure HTTP API and browser administration UI. It must work without Home Assistant, MQTT, cloud services, or an external server. A custom dashboard on an Android tablet can consume its API separately.

**User experience:** Flash an SD card once → power on → join the gateway's secure temporary Wi-Fi → select home Wi-Fi and configure credentials in a browser → discover gateway on LAN → monitor/control supported heat-pump functions → update and debug over the network without reflashing.

**Developer experience:** Develop and verify locally with simulated hardware and replayed F730 protocol captures. Cross-compile and deploy over SSH when physical testing is needed.

## 2. Goals and non-goals

### Goals
- Minimal, reliable Rust daemon suitable for ARMv6 single-core / 512 MB RAM.
- Correct F-series MODBUS 40 accessory protocol, starting with F730.
- Versioned HTTP API, OpenAPI schema, live telemetry, safe validated controls.
- Embedded responsive web administration UI and authenticated setup.
- Secure first-boot Wi-Fi access-point provisioning, mDNS, and recovery.
- Versioned register catalog, passive discovery, safe read-only probing where supported, and register explorer.
- Deterministic local simulation, real-frame replay, and observable verification surfaces.
- Signed application OTA, rollback, SSH dev deployment, protected diagnostics.
- Reproducible 32-bit Raspberry Pi OS Lite image with read-only root and persistent config.
- Extensible model adapters and optional future Matter/Apple Home support.

### Non-goals for initial release
- General-purpose home automation, automation rules, cloud backend, mandatory MQTT, Node-RED, Docker, or heavyweight databases.
- Native phone apps or rich historical analytics hosted on the Pi.
- Matter certification or full OS OTA before a tested boot-level recovery strategy exists.

## 3. Hardware and feasibility investigation

- **Board:** Raspberry Pi Zero WH; BCM2835 ARMv6 1 GHz; 512 MB RAM; 2.4 GHz Wi-Fi; soldered 40-pin header.
- **Heat pump:** NIBE F730, connected through existing NibePi RS485 hardware.
- **Reference:** https://github.com/anerdins/nibepi

Before implementing the driver, inspect NibePi source, pin assignments, serial configuration, frame formats, timing, checksums, acknowledgement behavior, register maps, and accessory activation requirements. The F-series MODBUS 40 interface must **not** be assumed to behave like arbitrary generic Modbus RTU. Record unknowns and capture real traffic where needed. Preserve the functioning existing installation until a safe migration and rollback are demonstrated.

## 4. Proposed stack

| Area | Choice |
|---|---|
| Core/runtime | Rust, Tokio |
| HTTP | Axum, Serde, OpenAPI |
| Admin UI | React, TypeScript, Vite; prebuilt static assets served by Rust |
| Network provisioning | NetworkManager over D-Bus; system networking handles AP/DHCP/DNS |
| Device service | systemd |
| OS/image | 32-bit Raspberry Pi OS Lite, pi-gen |
| Verification | Rust unit/integration tests, proptest, protocol replay, Playwright |
| Automation | GitHub Actions, just |
| Hardware build | ARMv6 Linux cross-compilation on developer machine/CI |

Avoid Node.js on the production Pi; it is only a frontend build/development dependency. Keep production dependencies and SD-card writes minimal.

## 5. Architecture and boundaries

1. **Protocol transport:** RS485 IO, frame encoding/decoding, timing, checksums, acknowledgements, retries, tracing, and failure recovery. Must remain responsive independently of HTTP clients.
2. **Device adapters:** F730 register definitions, scaling, units, firmware/model compatibility, capabilities, and safe commands. Permit other NIBE models later.
3. **Application core:** cached device state with timestamps, commands, validation, events, authentication/authorization boundaries, and diagnostics. Depends on interfaces, not hardware.
4. **Infrastructure adapters:** real/simulated serial transport, NetworkManager/simulated network, persistence, update service.
5. **Presentation:** versioned HTTP JSON API, live SSE or WebSocket events, OpenAPI, embedded admin UI.

Use the **same application core and API handlers** in simulated, replay, and physical-hardware modes. Replace only adapters. Prefer a small cohesive workspace; don't create crates merely for architectural aesthetics.

## 6. Heat-pump protocol and safety

- Start with read-only monitoring. Verify each measurement against known captures and the F730's own display.
- Detect disconnection, malformed frames, stale readings, and communication errors; show freshness explicitly.
- Implement writable controls only after register identity, type, scaling, range, acknowledgement, and failure semantics have been verified on the actual device.
- No generic unaudited write-register endpoint in the public API. Do not bypass heat-pump safety mechanisms.
- HTTP traffic, slow browser clients, or diagnostics must never starve serial communication.
- Never perform experimental writes against the physical pump without explicit approval and a verified safety plan.

## 7. Register catalog and discovery

Maintain versioned definitions with register ID, name, description, data type, scaling, unit, access mode, supported models/firmware, source, and verification state. Support:

- Device identity/model/firmware detection where protocol permits.
- Import of manufacturer/community register definitions, including offline upload.
- Passive observation of registers in real traffic.
- **Safe active probing** of candidate *read-only* registers only if the actual protocol supports it; authenticated, rate-limited, cancellable, and never blind address-space scanning.
- Distinct states: catalog-known, observed, confirmed readable, unsupported, unverified, unknown/undecoded.

A register's presence in a catalog must not imply support on this specific pump. Unknown registers must never become writable automatically. Provide an admin register explorer with raw/decoded values, units, timestamps, source, errors, and verification state. Reproduce discovery behavior with protocol fixtures.

## 8. HTTP API (illustrative, subject to validation)

```http
GET  /api/v1/status
GET  /api/v1/device
GET  /api/v1/device/identity
GET  /api/v1/measurements
GET  /api/v1/registers
GET  /api/v1/registers/catalog
GET  /api/v1/registers/discovered
GET  /api/v1/diagnostics
GET  /api/v1/events
POST /api/v1/discovery/scan
GET  /api/v1/discovery/status
POST /api/v1/controls/hot-water
POST /api/v1/controls/heating-offset
GET  /openapi.json
GET  /health
```

Use typed errors, documented schemas, authentication, command authorization, stale-state reporting, and live updates. Favor domain controls rather than raw register writes. Control endpoints remain disabled until physically verified.

## 9. Embedded admin interface

Responsive browser UI served directly by the Pi, covering:

- Login and initial admin setup.
- Device overview, connectivity, uptime, software/firmware and communication status.
- Live readings and available verified controls.
- Register catalog, search, verification status, raw and decoded values.
- Wi-Fi selection, connection diagnostics, device settings.
- Updates, restart/recovery, sanitized support bundle, protected debug view.

This is a maintenance interface, **not** the full household dashboard. A separate React/Chromium dashboard on an Android tablet will consume the API, potentially via a separate Linux server for history and analytics.

## 10. First-boot Wi-Fi provisioning

On first boot without valid network configuration:

1. Start a temporary AP with a unique SSID such as `NIBE-Gateway-A4F2` and unique secure setup credentials.
2. Present a captive portal or known setup URL to connected phone/computer.
3. Scan networks and accept SSID/password.
4. Attempt station-mode connection and verify success.
5. Advertise on LAN using mDNS (`nibe-gateway.local`), then finish admin/device setup.

NetworkManager owns network operations; Rust calls an abstract network interface backed by D-Bus. Consider Pi Zero WH's single Wi-Fi radio when transitioning from AP to station mode; the setup connection may drop. Make setup recoverable and network changes transactional where possible. Never automatically expose an unsecured AP on ordinary Wi-Fi outages. Require an explicit recovery/factory-reset mechanism. Keep Wi-Fi credentials protected.

## 11. Local simulation and developer experience

**First-class requirement:** Run and verify the entire service without a Pi or F730.

```bash
cargo run -- --device simulated
cargo run -- --device replay
cargo run -- --device hardware
pnpm dev    # Vite HMR with /api proxy to local Rust service
```

Modes:
- **Simulation:** deterministic synthetic measurements, commands, and simulated Wi-Fi state.
- **Replay:** recorded F730 serial frames through the real parser.
- **Hardware:** real RS485 and networking adapters.

Scenario fixtures should include normal heating, cold weather, hot-water recovery, compressor idle/running, disconnects, stale data, malformed frames, timeouts, rejected writes, first-boot setup, invalid Wi-Fi password, and network recovery. Support simulated time advance/freeze, fault injection, reset, and command history.

Development-only endpoints (e.g. `/__dev/scenario`, `/__dev/time/advance`, `/__dev/fault`, `/__dev/reset`) must be compiled out or inaccessible in production. Prefer protocol-level replay as the authoritative integration surface, with domain-level simulation for UI and workflow development.

## 12. Verification strategy

- **Unit:** frame parsing, register decoding/scaling, validation, state transitions.
- **Property-based:** frame bounds, invalid input, numeric edge cases, invariants.
- **Replay:** sanitized real F730 captures processed deterministically.
- **API contracts:** OpenAPI compatibility, auth, error semantics, command validation.
- **Browser:** Playwright flows for login, readings, register explorer, provisioning, and updates.
- **Fault injection:** serial disconnect, malformed messages, network failures, update failures.
- **Hardware:** RS485 timing, electrical integration, actual readings, approved controls, recovery.

For each important feature provide a repeatable initial state, action, observable result, and assertion. Avoid tautological tests that simply restate implementation. AI coding agents should be able to run and inspect these verification surfaces locally.

## 13. OTA updates and recovery

**Initial scope: application-level OTA**, not full OS OTA.

- Publish signed ARMv6 release artifacts with checksums, version and compatibility metadata.
- Support admin-UI upload and optional periodic release checking; default to manual approval.
- Use two persistent application slots (A/B). A separate small updater verifies, stages, activates, restarts, checks readiness, and rolls back on failure.
- Keep persistent configuration/network credentials across updates and rollbacks; design schema migrations for backward compatibility.
- Check more than just an open HTTP port: startup health and appropriate service readiness.
- Treat OS updates separately; do not promise rollback of kernel/rootfs changes without boot-level recovery testing.

Illustrative persistent layout:

```text
/var/lib/nibe-gateway/
  releases/slot-a/
  releases/slot-b/
  config/
  update-state/
```

## 14. Fast developer deployment and diagnostics

Ordinary development must never require rebuilding the image or reflashing SD:

```bash
just dev
just test
just build-pi
just deploy
just logs
just debug
just rollback
just image
```

`deploy` cross-compiles, transfers over authenticated SSH, activates a build, restarts the service, and checks health. Provide structured tracing, temporary verbose logs, serial-frame capture, communication counters, register inspection, and sanitized support bundles. Captures should be exportable for local replay. Developer mode requires explicit authorization, expires automatically, and **must not** bypass command safety or authentication.

## 15. Image and filesystem

Use **pi-gen** (https://github.com/RPi-Distro/pi-gen) for a reproducible minimal 32-bit Raspberry Pi OS Lite image containing gateway binary/static UI, systemd units, serial setup, NetworkManager, AP provisioning dependencies, mDNS, updater, and recovery tooling.

- Prefer read-only root filesystem.
- Use a small writable persistent data/config area for credentials, settings, update metadata and application slots.
- Keep ephemeral logs in memory where practical, with intentional diagnostic export.
- Verify behavior under unexpected power loss and corrupted/incomplete updates.
- Publish compressed flashable images from CI; support binary-only updates separately.

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

## 17. Proposed repository

```text
nibe-gateway/
  crates/
    core/
    protocol/
    device-f730/
    simulator/
    api/
    network/
    persistence/
    updater/
  apps/
    gateway/
    admin/
  fixtures/
    serial-captures/
    register-catalogs/
    scenarios/
  tests/
    contract/
    integration/
    e2e/
  packaging/
    systemd/
    raspberry-pi/
  scripts/
  docs/
  .github/workflows/
  Cargo.toml
  justfile
  README.md
```

This is a starting structure, not a mandate to over-modularize.

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

## 21. Definition of success

A user can flash a prepared SD image, configure the Pi through a secure temporary Wi-Fi portal, connect to a supported NIBE heat pump, inspect verified live readings and safely operate approved controls through a documented local API and admin UI. Developers can simulate, replay, test, debug, deploy, update, and roll back without reflashing. The appliance operates reliably on the Pi Zero WH without Home Assistant, Node-RED, MQTT, or cloud services.

**Product principle:** *Flash once. Configure in a browser. Develop locally. Deploy over the network. Operate independently.*
