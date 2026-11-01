# Provisioning backends

`SimulatedNetwork` never touches the host network. `network-manager` enables the
zbus system-bus implementation; the gateway also needs `--network network-manager`
to use it with the simulated pump for an isolated bench test. The selected
interface defaults to wlan0. SSIDs must be UTF-8, 1–32 bytes. Initial hardware
scope supports open station networks and WPA2 personal; enterprise/WEP are not
supported. The setup and recovery AP is open.
Never test this backend on the remote owner's active network interface.

The API contract is at the end of this file. NetworkManager and the JSON config have no shared transaction, so an
interruption can leave a claimed, configured station offline; explicit recovery then brings it back.
Provisioner serializes operations, verifies activation, saves only successful
connections, rolls back bad passwords, and never opens AP after station outage.
If NetworkManager refuses the AP (first boot, factory reset, failed setup
connect), the gateway stays up, reports `offline` with `last_error`, and retries
that AP every 30 s doubling to 5 min.
Checkpoints bound rollback even if the process is interrupted. The app and NM
stores cannot participate in a single filesystem transaction.

## System integration (bench/image maintainer)

Use a gateway-owned 0700 state directory on the persistent partition, and persist
NetworkManager's root-owned keyfiles/state separately.
The service runs as nibe-gateway with the narrow polkit allow list in
`packaging/common/49-nibe-gateway-network.rules` (every other NetworkManager
action is denied). The open AP needs `wifi.share.open`; a unit test ties the AP's
security mode to that list. Do not grant the service unrestricted D-Bus access
or change the remote development host network.
NM shared IPv4 uses dnsmasq-base, supplied by the image. Choose the actual radio
country using authorized local regulatory settings before enabling the AP.

`resources/nibe-gateway.service` is the Avahi advertisement template for port
8080. Once an authenticated LAN listener is deliberately enabled, install it
with `install -m644 crates/network/resources/nibe-gateway.service
/etc/avahi/services/nibe-gateway.service` and reload avahi-daemon; configure the
appliance hostname `nibe-gateway` and check `avahi-resolve-host-name
nibe-gateway.local` from a second LAN machine. Adjust the service port if the
listener port changes. The image's disabled template remains disabled for
its simulator/loopback factory configuration, which is not a reachable LAN
service. Do not enable mDNS on the developer host as part of local verification.

If Wi-Fi is unavailable, an authorized local maintenance operator can restart
with `--recover-network` plus the same data directory and `--network
network-manager`; join the open `NIBE-Gateway-XXXX` AP and log in with the
existing admin password. No default admin password or automatic GPIO reset
exists. The authenticated factory-reset endpoint requires `ERASE`, rotates the
setup token and reopens the open setup AP.

## API contract

All requests are same-origin JSON. Send `credentials: "same-origin"` and
`X-Requested-With: nibe-gateway` on every non-bearer mutation, including login and setup (CSRF defense; no CORS).
Errors use `{ "error": { "code": "...", "message": "..." } }`.
Auth/network errors: 401 `unauthorized`, 403 `forbidden`, 409 `conflict`,
422 `validation`, 429 `busy`, 503 `unavailable`. No passwords/tokens in URLs.

Public routes: `GET /health`, `GET /openapi.json`, `GET /api/v1/setup/status`,
`GET /api/v1/auth/session`, `POST /api/v1/auth/login`.
All other `/api/v1/*` reads (including SSE), discovery, controls and `/__dev/*`
require an admin session, or for allowed routes a scoped API key (`Authorization: Bearer`; no CSRF header, and the
`crates/api/src/access.rs` allowlist decides by scope). Controls keep their own safety gate.
Setup/provisioning requests accept either an admin session or `X-Setup-Token`
**only while no admin exists**. The token is read locally from
`<data-dir>/setup-secrets.json` (0600; never served by HTTP). The setup/recovery AP is **open** (no WPA2 password exists any more)
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
| GET `/api/v1/network/status` | session or setup token | `{ "state": "ap", "ssid": null, "setup_ssid": "NIBE-Gateway-XXXX", "last_error": null, "ip_addresses": ["127.0.0.1"], "simulated": true }` |
| GET `/api/v1/network/scan` | session or setup token | `{ "networks": [{ "ssid": "Home WiFi", "signal": 80, "secured": true }] }` |
| POST `/api/v1/network/connect` | `{ "ssid": "Home WiFi", "password": "correct-password" }` | network status; `state: "connected"` only after station verification; bad password returns 422 and restores previous connection/AP |
| POST `/api/v1/network/recovery` | session | network status with `state: "recovery_ap"`; explicit recovery AP |
| POST `/api/v1/setup/factory-reset` | session, `{ "confirmation": "ERASE" }` | `{ "reset": true }`, clears admin/network config and sessions, rotates setup credentials, returns to first-boot AP |

Network states: `ap`, `connecting`, `connected`, `offline`, `recovery_ap`.
A normal station outage remains `offline`; it never automatically starts AP.
A failed first-boot connect returns to `ap`, or to `offline` with `last_error` and a retried AP if the AP is refused; a failed configured reconnect restores
its prior profile, otherwise `offline`. A single-radio switch can disconnect the
browser before HTTP completion: reconnect to the printed AP on failure or use
`http://nibe-gateway.local:8080/` on success, then refetch status/session.
No captive DNS interception is promised; known setup URL is the initial path.
Simulator scan fixtures: `Home WiFi` (password `correct-password`),
`Guest WiFi` (open); other SSIDs and wrong passwords fail deterministically.
Sessions are memory-only, expire after 12h, and are invalidated on restart/reset.
Login/setup hashing runs off the async runtime with a bounded single-worker gate;
failed logins back off per client address, with a global cap on hashing work.

`/openapi.json` is the full schema.
