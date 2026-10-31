# WS08 — Integration review, hardware test plan, docs

- **Milestone:** M6/M7
- **Status:** done (review, fixes and docs complete. M6 stays blocked on hardware; open findings are listed below)
- **Owns:** docs/, README.md, tests/ (gap fills), HARDWARE_PENDING.md curation
- **Brief:** [15-milestones.md](../brief/15-milestones.md), [10-verification.md](../brief/10-verification.md), [16-future-and-agent-rules.md](../brief/16-future-and-agent-rules.md), [01-vision-and-goals.md](../brief/01-vision-and-goals.md)

## Checklist
- [x] Full review against the brief: gaps, safety violations, security issues. Small issues fixed; larger ones filed (findings table below)
- [x] Verify end-to-end locally: build the UI, run the gateway simulated and in replay, real browser check, API via curl
- [x] docs/hardware-test-plan.md: ordered, safe procedure for the first physical session (read-only first, NibePi rollback)
- [x] README.md: quickstart, modes, commands, architecture
- [x] Update PROGRESS.md milestones accurately (plus a "What's left" section)
- [x] HARDWARE_PENDING.md deduplicated and ordered (HP-01…HP-20, mapped to plan steps)

## Review scope and method

I read the code, not the progress files: protocol, device-f730 (runtime, decode, discovery, transport),
core service, api (lib, auth, provisioning, maintenance, f730, dev), updater (lib, artifact, rpc, service,
daemon), persistence, the network state machine, the gateway main/config, systemd units, the supervisor,
the build/deploy scripts and CI. I checked them against brief §6 (safety), §16 (security), the
implementer protocol summary and §13 (OTA). Adversarial checks:

- **Release build** (`cargo build --release`, no `dev`): `/__dev/*` returns 404 with and without a
  session. The OpenAPI document has no dev, raw or write paths. `/api/v1/*` data, events, support,
  capture, updates, restart, factory-reset, recovery, discovery and controls return 401 without a session.
  `/health`, `/openapi.json`, `/api/v1/setup/status` and the SPA are public.
- **Auth and CSRF:** wrong setup token → 401; POST without `X-Requested-With` → 403; cross-site `Origin`
  → 403; password shorter than 12 characters → 422; repeat setup and post-setup setup-token use → 401;
  duplicate/garbage cookies → 401; 100 KB body → 422; encoded/dot-segment/double-slash paths reach only the
  SPA shell or 404, never API data. Capture export with a session but no lease → 403; lease > 900 s → 422.
  Controls → `controls_disabled` (simulated default, replay).
- **Secrets:** support JSON checked against admin-password, setup-token, AP-password, SSID, Wi-Fi
  password and session-cookie canaries (none present); the gateway log had none either. Files are
  0700/0600. No `tracing` call logs request bodies or secrets.
- **Parser:** 20 000-case proptest runs; 3 000 random noisy chunks through `--device replay`; a
  non-JSON capture is rejected cleanly. Found and fixed **F1** below.
- **OTA:** traced every journal phase through stage, activate, readiness, rollback and recovery,
  including the interaction with config schema migration (**F6**) and the absence of a probation window (**F7**).

## Findings (prioritised)

