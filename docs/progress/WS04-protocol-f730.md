# WS04 — Protocol, F730 adapter, replay, register catalog & discovery

- **Milestone:** M2
- **Status:** done-hw (reads, polling and named writes on the owner's F730)
- **Owns:** crates/protocol, crates/device-f730, replay transport, fixtures/serial-captures, fixtures/register-catalogs (shared with WS01)
- **Brief:** [04-protocol-and-safety.md](../brief/04-protocol-and-safety.md), [05-register-catalog-and-discovery.md](../brief/05-register-catalog-and-discovery.md), [09-simulation-and-devex.md](../brief/09-simulation-and-devex.md), [10-verification.md](../brief/10-verification.md)

## Checklist
- [x] protocol: frame decoder/encoder per docs/research/protocol.md, checksum, ACK/NAK generation, streaming parser resilient to garbage, proptest
- [x] Serial transport trait impls: real (tokio-serial behind feature) and replay (from capture files), capture recorder/exporter
- [x] device-f730: register catalog loader (versioned JSON), decoding/scaling/units, identity detection, capability model
- [x] Discovery: passive observation, register states (catalog-known/observed/confirmed-readable/unsupported/unverified/unknown), safe read-probing framework (auth, rate-limit, cancellable, catalog-only, no blind scan) — disabled for hardware until verified
- [x] Wire into gateway `--device replay` and `--device hardware`; register explorer endpoints
- [x] Replay tests: fixture frames → expected decoded device state

## Verify

From the repository root:

```bash
python3 docs/research/tools/nibe_ref.py selftest
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 crates/device-f730/tests/replay-http-smoke.py
python3 crates/device-f730/tests/serial-pty-smoke.py
cargo run -p nibe-gateway -- --device replay
# Set up/login through the WS06 flow (apps/gateway/README.md), then:
curl -fsS -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/device
curl -fsS -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/registers/explorer
curl -fsS -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/device/capabilities
```

The replay curl script starts **cargo run -p nibe-gateway -- --device replay**
with an ephemeral bind/data directory, bootstraps a real administrator session,
compares all 17 values against `synthetic-f730-session.expected.json`, checks
581 explorer entries, auth/CSRF, forbidden/ambiguous probes, busy/cancel behavior,
read-only controls/no raw-write route, validated full offline catalog upload and
capture completion after SIGTERM. The PTY script uses the actual `serial` feature
in hardware mode against a local synthetic Linux terminal: split frames,
48852=0 followed by a high-word-first read response, scaling, ACK/NAK, RMU
silence and hardware probe/write refusal. Neither script accesses physical
hardware or NetworkManager. Fixture identity, values and all timing remain
synthetic; capture tests do not claim the owner's pump was observed.

## Open issues / hardware-pending

- Follow WS01 HW-1..HW-5 and the appended WS04 handoff in
  HARDWARE_PENDING.md. No real F730 captures/physical UART measurements exist.
- Hardware active probing is intentionally rejected; physical validation and an
  approved verified policy are prerequisites for enabling it. 48852 can be
  passively observed or supplied with a menu-verified `--word-swap` assumption;
  it is never written. Unknown 32-bit order and conflicting 0x68/word-swap=0
  values remain raw with explicit errors. HW-4 must resolve these.
- The 10 ms reply target measures kernel submission, not physical first-bit
  latency. Fatal serial/reply failures require service restart; no speculative
  automatic reopen. Capture is best-effort, with live and sidecar loss counters.
- Imports/observation verification are process-local. `--catalog` supports
  repeatable offline loading; persistent installation verification overlays
  remain future work. Long replay files must be split at 8 MiB/65,536 records.

## Log

- 2026-10-22: Added `nibe-protocol` and `nibe-device-f730`, preserving WS01
  fixtures/catalog unchanged. Implemented escaped-byte checksums/lengths,
  checksum substitution, bounded resynchronizing stream parser, safe read-only
  encoder and property tests. Added all catalog numeric types/divisors/units,
  source/conflict preservation, sentinel/raw reporting, product identity,
  48852-dependent read response decoding and conservative 32-bit data behavior.
  Added bounded passive observations and explicit read-probe authorization,
  allow-list, spacing, correlation, timeouts/retries and cancellation; hardware
  probes and every write remain blocked. Added replay/real serial transports,
  private opt-in capture export/summary/drain and dedicated UART OS thread/reactor
  with bounded channels and independent ACK target handling.
- 2026-10-22: Small additive shared changes: workspace/dependencies, gateway CLI
  adapter construction, startup before slow radio initialization (WS06
  coordination), mode-specific API module and optional insertion inside WS06's
  production provisioning/session/CSRF guard. Retained simulated mode/auth/
  network/persistence wiring and all existing core API contracts. WS06 refreshed
  both intentional OpenAPI snapshots and adjusted shared mode tests.  No commits.
- 2026-10-22: Verified published/reference bytes, complete fixture values/replies/
  counters, property tests, unknown/sentinel/malformed/identity cases, rate limits,
  cancellation and hardware refusal. ACK tests cover saturated observation
  channels, blocked application executor and a stalled send deadline. Real curl
  replay and feature-gated serial PTY scripts pass with production admin auth.
  Workspace default/all-feature tests, formatting and strict all-target
  all-feature clippy pass. Hardware-only follow-up appended without duplicating
  WS01 HW-1..HW-5.
- 2026-10-22 (final shared-checkout recheck): WS04 tests/strict clippy and both
  real curl scripts remain passing. The earlier full workspace default/all-feature
  and clippy checkpoints passed before concurrent WS07 integration. After the
  new updater/maintenance routes landed, latest workspace checks are temporarily
  blocked by WS07's OpenAPI snapshot changes, `production_build_has_no_debug_routes_or_schemas`
  expectations, and `crates/updater/tests/faults.rs` unchecked read result
  (`clippy::unused_io_amount`). Waited/retried rather than editing WS07 code.
  Latest workspace formatting check passes. WS07 owns completing its integration
  and rerunning global tests/clippy; these are not WS04 protocol/adapter failures.
- 2026-10-25 (bookkeeping): register survey of all 576 readable registers (`docs/research/register-survey-2026-10-25.md`) and display-photo verification of units and scales (`docs/research/panel-verification-2026-10-25.md`, HP-34); two-speed polling (0.1.12); 32-bit values read before 48852 decoded once the order is known (0.1.13); alarm reset as a 0 → 1 edge on 45171 (0.1.8, HP-31); 48132 Temporary Lux write (HP-32). Open: HP-34 leftovers (43147, energy counters, fan modes), HP-07/21 timing (nice to have).
- 2026-10-26 (register notes): catalog corrections 43108 unit `%` → null (active fan speed step 0 = normal,
  1–4 = temporary speed) and 40051 scale ÷100 → ÷10, in `fixtures/register-catalogs/f730.json` and the generator's
  `UNIT_SCALE_OVERRIDES`, each with a note citing `docs/research/panel-verification-2026-10-25.md` (conflict records
  kept, as for 43141). New per-installation register notes (`register-notes.json`, `PUT /api/v1/registers/notes`
  admin only, `register_notes` in the explorer) and the owner seed `scripts/register-notes-f730-owner.py`.
  Verify: `cargo test --workspace`, `python3 -m unittest discover -s scripts/tests`, `cd apps/admin && npm run
  test:e2e`. Open: apply the seed on the Pi (HARDWARE_PENDING HP-41).
