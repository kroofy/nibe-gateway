# WS06 — Network provisioning, auth, persistence

- **Milestone:** M3
- **Status:** done-hw for boot-partition provisioning, station Wi-Fi, mDNS and auth; setup AP done-sim (HP-13/14, HP-40)
- **Owns:** crates/network, crates/persistence, auth module in crates/api or core (coordinate)
- **Brief:** [08-wifi-provisioning.md](../brief/08-wifi-provisioning.md), [14-security.md](../brief/14-security.md), [11-ota-and-recovery.md](../brief/11-ota-and-recovery.md)

## API contract for WS05 (2026-10-22, implemented contract)

All requests are same-origin JSON. Send `credentials: "same-origin"` and
`X-Requested-With: nibe-gateway` on **every POST** (CSRF defense; no CORS).
Errors use `{ "error": { "code": "...", "message": "..." } }`.
Auth/network errors: 401 `unauthorized`, 403 `forbidden`, 409 `conflict`,
422 `validation`, 429 `busy`, 503 `unavailable`. No passwords/tokens in URLs.

Public routes: `GET /health`, `GET /openapi.json`, `GET /api/v1/setup/status`,
`GET /api/v1/auth/session`, `POST /api/v1/auth/login`.
All existing `/api/v1/*` reads (including SSE), discovery, controls and `/__dev/*`
require an admin session. Controls retain their independent WS02 safety gate.
Setup/provisioning requests accept either an admin session or `X-Setup-Token`
**only while no admin exists**. Initial setup needs the per-device setup token,
read locally from `<data-dir>/setup-secrets.json` (0600; never served by HTTP).
The installer must place this credential and AP password on a device-specific
label/secure handoff. Simulation users can read the local file.

| Method / path | Request | Successful response (200) |
|---|---|---|
| GET `/api/v1/setup/status` | none | `{ "setup_required": true, "network_state": "ap", "setup_ssid": "NIBE-Gateway-XXXX", "setup_url": "http://192.168.4.1:8080/", "simulated": true }` (SSID null outside AP/recovery) |
| POST `/api/v1/setup/admin` | `{ "password": "12+ characters" }` + setup token | `{ "authenticated": true, "setup_required": false }` + session cookie; rejects repeat setup |
| GET `/api/v1/auth/session` | optional cookie | `{ "authenticated": false, "setup_required": true }` |
| POST `/api/v1/auth/login` | `{ "password": "..." }` | session response + `Set-Cookie: nibe_session=...; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200` (Secure when TLS cookie option enabled) |
| POST `/api/v1/auth/logout` | session | session response, expires/revokes cookie |
| GET `/api/v1/network/status` | session or setup token | `{ "state": "ap", "ssid": null, "setup_ssid": "NIBE-Gateway-XXXX", "last_error": null, "simulated": true }` |
| GET `/api/v1/network/scan` | session or setup token | `{ "networks": [{ "ssid": "Home WiFi", "signal": 80, "secured": true }] }` |
| POST `/api/v1/network/connect` | `{ "ssid": "Home WiFi", "password": "correct-password" }` | network status; `state: "connected"` only after station verification; bad password returns 422 and restores previous connection/AP |
| POST `/api/v1/network/recovery` | session | network status with `state: "recovery_ap"`; explicit secure recovery AP |
| POST `/api/v1/setup/factory-reset` | session, `{ "confirmation": "ERASE" }` | `{ "reset": true }`, clears admin/network config and sessions, rotates setup credentials, returns to first-boot AP |

Network states: `ap`, `connecting`, `connected`, `offline`, `recovery_ap`.
A normal station outage remains `offline`; it never automatically starts AP.
A failed first-boot connect returns to `ap`; a failed configured reconnect restores
its prior profile, otherwise `offline`. A single-radio switch can disconnect the
browser before HTTP completion: reconnect to the printed AP on failure or use
`http://nibe-gateway.local:8080/` on success, then refetch status/session.
No captive DNS interception is promised; known setup URL is the initial path.
Simulator scan fixtures: `Home WiFi` (password `correct-password`),
`Guest WiFi` (open); other SSIDs and wrong passwords fail deterministically.
Sessions are memory-only, expire after 12h, and are invalidated on restart/reset.
Login/setup hashing runs off the async runtime with a bounded single-worker gate;
failed logins are globally limited (5 attempts/60s, includes unknown admin).

## Checklist
- [x] persistence: atomic writes (write-temp+fsync+rename), versioned config schema w/ migrations, secrets file perms 0600, minimal SD writes
- [x] auth: initial admin setup (no default password), argon2 hashing, session tokens/cookies, rate limiting on login, authorization for writes/updates/debug
- [x] network trait + simulated impl (scan, connect, wrong password, AP mode, recovery) + NetworkManager D-Bus impl (zbus) behind feature
- [x] Provisioning state machine: first-boot AP with unique SSID/pass (derived from serial/MAC), scan, connect-verify, fallback to AP, explicit recovery/factory-reset; never open AP on ordinary outage
- [x] mDNS advertisement (avahi service file or library)
- [x] API endpoints for wifi/setup/auth; tests for every state transition incl. invalid password and recovery