| # | Severity | Status | Location | Description | Suggested fix |
|---|---|---|---|---|---|
| F1 | High | **fixed** | `crates/protocol/src/lib.rs` (`Decoder::parse` rescue) | While a long frame was still incomplete, the "rescue" search accepted a `5C` that was the *second byte of an escape pair*. A legitimate data message containing value `0x005C` followed by a register whose low byte is `0x20` could then appear as a complete, checksum-valid nested frame for address `0x20`. The decoder emitted that phantom frame, so the gateway would ACK **while the pump was still transmitting** (bus collision), and the real data message was lost and never ACKed. I reproduced it with a concrete frame | Skip rescue candidates that are the second byte of a `5C 5C` pair in the data area (`escaped_second`). Regression test `escaped_payload_bytes_never_rescue_a_phantom_nested_frame` fails without the fix and passes with it |
| F2 | High | **fixed** | `scripts/build-frontend.sh` | Required an npm `package-lock.json`, but WS05 uses pnpm (`pnpm-lock.yaml`). The CI `frontend` job, the image workflow and `just deploy` (`deploy-release.sh` calls it) all failed | Use `pnpm install --frozen-lockfile && pnpm build`, falling back to `corepack pnpm`. Verified locally |
| F3 | High | **fixed** | `scripts/build-pi.sh` | Features were hard-wired to none, so there was no supported way to build the ARMv6 `serial`/`network-manager` binary that HW-3 and the bench steps need | `NIBE_PI_FEATURES` allowlist (`serial`, `network-manager`, both); `dev` and anything else rejected. Verified both ARMv6 builds pass the ELF gate, and the serial binary starts under QEMU ARM1176 and tries to open its UART |
| F4 | High | **fixed** (WS09; physical policy HP-21) | `crates/device-f730/src/runtime.rs` (`wire_loop`), `apps/gateway/src/main.rs`, `packaging/systemd` | One reply submission slower than the **hypothesised** 10 ms, or any UART error, permanently ends the wire task. There is no reopen, and `/health` stays `ready`, so systemd never restarts the service. The pump is left without its accessory (alarm 251) until someone intervenes. On the single-core Zero, Argon2 logins (19 MiB, about 1–2 s CPU), update verification and the like run at the same priority as the UART thread. | WS09: `WirePolicy` in `crates/device-f730/src/runtime.rs` tolerates isolated misses, bounds exponential UART reopen, and tracks monotonic wire health; gateway exits nonzero after exhaustion and `/health` reports fatal/recovering/age. Unit grants UART-only nice -5 via LimitNICE. Injected stall/error/full-channel and `tests/integration/ws09-runtime-smoke.py` PTY unplug/replug pass. [ADR 0040](../decisions/0040-wire-task.md), [0091](../decisions/0040-wire-task.md); timing remains a config hypothesis |
| F5 | High | **fixed** (WS09; boot/radio HP-23) | `packaging/systemd/nibe-gateway.service`, pi-gen stage | The factory unit runs `--device simulated --bind 127.0.0.1:8080` from a binary without `serial`/`network-manager`, and no Avahi file or Polkit grants are installed. A flashed image therefore cannot provision Wi-Fi, be reached on the LAN, or talk to the pump. This blocks M3, M4 and M7 acceptance ("non-developer can flash, provision, use") | WS09: `packaging/systemd/nibe-gateway.service` uses `hardware.json` (hardware/NM/LAN); pi-gen installs own-profile Polkit, Avahi and explicit simulated drop-in; Pi/CI defaults compile serial,network-manager with SHA-bound feature evidence. `NIBE_WIFI_COUNTRY` configures the region (SE hypothesis). Stage/unit/Polkit/ARMv6/QEMU checks pass; actual image generation requires rootful Docker. [ADR 0111](../decisions/0111-privilege-separation.md) |
| F6 | Medium | **fixed** (WS10) | `crates/updater` + `crates/persistence::migrate` | The updater only checks that the *new* release accepts config schema 2. Suppose a future release migrates `config.json` forward (v3) and is then rolled back, automatically or by hand. The old binary fails closed on the "future schema", so readiness fails and the result is `rollback_not_ready`, with the gateway down | WS10: additive `min_reader_version` contract preserves newer schema and unknown top-level/network settings in `crates/persistence/src/lib.rs`; v3 manual/automatic rollback exercised in `tests/integration/ws07-update-smoke.py`. [ADR 0095](../decisions/0095-minimum-config-reader.md) |
| F7 | Medium | **fixed** (WS10) | `crates/updater/src/lib.rs` (`finish_activate`) | The update is committed after 3 health probes within 30 s. A release that crashes or hangs later (for example on its first real pump frame, or after minutes) restart-loops without automatic rollback. Health also says nothing about the wire task (see F4) | WS10: durable probation phase, configurable 600-second continuous readiness, fatal-wire rejection and persisted restart/boot count in `crates/updater/src/{lib,service}.rs`; fake-clock tests and real restart-loop smoke. [ADR 0070](../decisions/0070-signed-updates-and-probation.md); Pi tuning HP-22 |
| F8 | Medium | **fixed** (WS10) | `crates/api/src/auth.rs` (`attempt`) | The login rate limit is global (5 per 60 s across all clients). I verified that any LAN client can keep the real admin locked out (correct password → 429) indefinitely | WS10: TCP-peer buckets, exponential backoff, 60/minute global admission and single Argon2 worker in `crates/api/src/auth.rs`; real two-source/spoofed-header test in WS06 smoke. [ADR 0060](../decisions/0060-first-owner-and-sessions.md) |
| F9 | Medium | **fixed** | `crates/api/src/maintenance.rs` (`upload`/`call`) | The 48 MiB upload body was copied again (`Bytes::to_vec`), about 96 MiB transient on a 512 MB Pi with the updater holding another copy | Pass `Bytes` through to the RPC; no copy |
| F10 | Medium | **fixed** (WS10) | `crates/device-f730` | Imported catalogs and observation/verification state are process-local; nothing records which registers were verified on *this* pump across restarts (brief §7 "verification state") | WS10: atomic change-only `register-verification.json` in `crates/persistence`, separate explorer overlay in `crates/api/src/f730.rs`, source/date in admin explorer; confirmed-read evidence survives passive sightings/restarts. [ADR 0041](../decisions/0041-register-knowledge-layers.md) |
| F11 | Medium | **fixed** (WS09; physical handoff HP-24) | Session C prerequisite | Feasibility Phase 3 (longer trial on the NibePi OS) needs a fallback watchdog unit that restarts NibePi when the gateway stops or sees no MODBUS40 frame for 60 s. Nobody has written it | WS09: `packaging/nibepi/{fallback.py,nibe-fallback.service,nibe-fallback.timer,README.md}` checks public wire health/frame age (configurable 60000 ms), stops gateway before starting legacy, retries failed stop and latches success. Compatible with read-only NibePi root. Fake-systemctl tests and PTY stale/unplug smoke pass. |
| F12 | Low | **fixed** | `crates/api/src/maintenance.rs`, `auth.rs` | A capture export without a diagnostic lease returned 403 "same-origin request header required" (misleading) | New `AuthError::lease_required()` ("active diagnostic lease required"); asserted in `ws07-update-smoke.py` |
| F13 | Low | **fixed** | `packaging/systemd/nibe-updater.service` | The root updater (SHA-256/Ed25519 over ≤ 48 MiB, slot fsyncs) ran at the same CPU/IO priority as the gateway on a single core | `Nice=10`, best-effort IO priority 7 |
| F14 | Low | **fixed** (WS10) | `crates/updater/src/artifact.rs` | No downgrade protection: an admin can install any older correctly signed release (including ones with known bugs) | WS10: SemVer precedence guard before slot mutation in `Updater::stage_confirmed`; explicit upload `confirm_downgrade` flag, local client flag and admin checkbox. [ADR 0099](../decisions/0099-http-maintenance-guards.md); updater and HTTP smoke tests |
| F15 | Low | **fixed** (WS10) | `crates/api` + gateway static serving | No CSP / `frame-ancestors` / `X-Frame-Options` on the UI. SameSite=Strict already keeps sessions out of cross-site frames | WS10: outer `nibe_api::security_headers` middleware covers SPA/assets/API/errors with CSP, frame-ancestors none, DENY, nosniff and no-referrer. Full 12-check Playwright suite passes without CSP violations. [ADR 0099](../decisions/0099-http-maintenance-guards.md) |
| F16 | Low | **fixed** (WS09) | `packaging/systemd/supervise.py` | If the gateway ever exits 0 without being asked, the supervisor returns 0 and `Restart=on-failure` does not restart it. Today the gateway exits 0 only on a signal | WS09: `packaging/systemd/supervise.py` maps unrequested exit 0 to 1, preserves nonzero/signal failures and returns 0 on requested shutdown. `scripts/tests/test_supervise.py` verifies exit 0/7/SIGTERM and graceful signal forwarding |
| F17 | Low | **fixed** (WS10) | `crates/updater/src/lib.rs` (`prepare_rollback`) | After a rollback, `previous == active`, so another manual "rollback" just restarts the same slot (harmless, but confusing) | WS10: `Updater::prepare_rollback` rejects previous == active outside recovery; updater regression and HTTP 409 after rollback. [ADR 0099](../decisions/0099-http-maintenance-guards.md) |
| F18 | Info | **fixed** (docs) | `docs/dev/setup.md`, `docs/dev/cross-compile.md`, `docs/research/README.md` | Stale statements: npm frontend, deploy "not implemented", HW item names | Updated |
| F19 | Info | open | `crates/protocol` (C0 branch) | When an accessory `C0` frame is incomplete, the decoder drops to the next `5C`. A crafted accessory payload could in theory resync into a phantom pump frame. This is far less likely than F1 because accessory requests are 6/10 bytes | Revisit with real captures (HP-03) |
| F20 | Info | **fixed** (WS10) | API | Uptime and IP address are still not in the API (the UI says so). There is no periodic release check (optional in the brief). There are no soak tests yet (M7) | WS10: monotonic `uptime_s` and backend `ip_addresses` in status; NetworkManager IPv4/IPv6 AddressData, OpenAPI snapshots/generated types and overview/Wi-Fi UI updated. HTTP/browser verified; optional periodic release checking stays optional and soak belongs to WS09. [ADR 0099](../decisions/0099-http-maintenance-guards.md) |

