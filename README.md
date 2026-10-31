# NIBE Gateway

A lightweight, local-first gateway for NIBE F-series heat pumps, starting with the **F730**. It is
written in Rust for a **Raspberry Pi Zero WH** (ARMv6, 512 MB) with an RS485 HAT. The aim is to
replace the legacy NibePi runtime (Node-RED/Node.js) with one daemon. That daemon provides a
versioned HTTP API, an embedded admin UI, secure Wi-Fi provisioning, signed A/B application updates,
and a MODBUS40 accessory implementation (reads, and writes of any register through the API and MQTT). It needs no cloud, MQTT or Home Assistant.

> *Flash once. Configure in a browser. Develop locally. Deploy over the network. Operate independently.*

**Status (2026-10-25): running on the owner's F730.** It runs on a Pi Zero W with Alpine Linux in A/B OS slots and a
static musl gateway (0.1.22). Polling, history, the UI and app/OS OTA are verified on hardware, and so are three named
controls (heat offset 47011, alarm reset 45171, Temporary Lux 48132). Open hardware items:
[HARDWARE_PENDING](docs/progress/HARDWARE_PENDING.md). See [Project status](#project-status).

## Quickstart (no hardware needed)

Prerequisites: Rust ≥ 1.85 (cargo), Python 3, and Node 22+ with pnpm for the UI. For `just`, the
pinned ARMv6 toolchain and the rest, run `./scripts/bootstrap-tools.sh` once and add
`~/.local/share/nibe-build-tools/bin` to `PATH` ([docs/dev/setup.md](docs/dev/setup.md)).

```bash
# 1. Build the admin UI (optional; the API works without it)
pnpm --dir apps/admin install --frozen-lockfile && pnpm --dir apps/admin build

# 2a. Simulated F730 (deterministic scenarios, simulated Wi-Fi)
cargo run -p nibe-gateway -- --device simulated
# 2b. Replay recorded protocol frames through the real parser (bundled capture is SYNTHETIC)
cargo run -p nibe-gateway -- --device replay [--capture FILE.capture.jsonl --word-swap 0|1]
# 2c. Real RS485 (bench only; the appliance runs it from /etc/nibe-gateway/hardware.json)
cargo run -p nibe-gateway --features serial -- --device hardware --serial /dev/ttyAMA0 --network simulated
```

Open <http://127.0.0.1:8080/>. There is **no default password**. First-time setup needs the
per-device setup token from the private file `<data-dir>/setup-secrets.json` (simulation:
`./.data/setup-secrets.json`), and you then choose an admin password of 12+ characters. To do the same
with curl:

```bash
token=$(python3 -c 'import json;print(json.load(open(".data/setup-secrets.json"))["setup_token"])')
curl -fsS -c /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' -H "X-Setup-Token: $token" \
  -H 'Content-Type: application/json' -d '{"password":"choose-a-long-local-password"}' \
  http://127.0.0.1:8080/api/v1/setup/admin
curl -fsS -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/measurements
curl -N    -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/events        # live SSE
curl -fsS http://127.0.0.1:8080/openapi.json                               # OpenAPI 3.1
```

For UI development, run `pnpm --dir apps/admin dev`: Vite HMR with an `/api` proxy to `127.0.0.1:8080`.
`just dev` runs the simulator with the `dev` feature, which adds the `/__dev/*` scenario, time and fault
routes. They need an admin session *and* an expiring diagnostics lease. More options:
[apps/gateway/README.md](apps/gateway/README.md).

## `just` commands

| Command | What it does |
|---|---|
| `just dev [args]` | Simulator with the `dev` feature (scenarios, time freeze/advance, faults) on loopback |
| `just test` | `cargo test --workspace --all-features` plus tooling self-tests |
| `just build-pi` | ARMv6 static musl (`arm-unknown-linux-musleabihf`; `NIBE_PI_LIBC=gnu` for the old glibc 2.28 build) release gateway, updater and system helper, with an ELF gate. `NIBE_PI_FEATURES=serial,network-manager` is the appliance default (empty explicitly builds simulation only); `dev` is rejected |
| `just deploy` | Builds the UI, signs a release, uploads over key-only SSH, activates through the updater, checks health |
| `just rollback` | Rolls back to the previous application slot over SSH and waits for `rolled_back` |
| `just logs` / `just debug` | Follow `/var/log/messages` / services, updater status and boot traces over SSH |
| `just os-bundle KEY VERSION` / `just image KEY VERSION` | Signed Alpine OS bundle (OTA) / plus the fresh-card `.img.xz` built from it; rootless Docker ([docs/dev/image-build.md](docs/dev/image-build.md)) |

Further checks: `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
`pnpm --dir apps/admin test:e2e` (Playwright, real Chromium), `tests/integration/*.sh|py`,
`python3 crates/device-f730/tests/{replay-http,serial-pty}-smoke.py`, and
`python3 docs/research/tools/nibe_ref.py selftest`.

## Architecture

Simulated, replay and hardware modes share **one application core and one set of API handlers**. Only the
adapters differ. The MODBUS40 wire task runs on its own OS thread with its own reactor, and talks to the
rest of the system only through bounded, non-blocking channels. HTTP clients, slow SSE consumers, disk
capture and updates therefore cannot delay an ACK.

```text
 RS485 HAT ─ /dev/ttyAMA0 ─┐   replay capture ─┐      simulator ─┐
                           ▼                   ▼                  ▼
            nibe-device-f730 (wire thread: decode → ACK/NAK → observe)   nibe-simulator
                           └──────────── Device trait ───────────────┘
                                              ▼
                      nibe-core: cached state + freshness, validated commands,
                                 events, diagnostics, readiness heartbeat
                                              ▼
       nibe-api: /api/v1 (auth, CSRF, typed errors, SSE, OpenAPI) + admin SPA
            │                │                     │
     nibe-network     nibe-persistence      nibe-updater (separate root daemon,
  (NM D-Bus / sim)   (atomic versioned JSON)   Unix socket, signed A/B slots)
```

| Crate / app | Role |
|---|---|
| [`crates/protocol`](crates/protocol) | NIBE accessory-bus framing: escaped LEN/checksum, `5C→C5` substitution, resynchronising bounded decoder, ACK/NAK reply policy, read request encoder and a checked write-request encoder, both refusing ambiguous `0x5C` checksums and data |
| [`crates/device-f730`](crates/device-f730) | F730 catalog (581 registers, all `write_allowed=false`), decoding/scaling/sentinels, 48852 word order, identity, passive discovery, rate-limited read probes and polling (hardware reads opt-in, ADR 0101/0102), open and gated writes (ADR 0104, 0125), replay/serial transports, capture recorder |
| [`crates/core`](crates/core) | Device-independent state cache, freshness/stale reporting, command validation and control gate, events, diagnostics, `Device`/`DeviceTransport` traits |
| [`crates/simulator`](crates/simulator) | Deterministic F730-like scenarios, simulated clock, faults, command history |
| [`crates/api`](crates/api) | Axum routes, auth (Argon2id, memory-only sessions, rate limit), CSRF guard, provisioning, maintenance (updates, diagnostics lease, sanitised support JSON), OpenAPI |
| [`crates/network`](crates/network) | Provisioning state machine; simulated backend and NetworkManager (zbus) backend behind a feature; Avahi service file |
| [`crates/persistence`](crates/persistence) | Locked 0700 data dir, 0600 secrets, fsync + rename writes, schema migration |
| [`crates/updater`](crates/updater) | `nibe-updater` (root daemon: verify Ed25519 + SHA-256, stage inactive slot, atomic symlink switch, readiness check, rollback, crash recovery) and `nibe-release` (keygen/sign) |
| [`apps/gateway`](apps/gateway) | The `nibe-gateway` binary: CLI/JSON config, mode wiring, static UI |
| [`apps/admin`](apps/admin) | React + TypeScript + Vite admin UI (only React as a runtime dependency), typed client generated from OpenAPI |
| [`fixtures/`](fixtures) | F730 register catalog, synthetic protocol vectors/captures, simulator scenarios |
| [`tests/`](tests) | HTTP contract + OpenAPI snapshots, scenario integration, curl/Python end-to-end smokes, Playwright runner |
| [`packaging/`](packaging) | Alpine appliance (`alpine/`: package allowlist, OpenRC services, initramfs, kernel/firmware/Tailscale pins, bundle and card builders), files shared with the retired Raspberry Pi OS image (`common/`), NibePi fallback (`nibepi/`, retired with NibePi, HP-24) |
| [`scripts/`](scripts) | Toolchain bootstrap, Pi build/ELF gate, QEMU smokes, image build, SSH deploy/rollback/diagnostics |

## Safety stance

- **Accessory, not master.** The gateway emulates the MODBUS40 accessory at address `0x20`. It ACKs
  every frame addressed to it, answers read tokens with bounded read requests only when
  `allow_hardware_reads` is set (ADR 0101/0102), and stays silent for every other address (including
  RMU40). It never transmits unsolicited.
- **Any register by number, if you ask for it.** On hardware, the HTTP API and MQTT (with commands enabled)
  read and write any register number with any value; the gateway does not check ranges, so you decide what is
  safe (ADR 0125, 0126). Turn this off with `open_writes: false` to keep only the named controls (heat offset
  47011, Temporary Lux 48132, alarm reset 45171 as a 0 → 1 edge), which were each verified on the F730 and need
  `allow_hardware_writes` with an explicit `writable_registers` list (ADR 0104, 0105, 0113). Every write is
  sent once, never retried, and read back. The gateway itself never writes 48852.
- **Control lease (heat offset only).** A heating-offset write may carry `lease_seconds` (60–3600). If the
  writer does not write again with a lease before it lapses, the gateway writes back the offset read before the
  first leased write, through the same gated control, also after a restart; a restore that fails transiently is
  re-sent every 30 s. A write without a lease ends the lease. Lease state is in `GET /api/v1/status`
  (`heating_offset_lease`). This is the only gateway-initiated write besides the alarm-251 reset (ADR 0123, 0105).
- **No blind probing.** Active read probes allow only explicit catalog IDs that are safe to encode (max 32,
  ≥ 1 s apart, cancellable, authenticated).
- **Serial before everything.** The wire thread is isolated. Observation/capture overflow is counted
  and dropped instead of blocking replies.
- **Security.** No default password. Setup uses a per-device token that is never served or logged.
  Argon2id, HttpOnly SameSite=Strict sessions, a CSRF header on every POST, and authentication for every
  data, update and debug route. Debug and capture export also need an expiring lease (≤ 15 min). The
  support bundle is built from an allowlist of numeric/enum fields. `/__dev` routes are compiled out
  of release builds, and `build-pi.sh` refuses the `dev` feature. Updates are Ed25519-signed and applied
  by a separate privileged updater.
- **Recoverable updates.** NibePi has been replaced; its card now holds the appliance image. Recovery is
  app A/B slots with probation and rollback, OS A/B slots with a one-shot tryboot trial and health-checked
  commit (ADR 0106, 0117/0118), and a fresh-card image (`just image`). The phased NibePi migration
  ([ADR 0012](docs/decisions/0012-ws01-nibepi-migration-strategy.md)) is complete.

## Project status

| Milestone | Status |
|---|---|
| M0 Research & feasibility | done |
| M1 Local foundation | done |
| M2 Protocol & register system | done (real F730 captures, 576-register survey) |
| M3 Provisioning & administration | done-hw (setup-AP onboarding still only simulated) |
| M4 Pi appliance | done-hw (Alpine A/B on the owner's Pi; fresh-card image built, not yet flashed) |
| M5 OTA & dev workflow | done-hw (failure-path and power-cut tests deferred) |
| M6 Real F730 integration | done-hw (polling, history, three verified controls; MQTT tested against a Docker broker only) |
| M7 Stabilization & release | in-progress (power-cut matrix, long soak and release publishing pending) |

Details, open findings and "what's left": [PROGRESS.md](PROGRESS.md) ·
[WS08 integration review](docs/progress/WS08-integration-review.md).

## Documentation

- Brief (requirements), split by topic: [docs/brief/00-index.md](docs/brief/00-index.md)
- Progress and workstreams: [PROGRESS.md](PROGRESS.md), [docs/progress/](docs/progress/)
- **Open hardware items** (summary at the top): [docs/progress/HARDWARE_PENDING.md](docs/progress/HARDWARE_PENDING.md);
  the original first-session plan (mostly historical now): [docs/hardware-test-plan.md](docs/hardware-test-plan.md)
- Protocol research: [docs/research/](docs/research/README.md), especially
  [protocol-summary-for-implementers.md](docs/research/protocol-summary-for-implementers.md)
- Decisions (ADRs): [docs/decisions/](docs/decisions/)
- Developer docs: [setup](docs/dev/setup.md), [cross-compile](docs/dev/cross-compile.md),
  [image build](docs/dev/image-build.md), [OTA & diagnostics](docs/dev/ota-and-diagnostics.md)
- Rules for coding agents: [AGENTS.md](AGENTS.md)

## License and disclaimer

MIT, see [LICENSE](LICENSE). The F730 register catalog text derives from NIBE ModbusManager data via NibePi (MIT);
attribution is in [fixtures/register-catalogs/README.md](fixtures/register-catalogs/README.md)
([ADR 0010](docs/decisions/0010-ws01-register-catalog-sources.md)).

This is an independent hobby project. It is not affiliated with or endorsed by NIBE Energy Systems. It talks
directly to your heat pump; on hardware it can write any register you tell it to (ADR 0125), and you use it at your own risk.
Report security problems privately through GitHub's *Report a vulnerability* button rather than a public issue.
