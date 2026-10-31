# Tech stack, architecture boundaries, repository layout

> Source: ORIGINAL_BRIEF.md §4, §5, §17

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