Safety and security items that **pass** (verified in code and by test): no raw-write endpoint and no
write encoder; write token → ACK only; RMU40 and other addresses get no reply; hardware probes → 403;
the read encoder refuses checksum-`0x5C` (40043, 48968) and `5C`-in-data (48732) requests; checksum and LEN
are over escaped wire bytes with `5C→C5` accepted only when the computed XOR is `5C`; unescape happens after the checksum;
48852 = 0 → high word first for read responses only; 0x68 with 48852 = 0 stays raw; 48852 is never
written. The wire thread is isolated with bounded `try_send` channels (tests cover a saturated observation
channel). The serial port is opened with TIOCEXCL plus an exclusive `flock` (new PTY test: a port already locked is
refused). There is no default password. Setup token and AP password are 256/128-bit random, 0600, never served or
logged. Every data/update/debug route needs a session; debug and capture also need the ≤ 900 s lease, which is
revoked on logout or restart. Support JSON is an allowlist. Dev routes are compiled out and `build-pi.sh` cannot add
them. Updates are verified with `verify_strict` Ed25519 over the manifest plus the payload SHA-256, with
bounded tar entries (no links, no `..`, no absolute paths) and durable journal phases; interrupted staging is
discarded, and interrupted switch/check/rollback goes back to the previous slot.

