# WS02 — Core, simulator, API, gateway app

- **Milestone:** M1
- **Status:** done-hw (runs the owner's F730 since 2026-10-24; leftovers in HARDWARE_PENDING); heating-offset lease, crash-safe supersession and pre-dispatch watermark fixes done-sim 2026-10-29 (HP-42)
- **Owns:** Cargo.toml (workspace), crates/core, crates/simulator, crates/api, apps/gateway, fixtures/scenarios, tests/contract, tests/integration
- **Brief:** [03-stack-and-architecture.md](../brief/03-stack-and-architecture.md), [06-http-api.md](../brief/06-http-api.md), [09-simulation-and-devex.md](../brief/09-simulation-and-devex.md), [10-verification.md](../brief/10-verification.md), [14-security.md](../brief/14-security.md)

## Checklist
- [x] Cargo workspace; minimal deps (tokio, axum, serde, utoipa or similar for OpenAPI, tracing)
- [x] core: device state cache with timestamps/freshness, measurement model, command model with validation, events bus, diagnostics counters, DeviceTransport/Device trait boundaries
- [x] simulator: deterministic simulated F730 (scenarios: normal heating, cold weather, hot-water recovery, compressor idle/running, disconnect, stale, malformed, timeout, rejected write, first-boot, bad wifi pw, net recovery), simulated clock (advance/freeze), fault injection, command history
- [x] api: /api/v1/* endpoints from brief §8, typed errors, stale-state reporting, SSE /api/v1/events, /openapi.json, /health; controls return 'disabled until verified' unless enabled
- [x] /__dev/* endpoints behind cargo feature `dev` (compiled out by default in release)
- [x] apps/gateway: CLI `--device simulated|replay|hardware`, config, serves static admin dir if present
- [x] Serial/protocol task isolated from HTTP (bounded channels, never blocked by slow clients) — documented + tested
- [x] Contract tests over real HTTP; OpenAPI snapshot test; scenario-based integration tests
- [x] Heating-offset control lease (ADR 0123): `lease_seconds` on the write, restore of the read-back baseline on lapse and at startup, state in `/api/v1/status` and the Controls page (done-sim; hardware HP-42)
- [x] Setpoint leases (ADR 0127): `lease_seconds` on register writes to 47045/47049/47375 through the same coordinator as the 47011 lease, over HTTP and MQTT, state in `/api/v1/status` `register_leases` (done-sim; hardware HP-45)

## Verify
Run from the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
./tests/integration/http-smoke.sh default
./tests/integration/http-smoke.sh dev
cargo run -p nibe-gateway -- --device simulated
# In another terminal:
curl -fsS http://127.0.0.1:8080/api/v1/device
curl -fsS http://127.0.0.1:8080/openapi.json
curl -N http://127.0.0.1:8080/api/v1/events
```

The automated suites each pass 16 tests. Both smoke modes start the real binary,
use curl on an ephemeral port, validate static SPA/API boundaries, and stop via
SIGTERM with an open SSE stream. Contract tests also use real TCP. OpenAPI
snapshots are checked for default and dev builds; regeneration commands are in
`tests/contract/README.md`. `apps/gateway/README.md` documents configuration,
scenario/fault JSON bodies, safety gates and the adapter/auth insertion points.

## Open issues / hardware-pending
- WS04 still owns protocol parsing, replay, hardware transport and real F730 catalog.
  `--device replay|hardware` deliberately fails until those adapters are installed;
  no silent synthetic fallback. Application isolation is verified; physical
  wire/ACK timing and measurements need WS04 + hardware.
- WS06 owns production authentication/network operations/persistence. The current
  executable refuses non-loopback binding and injects an explicit local-development
  authorizer. Network scenarios represent domain state only; `network_recovery`
  stays `recovering` until a new scenario is selected.
- WS07 updater remains an explicit construction/workspace extension point.
- Hardware/replay writes remain disabled in every feature/config combination.
  Synthetic controls require `dev` plus `--allow-simulated-controls`; fixture IDs
  1–5 and offset/boost limits are not claims about F730 hardware.
- Pi resource limits and physical read/ACK correctness have exact deferred steps
  appended to `HARDWARE_PENDING.md`. No physical hardware was available.

## Log

- 2026-10-22: Implemented the four-member Cargo workspace and generated Cargo.lock
  (necessary root build artifact outside the listed source directories). Added
  generic catalog/readings/freshness, validated domain commands, diagnostics,
  bounded request/history/event queues, cancellable adapter deadlines, graceful
  shutdown and Device/DeviceTransport/Clock interfaces. Added 13 checked-in
  synthetic scenarios, frozen/advanced time, faults, recovery, reset, accepted
  command history and timed boost expiry. Implemented all illustrative v1 routes,
  passive-only discovery, typed errors, SSE with resync, generated OpenAPI and
  explicit Authorizer capability injection. Added CLI/JSON configuration,
  loopback-only policy, optional static SPA hosting and clear future WS04/06/07
  construction points. Default builds exclude dev routes and schemas. ADRs
  `0020-ws02-bounded-core-and-adapter-contract.md` and
  `0021-ws02-local-simulation-and-control-policy.md` record decisions.
- 2026-10-22: Verified default/all-feature tests (16 each), strict all-target clippy,
  default clippy, formatting and executable curl smoke in default/dev modes.
  Integration caught and corrected SPA fallback HTTP status and exercised
  shutdown with open SSE. Tests cover unknown register preservation, hardware
  control refusal, queue saturation, adapter timeouts, stale/fault recovery,
  authorization injection, invalid commands and scenario responses. Only shared
  tracking files, mandated ADRs and the generated lockfile were touched outside
  owned source/test paths. No commits; concurrent WS01/WS03 work preserved.
- 2026-10-25 (bookkeeping): scope grew on hardware: history store and `/api/v1/history` (ADR 0114), MQTT bridge with Home Assistant discovery (ADR 0115; Docker broker only), API keys (ADR 0110), named controls temporary-lux and the alarm-reset 0 → 1 edge (HP-31/32), and the gateway clock following the boot-time NTP step (0.1.22, HP-39 passed). Live on Alpine (HP-38). Open: MQTT against a real broker, history numbers (HP-33).
- 2026-10-29: **Heating-offset control lease** (ADR 0123, owner decision; safety net for the "Neo" planner).
  `POST /api/v1/controls/heating-offset` takes an optional `lease_seconds` (60–3600). The first leased write records
  the fresh read-back of the offset as the baseline (503 if there is none); renewals (leased writes before expiry)
  keep it and move the expiry. When a lease lapses, `crates/api/src/lease.rs` writes the baseline through
  `Gateway::command` (same ADR 0104 gates); transient failures are re-sent every 30 s, permanent ones are recorded
  as a failed restore. A write without a lease (API, or MQTT seen in the command history) ends the lease without
  a restore. State persists in `control-lease.json` in the state dir (kept on factory reset); at startup a lapsed
  lease is restored at once and an active one is clamped to at most 3600 s from now (clock before NTP). Holder is
  the API key name or `admin` (new `Authorizer::principal`). `GET /api/v1/status` has `heating_offset_lease`
  (lease, `remaining_s`, `last_restore`); SSE event kind `lease`; the Controls page shows a lease line.
  Touched outside WS02's paths (minimal): `crates/persistence` (lease file), `apps/admin` (Controls lease line,
  `lease` event, regenerated `schema.ts`), README safety section, ADR 0113 amendment.
  Verify: `cargo test --workspace`, `cargo test --workspace --all-features` (lease tests need `dev`:
  `cargo test -p nibe-api --features dev lease`), `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo fmt --all -- --check`. Simulator proof (real clock, curl, API key `neo`, kill -9 restart): lapse restored
  −2 after renewal; restart clamped and later restored. Open: hardware test HP-42; restore `ok` means the write was
  accepted/queued, the read-back is the confirmation.
- 2026-10-29: **Lease review hardening (done-sim; HP-42 remains open).** All heating-offset callers,
  including MQTT's Gateway command path, now enter the same coordinator operation lock. Durable lease intent
  and owner cancellation precede dispatch; save failures return Unavailable and restore the previous in-memory
  intent. An expired lease retains its baseline while a restore is queued, until an accepted terminal outcome
  and a strictly later fresh read-back confirm it. Transient failures retry after 30 s; permanent dispatch
  failures retain the baseline and report failure. Owner writes cancel pending restoration. New leases require
  confirmed readings after the preceding write; renewals during pending restore keep the original baseline.
  Removed command-history inference, so renewals cannot hide owner writes. No new dependencies/API schema.
  Added five regression tests: queued outcome/read-back ordering, restore-pending baseline retention,
  owner supersession/new-baseline gating, shared-lock MQTT dispatch, and storage-failure refusal. Updated two
  fake-clock startup tests and HTTP contract timing for strictly post-completion observations. Initial focused
  runs exposed old fake-clock assumptions and receipt-detail regressions; corrected. Not every added regression
  test was run failing-first against the original implementation.
  Minimal cross-workstream change: WS04 runtime exposes terminal offset outcome through the core Device seam;
  ADR 0123/index and shared progress/hardware records amended. Existing uncommitted work preserved; no commits.
  Verified: `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace`;
  `cargo test --workspace --all-features`; `cargo test -p nibe-api --features dev lease` (9 unit + 3 HTTP);
  `./scripts/check-tooling.sh` (65 tests). OpenAPI snapshots pass unchanged by this hardening.
  `timeout 8s cargo run -p nibe-gateway -- --device simulated` reached the listening state (expected exit 124).
  Open: HP-42 physical pump/SD tests; see appended hardware procedure. No hardware or deployment performed.

- 2026-10-29: **Three lease review findings fixed (done-sim).** Definite pre-queue rejection
  (Busy/Validation/ControlsDisabled) durably restores the entire previous lease intent, including pending
  restoration, and leaves the freshness watermark unchanged. Timeout/Unavailable and other ambiguous failures
  retain conservative cancellation and completion gating. Dedicated in-memory offset outcome follows every
  offset state transition, independently of other controls; queued/sent attempts invalidate older completion.
  Minimal cross-workstream edits: WS04 runtime and write engine (one retained outcome, no dependency or wire/API
  schema change). ADR 0123/index amended; precise physical checks appended under HP-42. No commits.
  Failing-first proof: `cargo test -p nibe-api --features dev lease_definite_rejection` failed both new tests
  (lost persisted lease; missing-completion 503), and
  `cargo test -p nibe-device-f730 offset_completion_survives_other_controls` failed with None instead of the
  accepted offset outcome. All three passed after fixes; the latter also covers automatic alarm reset and
  invalidation by a new offset attempt. Open: HP-42 physical pump/power-cut acceptance.
  Verification passed: `cargo fmt --check`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo test --workspace`; `cargo test --workspace --all-features`;
  `cargo test -p nibe-api --features dev lease` (11 unit + 3 HTTP);
  `./scripts/check-tooling.sh` (65 tests). OpenAPI snapshots passed unchanged; no regeneration.
  `timeout 8s cargo run -p nibe-gateway -- --device simulated` reached the listening state (expected exit 124).

### 2026-10-29 — remaining lease review findings P1/P2 (done-sim)
- [x] P1: retain the lease with durable pending owner offset/attempt time before dispatch; accepted/ambiguous
  outcomes clear it, definite rejection rolls back. Startup waits for a fresh reading: matching owner offset
  cancels, otherwise retains the baseline and normal expiry. No uncertain owner write is replayed.
- [x] P2: capture the write watermark before dispatch; definite rejections still roll it back.
- Both new regression tests were actually run failing before the fix: crash protection assertion failed;
  completion-before-resume returned Unavailable despite a post-completion reading. Both pass after the fix.
  Crash tests reload captured on-disk state into a fresh Store/coordinator, test cached-reading refusal,
  matching/nonmatching pump values, and expiry restoration. Busy and accepted dispatch boundaries covered.
- Minimal cross-workstream change: optional backward-compatible marker in crates/persistence; refreshed the
  two OpenAPI contract snapshots. ADR 0123 amended; HP-42 appended. No dependencies, commits or pump writes.
- Verification (all passed): `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace`;
  `cargo test --workspace --all-features`; `cargo test -p nibe-api --features dev lease`;
  `scripts/check-tooling.sh` (65 tests). Initial workspace runs caught the added schema field; regenerated
  with `UPDATE_OPENAPI=1 cargo test -p nibe-api openapi_snapshot` and the same with `--all-features`, then
  reran both workspace suites. `timeout 8s cargo run -p nibe-gateway -- --device simulated` reached listening
  on 127.0.0.1:8080 (intentional timeout). Physical Pi/F730 crash/power-cut checks remain HP-42.

### 2026-10-29 — fourth lease review: owner never locked out (done-sim)
- Review round 4 found no P0/P1. P2 fixed: after a restart with an unresolved owner-supersession marker
  and no fresh 47011 reading, every offset write returned 503, including an explicit owner override.
  Now only leased writes (renewals/new leases) wait for reconciliation; an unleased owner write replaces
  the marker with its own request and proceeds as before.
- Regression test `lease_unresolved_supersession_never_blocks_a_new_owner_write` ran failing first
  (owner write Unavailable), passes after the fix.
- Known, accepted (ADR 0123 policy): if the owner requested the value already leased, a crash before
  dispatch reconciles as "owner write landed" and ends protection; the pump already holds the owner's value.
- Verification (all passed): `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`
  (default and `--all-features`); `cargo test --workspace --all-features` (149); `cargo test -p nibe-api
  --features dev lease`; `scripts/check-tooling.sh`.

### 2026-10-29 — 0.1.25 deployed; HP-42 steps 1–6 verified on the F730
- First OTA attempt rolled back: `scripts/deploy-release.sh` was run without `just build-pi`, so it signed the stale
  0.1.24 binary as 0.1.25 and readiness never matched the version. Rebuilt with `./scripts/build-pi.sh`, checked
  `/health` 0.1.25 on the Pi in simulated mode, redeployed: activated, probation complete, slot b, 0.1.24 kept.
- HP-42 steps 1–6 passed on the live pump (details under HP-42 in HARDWARE_PENDING.md). Pump left at −2.
- Open: `nibe-release sign` / `deploy-release.sh` do not check the binary's version against the manifest version;
  use `just deploy` (builds first) or add that guard.

### 2026-10-31 — Open register writes (ADR 0125, owner decision)
- Owner asked for every register to be writable through the API by default, fully open (no allowlist, catalog
  flags or range). New `POST /api/v1/controls/register-write` with `{"register", "raw"}` or `{"register", "value"}`
  (value × catalog scale, rounded). Control capability (admin session or control-scoped key). 47011 is routed
  through the heating-offset control so ADR 0123 lease semantics hold (-10..10).
- Config `open_writes` (default true, CLI `--no-open-writes`); active only with `--device hardware` and
  `--allow-hardware-reads`. When active it also enables the named controls and their MQTT entities.
- WS04-owned files touched (minimal): `crates/device-f730/src/writes.rs` (`WriteEngine::with_policy`, `open`
  status flag) and `runtime.rs` (`AdapterConfig::open_writes`). Encoding safety (0x5C bytes) still rejects a write.
- AGENTS.md rule 6, brief 04, ADR 0104 (amendment) and the ADR index updated; ADR 0125 added.
- Verify: `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
  `UPDATE_OPENAPI` snapshots regenerated for default and `dev`.
- Open: HP-44 (first live open write). Simulated and replay mode do not implement it (501 / 403). The admin UI has
  no form for it.

### 2026-10-31 — Register read/write by number on HTTP and MQTT (ADR 0126, owner decision)
- Owner wants the API and MQTT equally able to read and write any register number (no NibePi 1:1 layer).
- Shared implementation `crates/api/src/registers.rs` (`Registers`): `write` (raw or catalog-scaled, 47011 via
  the heating-offset control) and `read` (queues one read of any encodable register number and waits up to 20 s
  for the answer). HTTP: `register-write`, new `register-read` (returns the reading). MQTT (behind
  `allow_commands`): `<base>/register/<id>/set`, `/set_raw`, `/get`; `get` publishes the value on
  `<base>/register/<id>` (not retained); outcomes on `<base>/result`.
- WS04-owned files touched (minimal): `discovery.rs` `ProbeEngine::read_once` (any encodable ID, hardware reads
  required) and `runtime.rs` `F730Handle::read_once`. `nibe_mqtt::start` takes an optional `RegisterAccess`.
- README safety stance, gateway README, AGENTS.md rule 6, ADR 0115 amendment, ADR 0126 and index updated.
- Verify: `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo test --workspace --all-features`; OpenAPI snapshots regenerated (default and `dev`).
- Open: no MQTT broker test harness exists, so the MQTT topics are covered by parse/topic unit tests only
  (HP-44 step 6). One-off reads are not in `/api/v1/registers` or history by design (poll list = live).

### 2026-10-31 — Poll choices from the UI, API and MQTT (ADR 0126 amendment)
- Per-register poll mode fast / slow / off: `Registers::set_poll` (shared), `POST /api/v1/controls/register-poll`,
  `GET /api/v1/registers/polls`, MQTT `<base>/register/<id>/poll`, and a Polling column on the Register explorer
  (hardware/replay only). Saved as `poll_overrides` (`{"<id>": "fast|slow|off"}`, deviations only) in the
  settings file `config.json`; applied on top of `hardware.json` at start (`restore_polls`).
- WS04 files (minimal): `ProbeEngine::replace_polls`, `F730Handle::set_polls`. WS05 files: `Registers.tsx`,
  `Integrations.tsx`, `api.ts`, regenerated `schema.ts`, `table.css` (`cell-select`). WS06: `PollMode`,
  `PollOverrides` in persistence.
- MQTT discovery now re-announces when registers get their first value (slow polls, newly polled registers).
- Verify: `cargo test --workspace --all-features`; `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`; `pnpm --dir apps/admin build`; `node tests/e2e/run.mjs` (14 passed). Contract test
  `poll_choices_apply_at_once_and_are_saved_in_the_settings_file` runs against the replay adapter.


### 2026-10-31 — Setpoint leases (ADR 0127), one coordinator for every lease
- Owner request (Neo planner migrating from NibePi): lease 47045, 47049 and 47375 like 47011; 43005 (degree minutes)
  explicitly excluded by the owner.
- First built as a second coordinator beside the offset lease; on the owner's review merged into one, so the ADR
  0123 rules exist once. `crates/api/src/lease.rs` `Leases` now keys leases by register and talks to each register
  through a `LeaseChannel` (validate, write, read after a time, last completion): `OffsetChannel` (47011 through
  `dispatch_command`, so ADR 0104 gates, simulation, command history and the MQTT `HeatingOffsetWriter` hook are
  unchanged) and `SetpointChannel` in `registers.rs` (shared register write; reads use the write engine's
  automatic read-back, else one read). `Registers::new` adds the setpoint channels; `Registers::write` sends
  every leasable register to `Leases`, so MQTT can lease 47011 too. `lease_seconds` elsewhere is 422.
- Rules unchanged from ADR 0123 and its amendments for every register: baseline from a reading after our last
  write completed, renewals keep it, unleased writes end it (durable pending marker, decided by the next reading
  after a restart), definite rejections roll back, restore confirmed by an accepted outcome plus a later reading,
  restores re-sent every 30 s otherwise, non-transient dispatch errors reported (`ok: false`).
- Persistence: still `control-lease.json`. 47011 stays in `heating_offset`/`last_restore` (a 0.1.29 rollback still
  restores it); other registers in new `registers`/`restores` maps, ignored by older releases
  (`LeaseState::{by_register, from_registers}`; test `heating_offset_lease_stays_readable_by_releases_before_setpoint_leases`).
- HTTP: `lease_seconds` on `register-write`; `register_leases` (every lease, `leasable`, `restores`) in
  `/api/v1/status`, `heating_offset_lease` unchanged for the Controls page. MQTT: `set`/`set_raw` accept a number
  or `{"value": n, "lease_seconds": s}`; holder `mqtt`.
- WS04 file touched (minimal): `writes.rs` keeps the latest outcome per register (`WriteStatus::last_for`). WS06:
  `RegisterLease`, `RegisterLeaseRestore`, `LeaseState` maps. WS05: regenerated `schema.ts` only.
- Tests: the existing 12 offset-lease tests pass unchanged apart from the call name (`write_offset`); one new test
  runs a setpoint lease through its own channel beside an offset lease; persistence compatibility test; MQTT
  payload parsing.
- Verify: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo test --workspace` and `cargo test --workspace --all-features`; OpenAPI snapshots regenerated with
  `UPDATE_OPENAPI=1`; simulator starts and `/health` is 200.
- Open: HP-45 including a 47011 regression run (HP-42 steps 1–6), since the offset lease now runs on the merged
  coordinator; no admin UI for setpoint leases (status JSON only); admin `tsc` not run here (no `node_modules`).