## Verify
Run from repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
./tests/integration/ws06-http-smoke.sh
./tests/integration/http-smoke.sh default
./tests/integration/http-smoke.sh dev
cargo run -p nibe-gateway -- --device simulated --data-dir /tmp/nibe-ws06-dev
```

The last command creates a private local setup proof; see
`apps/gateway/README.md` for the exact curl setup/login/cookie flow.
`crates/network/README.md` describes opt-in hardware bench invocation and Avahi
installation. The simulated default never touches the host radio. OpenAPI
snapshots cover all new schemas, cookie/setup security alternatives, and the
required POST header is documented in the contract/description.

Verified: `cargo test --workspace` and `cargo test --workspace --all-features`,
strict all-target/all-feature workspace clippy, workspace formatting, all three
curl scripts, and an all-feature gateway compile pass. The auth contracts also
verify that revoking a session closes an already-open SSE stream. Workspace
verification includes the concurrently integrated WS04 crates.

## Open issues / hardware-pending
- No hardware/radio operation was attempted on the remote development host.
  Pi radio transitions, NM/Polkit/driver permissions, mDNS/real LAN, secure
  physical credential handoff, SD power cuts and ARMv6 Argon2 budget are exact
  append-only entries in `HARDWARE_PENDING.md`.
- NM and JSON have no shared transaction: an interruption can leave a claimed,
  configured station offline; explicit secured recovery is required (ADR 0060).
  No automatic outage AP, default password or GPIO reset is provided.
- `/api/v1/network/status` is authoritative for provisioning. Existing pump
  snapshot/status `network` fields remain synthetic scenario domain fields,
  independent from this infrastructure adapter.
- Hardware NM backend is feature-gated and opt-in for a simulated bench pump.
  Factory image remains loopback/simulator with mDNS disabled. Service-user
  NM privilege policy and the physical label/handoff must be validated before
  enabling radio provisioning in a production image; root-console bench steps
  are provided. TLS termination is deployment-owned; `--secure-cookie` supports it.

## Log

- 2026-10-22: Published the auth/setup/network API contract **before code** for
  WS05. Implemented `crates/persistence` (locked 0700 dir, 0600 secrets/config,
  no-change suppression, fsync/rename, v1→v2 migration, preserved unknown keys,
  fail-closed corruption/future schema), and `crates/network` (shared state
  machine, deterministic simulated fixtures/outage, connecting progress, explicit
  recovery/reset, optional zbus 4 system-bus NM adapter with checkpoints and
  verified station/AP activation). Added opt-in Avahi XML and bench/deployment
  instructions. Setup SSID suffix uses serial/MAC, with independent random
  password/proof. NM owns protected station secrets; app stores only selected SSID.
- 2026-10-22: Added API-local Argon2id admin setup/login, bounded off-runtime
  hashing/admission, random expiring memory-only cookie sessions, logout/reset
  revocation, same-origin POST header/Origin defenses, body-size limit and typed
  errors. Integrated real auth/network/store in gateway, configurable data dir,
  Secure-cookie option and maintenance-console recovery. Existing core capability
  injection and disabled hardware controls remain intact. Network mutation jobs
  complete if a single-radio HTTP client disconnects. SSE rechecks authorization.
  Added/updated real-TCP auth/state tests and executable curl flows including
  process restart. Authenticated existing tests keep WS02 checks meaningful.
- 2026-10-22: Minimal shared-path edits: root Cargo members/comment and generated
  Cargo.lock; API/app integration and contract tests/snapshots (WS02 done);
  .gitignore excludes local .data secrets; systemd ExecStart now passes its
  existing private `/var/lib/nibe-gateway/state` directory (WS03). No other
  packaging defaults/network privileges were broadened. Coordinated WS04's
  concurrent gateway integration and preserved its new workspace members.
  ADR `0060-ws06-auth-and-durable-provisioning.md` records decisions/tradeoffs.
  No commits; unrelated agents' changes preserved.
- 2026-10-22: Final validation passed: default + all-feature workspace tests,
  strict workspace clippy (`--all-targets --all-features -- -D warnings`),
  `cargo fmt --all -- --check`, all-feature gateway compilation, and all three
  real-executable curl suites. Rechecked gateway onboarding/restart after moving
  network startup behind device/wire service startup so NM transition waits do
  not delay UART servicing. Corrected SSE test transport to request connection
  close and verified real session-revocation shutdown. Regenerated both OpenAPI
  snapshots after WS04's additive read-only explorer schemas landed. All checklist
  software items complete; only the explicitly documented physical/deployment
  validations remain. No hardware claims and no commits.
- 2026-10-25 (bookkeeping): boot-partition provisioning verified (HP-30, ADR 0108); NetworkManager boot race fixed (0.1.1); a gateway restart keeps a connected station (0.1.21); Rust provisioner replaced Python (HP-36); avahi on wlan0 only on Alpine, `nibe-gateway.local` resolves (alpine.6, HP-38); root Wi-Fi fallback `nibe-wifi-fallback` (ADR 0092 amendment), installed with alpine.7 but not yet triggered (HP-40 step 6). HP-12/HP-15 verified. Open: HP-13/14 setup AP and recovery, HP-40 (bench).
