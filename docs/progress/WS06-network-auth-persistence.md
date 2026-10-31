# WS06 — Network provisioning, auth, persistence

- **Milestone:** M3
- **Status:** done-hw for boot-partition provisioning, station Wi-Fi, mDNS and auth; setup AP done-sim (HP-13/14, HP-40); open setup AP (ADR 0060) done-sim 2026-10-31
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
**only while no admin exists**. The token is read locally from
`<data-dir>/setup-secrets.json` (0600; never served by HTTP). Since 2026-10-31
(ADR 0060) the setup/recovery AP is **open** (no WPA2 password exists any more)
and a client on it (socket peer in 192.168.4.0/24 *and* the AP up) needs no token
while no admin exists; there is no label or handoff. From the LAN, loopback or
Tailscale the token is still required, so simulation users read the local file.
`GET /api/v1/setup/status` reports `setup_token_required` for the asking client;
the UI shows the token field only when it is true.

| Method / path | Request | Successful response (200) |
|---|---|---|
| GET `/api/v1/setup/status` | none | `{ "setup_required": true, "setup_token_required": true, "network_state": "ap", "setup_ssid": "NIBE-Gateway-XXXX", "setup_url": "http://192.168.4.1:8080/", "simulated": true }` (SSID null outside AP/recovery) |
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
  ADR `0060-first-owner-and-sessions.md` records decisions/tradeoffs.
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
- 2026-10-25 (bookkeeping): boot-partition provisioning verified (HP-30, ADR 0060); NetworkManager boot race fixed (0.1.1); a gateway restart keeps a connected station (0.1.21); Rust provisioner replaced Python (HP-36); avahi on wlan0 only on Alpine, `nibe-gateway.local` resolves (alpine.6, HP-38); root Wi-Fi fallback `nibe-wifi-fallback` (ADR 0111 amendment), installed with alpine.7 but not yet triggered (HP-40 step 6). HP-12/HP-15 verified. Open: HP-13/14 setup AP and recovery, HP-40 (bench).
- 2026-10-31: Open setup AP without device credentials (ADR 0060, owner decision). Found while
  preparing HP-47: the `firstboot.1` AP was WPA2 with a random password and the admin form needed the setup token,
  both only in root-only `setup-secrets.json`, and the SSID is `NIBE-Gateway-XXXX`, not `nibe-gateway-setup`.
  Changes: `SetupSecrets` no longer has `ap_password` (old files with it still load; dropped at the next factory
  reset); NM `start_ap` builds the AP without `802-11-wireless-security` (settings now a free function, unit
  tested). `Auth::provisioning`/`setup` take `on_setup_ap`; `auth::setup_ap_client` = socket peer (ConnectInfo,
  never forwarded headers) in 192.168.4.0/24 **and** the provisioner's published state `ap`/`recovery_ap`, so the
  token stays required on a home LAN using the same subnet, from loopback (simulation), Tailscale and the LAN.
  Once an admin exists neither the AP nor the token counts (unchanged). `GET /api/v1/setup/status` adds
  `setup_token_required` for the asking client (OpenAPI snapshots regenerated); the UI shows the token field
  unless it is `false`, rather than inferring it from `network_state` (same rule as the guard, one place).
  Admin password rule, login limits, station password and "no AP after an outage" unchanged. Card README names
  the open AP. Cross-workstream: `apps/admin` (WS05, AuthForm/api/schema/fixture), `packaging/alpine/boot/README.txt`
  (WS03), contract test support (`Server::unclaimed_from`).
  Tests: persistence `setup_secrets_without_ap_password_and_legacy_files_load`; network
  `setup_ap_is_open_and_station_keeps_wpa2`; api `setup_ap_client_needs_the_ap_subnet_and_the_ap_up`,
  `tokenless_setup_only_from_the_ap_and_only_while_unclaimed`; contract
  `setup_ap_clients_need_no_setup_token_until_an_admin_exists` (AP peer tokenless incl. CSRF still enforced, LAN
  peer needs the token, AP subnet with the station up needs the token, nothing after the admin exists).
  Verify: `cargo test --workspace`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo fmt --all -- --check`; `pnpm --dir apps/admin run typecheck`; `pnpm --dir apps/admin run build &&
  pnpm --dir apps/admin test:e2e` (14/14). Image `0.1.31-firstboot.2` built from HEAD + the uncommitted WS03
  first-boot layout diff (worktree t3-998cdbcb, ADR 0129) + this diff; see HARDWARE_PENDING 2026-10-31 entry.
  Open: hardware (open AP on brcmfmac, peer address over NM shared mode): HP-47 step 4 with `firstboot.2`, HP-40
  step 3, HP-13. Commit together with the WS03 first-boot diff, which `firstboot.2` contains.
- 2026-10-31: HP-47 step 4 with `firstboot.2` failed on the Pi: no AP. Cause (verified from the card's p3
  `boot-logs/0001-a-normal/messages.txt`): every `AddAndActivateConnection2` from uid 995 was refused with "Not
  authorized to share connections via wifi." An open shared AP needs polkit `wifi.share.open`; the rule allowed
  only `.protected` and its catch-all denied the rest (`test_appliance_policy.py` even asserted that). The gateway
  then exited `Error: Unavailable`, supervise-daemon gave up after 6 restarts, so HTTP never came up. QEMU runs
  `--network simulated` and never asks polkit. Fixes: (1) the rule allows `wifi.share.open` (`.protected` and the
  catch-all NO kept; ADR 0060 amendment). (2) A refused AP no longer stops the gateway: `Provisioner::new`, a
  failed setup connect, the persistence-failure path and factory reset record a pending AP; state `offline`,
  `last_error` "setup access point did not start: ...", and the restore loop calls `retry_ap` (30 s doubling to
  5 min; only for a pending AP, never for an offline saved station; cleared when the station or an AP comes up).
  `--recover-network` still fails loudly (maintenance console). NM logs its refusal reason at warn. (3) Lint
  `the_polkit_rule_allows_sharing_the_setup_ap_as_built` (network, `network-manager` feature) maps the AP's
  security mode to the share action and checks the shipped rule's allow list; it fails against the old rule.
  Crate README updated (it still described a WPA2 AP and no polkit grant).
  Tests: network `a_refused_setup_ap_keeps_the_gateway_up_and_is_retried_with_a_backoff`,
  `an_offline_station_never_gets_an_ap_from_the_ap_retry`, `a_factory_reset_whose_ap_is_refused_retries_it`.
  Verify: `cargo test --workspace --all-features --locked`; `cargo test --workspace`; `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`; `cargo fmt --all -- --check`;
  `python3 -m unittest discover -s scripts/tests`. Image `0.1.31-firstboot.3` built from worktree t3-f5f53074,
  which now holds the WS03 first-boot diff (t3-998cdbcb), the ADR 0060 diff (t3-97bb8e62) and this fix; commit
  from there. Open: HP-47 from step 2 with `firstboot.3`.
- 2026-10-31: Update from a `firstboot.3` card to 0.1.32 via the UI failed: `setup-secrets.json` written by the
  open-AP code has no `ap_password`, and every earlier release deserializes it as a required 32-hex field
  (reproduced: 0.1.32 on such a data dir exits ``missing field `ap_password` ``). Fix: `SetupSecrets` keeps
  `ap_password` (private, unused; earlier releases use it as their WPA2 AP password), generate and factory reset
  write it, and `Store::open` adds one to a file that lacks it (one write); validation is the same rule as earlier
  releases. Test `setup_secrets_stay_readable_by_releases_before_the_open_ap` (failed before the fix); checked by
  hand that 0.1.32 starts on a firstboot.3 data dir after one start of the fixed build. A firstboot.2/.3 card
  therefore updates to 0.1.33 (or later) but never to 0.1.32. Renumbered on rebase onto 0.1.32 (which took ADR
  0128 and HP-46 for the audit log): first boot is ADR 0129, the open AP ADR 0060, the fresh-card test HP-47.
