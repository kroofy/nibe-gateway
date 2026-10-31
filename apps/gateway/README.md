# Gateway

From the repository root:

```bash
cargo run -p nibe-gateway -- --device simulated
# Equivalent JSON configuration; CLI overrides JSON:
cargo run -p nibe-gateway -- --config apps/gateway/config.simulated.json
```

Default bind is `127.0.0.1:8080`. Simulation uses a private `./.data` directory;
`--data-dir PATH` overrides it. Future hardware mode defaults to
`/var/lib/nibe-gateway`; the packaged service uses its separately owned
`/var/lib/nibe-gateway/state`. Never put app state in a served admin directory.
Do not run two gateways against one data directory (an exclusive lock rejects it).

The UI and `/health`, `/openapi.json`, `/api/v1/setup/status`,
`/api/v1/auth/session` are public; device/API data requires an admin session.
On first boot there is no admin password. Read the per-device setup proof from
the local 0600 `<data-dir>/setup-secrets.json` and enter it in the setup UI.
Physical installations need a secure printed/installer handoff; no secrets are
served by HTTP or logged. Example local simulator setup:

```bash
setup_token=$(python3 -c 'import json; print(json.load(open(".data/setup-secrets.json"))["setup_token"])')
curl -fsS -c /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' \
  -H "X-Setup-Token: $setup_token" -H 'Content-Type: application/json' \
  -d '{"password":"choose-a-long-local-password"}' \
  http://127.0.0.1:8080/api/v1/setup/admin
curl -fsS -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/device
curl -fsS http://127.0.0.1:8080/openapi.json
curl -N -b /tmp/nibe-cookies http://127.0.0.1:8080/api/v1/events
```

For subsequent login, POST the password JSON to `/api/v1/auth/login` with the
same X-Requested-With header and save the returned cookie. Every POST, including
login/logout/setup/debug, requires that header. Cookies are HttpOnly and
SameSite=Strict. Sessions expire after 12 hours, on logout/reset, or on restart.
Use `--secure-cookie` behind a same-origin TLS endpoint. There is no built-in
TLS termination; plain HTTP is intended for a protected setup AP/private LAN.
Never expose it directly to the public internet. Network/auth contract and all
request/response bodies: `docs/progress/WS06-network-auth-persistence.md`.

Pump simulation and Wi-Fi simulation are independent. `/api/v1/network/status`
is authoritative provisioning state; the legacy `network` field on the pump
snapshot/status remains the synthetic scenario's domain field. A fresh simulated
network starts in secured setup AP state, with scan fixtures `Home WiFi`
(password `correct-password`) and `Guest WiFi` (open). Failed credentials restore
the prior AP/station; ordinary configured outages never start AP. Saved SSID and
admin hash survive restart. Passwords for station mode are owned by NM, not JSON.

`--network auto` selects simulation for simulated/replay mode and NetworkManager
for hardware mode. To bench-test actual Wi-Fi while simulating the pump,
compile `--features network-manager`, explicitly pass `--network network-manager
--bind 0.0.0.0:8080`, and use an isolated Pi/local authorized root console. This
switches the real radio; do not use it on the remote development host. The backend
needs NetworkManager system D-Bus privileges, a configured regulatory country,
and a persistent root-owned NM profile directory (see `crates/network/README.md`).
`--network-interface wlan0` selects the radio. A single radio may drop the setup
browser connection; try the printed AP after failure or `nibe-gateway.local:8080`
after success. Known setup URL is `http://192.168.4.1:8080/`; no captive portal is
claimed. AP startup is first boot or explicit recovery only.
`--recover-network` is a maintenance-console override for an unreachable device;
it starts the same secured AP and preserves the admin password. Reachable admins
can call `/api/v1/network/recovery`. Factory reset requires an authenticated POST
with `{"confirmation":"ERASE"}`, removes gateway network/admin state, revokes
sessions and rotates setup proof/AP password. Obtain the new physical credentials.