## Verify

```bash
export PATH="$HOME/.local/share/nibe-build-tools/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
just test                                   # = cargo test --workspace --all-features + tooling tests
cargo test -p nibe-protocol escaped_payload  # F1 regression
PROPTEST_CASES=20000 cargo test -p nibe-protocol
./scripts/build-frontend.sh                  # F2 (pnpm)
pnpm --dir apps/admin test:e2e               # 10 real-Chromium checks; screenshots in tests/e2e/artifacts/
./tests/integration/http-smoke.sh default && ./tests/integration/http-smoke.sh dev
./tests/integration/ws06-http-smoke.sh
./tests/integration/ws07-update-smoke.py     # includes F12 assertion
python3 crates/device-f730/tests/replay-http-smoke.py
python3 crates/device-f730/tests/serial-pty-smoke.py   # includes second-master lock refusal
NIBE_PI_FEATURES=dev ./scripts/build-pi.sh   # must fail (exit 2)
NIBE_PI_FEATURES=serial NIBE_TARGET_DIR=/tmp/ws08-pi-serial ./scripts/build-pi.sh
NIBE_PI_FEATURES=serial,network-manager NIBE_TARGET_DIR=/tmp/ws08-pi-full ./scripts/build-pi.sh
# Release-build adversarial checks (expect 404 for /__dev, 401 for data/update/debug without a session):
cargo build --release -p nibe-gateway
./target/release/nibe-gateway --device simulated --bind 127.0.0.1:18301 --data-dir "$(mktemp -d)/d" &
for p in /__dev/reset /api/v1/device /api/v1/diagnostics/support /api/v1/updates/status; do
  curl -s -o /dev/null -w "$p %{http_code}\n" http://127.0.0.1:18301$p; done; kill %1
```