`--device replay` uses WS04 capture fixtures; hardware mode requires the `serial`
feature and the separately verified physical adapter/wiring. Controls have an
independent safety gate. On hardware with `--allow-hardware-reads`, any register
can be written through `POST /api/v1/controls/register-write` with
`{"register":47049,"value":47.0}` (catalog-scaled) or `{"register":47049,"raw":470}`
(ADR 0125, on by default; `--no-open-writes` or `"open_writes": false` turns it
off). No allowlist or range check applies: the caller decides. Each write is sent
once and read back; check `probe.writes.last` in `/api/v1/registers/explorer`.
`POST /api/v1/controls/register-read` with `{"register":47049}` reads any register
number now and returns the pump's answer (up to 20 s; 504 if it never answers).
MQTT offers the same with `allow_commands` on (ADR 0126):
`<prefix>/<device>/register/<id>/set` (scaled value, decimal comma accepted),
`.../set_raw` (raw integer) and `.../get` (publishes the value on
`<prefix>/<device>/register/<id>`, not retained) and `.../poll` (`fast`, `slow`
or `off`). Poll choices are also on the Register explorer page and
`POST /api/v1/controls/register-poll` `{"register":47049,"mode":"slow"}`; they are
saved in `config.json` in the data directory as deviations from `hardware.json`
(`GET /api/v1/registers/polls` shows both). Outcomes go to
`<prefix>/<device>/result`. Writes to 47011, 47045, 47049 and 47375 may carry a
lease (ADR 0123, 0127): `"lease_seconds": 600` in the HTTP body, or the MQTT
payload `{"value": 45.0, "lease_seconds": 600}` on `.../set` or `.../set_raw`.
Renew by writing again with a lease; otherwise the gateway writes back the value
it read before the first leased write. A plain write ends the lease. State:
`register_leases` (every lease) and `heating_offset_lease` in `GET /api/v1/status`.
Replay and simulated mode answer 403/501. Synthetic controls need both
`--features dev` and `--allow-simulated-controls`. Development routes are compiled
out without `dev`:

```bash
cargo run -p nibe-gateway --features dev -- --device simulated --freeze-time --allow-simulated-controls
# Authenticate using the setup/login flow above, then authorize an expiring lease:
curl -fsS -b /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' \
  -H 'Content-Type: application/json' -d '{"seconds":300,"confirmation":"AUTHORIZE_DIAGNOSTICS"}' \
  http://127.0.0.1:8080/api/v1/diagnostics/developer
curl -fsS -b /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' \
  -H 'Content-Type: application/json' -d '{"name":"cold_weather"}' \
  http://127.0.0.1:8080/__dev/scenario
curl -fsS -b /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' \
  -H 'Content-Type: application/json' -d '{"offset":2}' \
  http://127.0.0.1:8080/api/v1/controls/heating-offset
```

Scenario names: `fixtures/scenarios/scenarios.json`. Dev reset restores pump/time
state, preserving real admin/network config. `/health` reports application readiness
from the recent monotonic core heartbeat and live worker, with compiled version
and supervisor-provided release identity. Pump faults remain separately degraded;
`/api/v1/status` reports pump freshness. SSE
uses bounded live state/command/discovery/scenario/resync events; refetch device
state on connect/reconnect/resync. Slow clients cannot block device adapters.
SIGINT/SIGTERM close SSE and drain HTTP. A command timeout can have unknown
outcome: do not blindly retry writes. `--admin-dir PATH` serves a prebuilt SPA
when `index.html` exists; unknown API/debug paths remain JSON errors.

WS04 replay and serial options:

```bash
# Bundled SYNTHETIC F730 session (no physical measurements); local auth/storage:
cargo run -p nibe-gateway -- --device replay
# Custom capture and source-faithful offline catalog; automatic word order:
cargo run -p nibe-gateway -- --device replay --capture path/session.capture.jsonl --catalog fixtures/register-catalogs/f730.json
# Add --word-swap 0|1 only when that capture/installation's setting is known.
```

Hardware construction is `cargo run -p nibe-gateway --features
serial,network-manager -- --device hardware --serial /dev/ttyAMA0
--record-capture /private/new-session.capture.jsonl`. This opens the real UART
and may change the radio; use only WS01 HW-3's approved migration/fallback plan.
`--network simulated` permits an isolated local PTY test. Automatic word order
starts unknown; passive observations/read responses of 48852 set it, and
`--word-swap` can supply a menu-verified assumption. 48852 is never written.
32-bit data messages with word-swap=0 stay raw because sources conflict.
Hardware probes and all hardware/replay writes are disabled in every build.
Fatal UART/reply failures stop servicing, expose the error and require restart.

Authenticated F730/replay routes (POST also needs X-Requested-With):

- `GET /api/v1/registers/explorer`: all catalog metadata, observations/raw/decoded
  values, errors, source, verification, unknown entries and wire diagnostics.
- `GET /api/v1/device/capabilities`: explicitly false write/RMU/hardware-verification
  flags; active probing is supported for replay only.
- `POST /api/v1/discovery/probe` with `{"register_ids":[48852,40004]}`:
  explicit catalog-only reads, max 32, >=1 second spacing and one outstanding.
- `GET /api/v1/discovery/probe/status` and
  `POST /api/v1/discovery/probe/cancel`: bounded session status/cancellation.
- `POST /api/v1/registers/catalog/import`: offline version-1 F730 JSON (2 MiB max).
  Validated, process-local, writes disabled, clears prior observations. Use
  `--catalog` to load the same file on restart.

`--record-capture` creates new private JSONL plus `.summary.json` completeness
metadata; it never overwrites an existing file. Recording is bounded/best-effort,
with drops/failures in explorer diagnostics and the drained sidecar. Check that
`complete` is true before treating a recording as a complete replay. Replay
imports cap files at 8 MiB / 65,536 chunks; split long recordings. Timings measure
userspace receive/submission, not physical RS485 first-bit latency.

Repeatable hardware-free verification, including actual curl calls:

```bash
python3 crates/device-f730/tests/replay-http-smoke.py
python3 crates/device-f730/tests/serial-pty-smoke.py
```


Signed application OTA, the independent updater, developer deployment and expiring
diagnostics are documented in [OTA and diagnostics](../../docs/dev/ota-and-diagnostics.md).

## Wire resilience policy

The JSON config accepts `wire` with hypothesis defaults (ADRs 0090–0091):

```json
{"wire":{"reply_deadline_ms":10,"tolerated_consecutive_misses":3,"reopen_attempts":5,"reopen_backoff_ms":250,"frame_timeout_ms":60000,"thread_nice":-5}}
```

CLI device/bind settings still override config. A single late reply is counted;
an on-time reply clears the consecutive streak. The fourth consecutive miss or
UART error triggers bounded reopen with exponential backoff. Exhaustion exits
nonzero for systemd. No late reply is retried. `/health` reports `device_fatal`,
`device_degraded`, `wire_recovering`, nullable `last_frame_age_ms`, reopen count
and whether thread priority was applied. Recovery/fatal is HTTP 503; silence is
degraded HTTP 200 so the UI stays accessible. Only valid MODBUS40 frames refresh
age. Session C's independent watcher restores NibePi after >=60 s without one.
The ordinary shell may lack priority permission; the appliance unit grants the
bounded nice limit. Physical timing remains HP-21.

Import a private NibePi RAW log before replay (RX chunks only, no identity claim):

```sh
python3 scripts/import-nibepi-log.py /private/nibepi.log /private/f730.capture.jsonl
cargo run -p nibe-gateway -- --device replay --capture /private/f730.capture.jsonl --word-swap auto
python3 scripts/soak-local.py --duration-seconds 180 --report /tmp/nibe-soak.json
```

The importer follows upstream `backend.js`/`log.js` format, rejects backwards wall
clock changes, and preserves chunk boundaries. Inspect/sanitize payloads before
sharing; log receipt times do not establish UART timing. Soak defaults enforce
96 MiB per main gateway, 256 MiB combined monitored RSS and 16 MiB post-warmup
growth, with two SSE clients per mode and repeated signed-update smoke cycles.