All of the above passed on 2026-10-22 (host: Arch Linux, system Rust; the Pi builds use the pinned toolchain).

## Open issues / hardware-pending

- WS09 fixed F4/F5/F11/F16 and WS10 fixed F6/F7/F8/F10/F14/F15/F17/F20 locally. F19 still needs real captures. Physical wire, appliance and fallback acceptance remain HP-21/23/24; WS10 bench acceptance is HP-22.
- All physical verification is in [HARDWARE_PENDING.md](HARDWARE_PENDING.md) (HP-01…HP-24), to be run in the order of
  [docs/hardware-test-plan.md](../hardware-test-plan.md). Nothing has been run on the owner's hardware.
- The bundled replay capture and all protocol vectors are **synthetic**. M2's acceptance ("recorded F730 frames decode
  into verified device state") needs the HP-03 capture.

## Log

- 2026-10-22: Read all brief, progress, research and ADR files, then reviewed the code listed above. Ran
  `cargo test --workspace --all-features` (pass), all six curl/Python smokes (pass), the Playwright suite
  (10/10 pass; screenshots of overview, stale readings, mobile register explorer and updates inspected), and
  manual `--device simulated` (release build) and `cargo run -p nibe-gateway -- --device replay` sessions with curl:
  setup, status, identity, measurements, explorer (581 entries, 17 decoded, all `writable: false`), probe refusals,
  and disabled controls.
- 2026-10-22: Adversarial tests as listed under "Review scope". Found F1 by analysing the rescue path and
  confirmed it with a concrete frame. Fixed F1, F2, F3, F9, F12 and F13 with tests or verification. Added a
  second-master lock refusal check to the PTY smoke; serialport-rs already takes `flock`, so no code change was needed.
- 2026-10-22: Wrote `docs/hardware-test-plan.md` (Session A in detail: inventory, NibePi backup, passive
  capture, binary on the NibePi OS, a ≤ 10-min read-only trial using a transient unit with `ExecStopPost` NibePi
  restart plus a 15-min dead-man timer; Sessions B–D outlined; approval points; time estimates). I dry-ran
  A4/A5's curl/login/explorer/show.py commands locally, with replay standing in for hardware. Rewrote
  HARDWARE_PENDING.md as an ordered, deduplicated HP-01…HP-20 list (all WS01–WS07 items merged; the original
  text is in git history). Wrote the root `README.md`. Fixed stale dev docs. Updated PROGRESS.md.
- Files touched outside WS08's own paths (small, noted per AGENTS.md): `crates/protocol/src/lib.rs` (F1 fix and
  test), `crates/api/src/{auth,maintenance}.rs` (F9, F12), `scripts/build-frontend.sh` (F2), `scripts/build-pi.sh` (F3),
  `packaging/systemd/nibe-updater.service` (F13), `tests/integration/ws07-update-smoke.py`,
  `crates/device-f730/tests/serial-pty-smoke.py`. No commits.

- 2026-10-23: WS10 resolved F6/F7/F8/F10/F14/F15/F17/F20; implementation/test pointers added to each finding. Minimum-reader migration policy, continuous-readiness probation, source auth limits, persisted evidence, downgrade/rollback guards, response security and uptime/IP are recorded in ADRs 0095, 0070, 0060, 0041 and 0099. Hardware tuning/durability remains HP-22; no pump writes or commits.

- 2026-10-23: WS09 resolved F4/F5/F11/F16 with configurable wire resilience/health/priority, hardware/LAN packaging, read-only-root NibePi fallback and supervisor exit mapping. Importer and bounded local soak added; verification and physical limits are recorded in [WS09](WS09.md). No commits.
