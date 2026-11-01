# Signed updates, deployment and diagnostics

The factory Pi image seeds `/var/lib/nibe-gateway/releases/slot-a`, `current`,
and a private root-owned `update-state/state.json`. App config remains in the
service user's private `state/`; update/rollback never replaces it. The immutable
updater is `/usr/lib/nibe-gateway/nibe-updater`; its independent OpenRC service
(`rc-service nibe-updater`, Alpine base) uses `/etc/nibe-gateway/release.pub` and a 0660 socket under `/run/nibe-updater`.
The gateway account has socket access and read/execute access to slots, no
write access to them. A failed update preserves the running slot. Uploading the
next valid release replaces the inactive slot and consumes its previous rollback
point; activate only after checking status. Interrupted switch/readiness/probation/rollback
is conservatively recovered to the previous release on updater restart.

Generate a release key once in a private location outside this checkout:

```bash
cargo run -p nibe-updater --bin nibe-release -- keygen /secure/nibe-release.key /secure/nibe-release.pub
# Back up the private key offline. Only the raw 32-byte public key goes on devices.
just build-pi
./scripts/build-frontend.sh
cargo run -p nibe-updater --bin nibe-release -- sign /secure/nibe-release.key 0.1.0 arm-unknown-linux-gnueabihf target/pi-musl/arm-unknown-linux-musleabihf/release/nibe-gateway apps/admin/dist /tmp/nibe-0.1.0.tar
# The release target label stays arm-unknown-linux-gnueabihf (the updater's platform id); the binary is static musl.
# OS image and bundle: docs/dev/image-build.md (scripts/build-alpine-os-bundle.sh, scripts/build-alpine-image.sh).
```

The manifest version must match the gateway's compiled workspace version. Bump
that version before publishing a new release. The compatibility identifier is
`nibe-gateway-app-v1`, and the schema range currently includes persistent config
v2. Signatures cover exact manifest JSON bytes; do not reformat signed JSON.
The outer tar contains `manifest.json`, raw 64-byte `manifest.sig` and `payload.tar`.
The payload contains only `nibe-gateway` and regular files below `admin/`.
Max artifact size is 48 MiB / 2048 regular entries; archives are uncompressed to
bound extraction and avoid decompression bombs. Keys must be raw 32-byte files;
the signer refuses private-key group/other permission bits. No private key is stored in CI (the manual pi-gen
image workflow was retired with Raspberry Pi OS; bundles and card images are built locally with the key).
The factory manifest is generated from image content, rooted in the image's trust.
Application OTA does not update the privileged updater, OS or boot firmware.

## Publishing a release on GitHub

Bump the workspace version, commit, then:

```bash
NIBE_SIGNING_KEY=~/.config/nibe-gateway/release.key scripts/publish-github-release.sh   # --draft to review first
```

It refuses a dirty tree or an existing tag, cross-builds, signs `target/releases/nibe-gateway-<version>.tar`,
tags `v<version>`, and creates the GitHub Release with the tar and its `.sha256`. A draft is invisible to
gateways until you publish it. The Updates page then shows the release after an admin presses
**Check for updates**. **Install x.y.z** (one confirmation) fetches the tar with the image's `curl` (HTTPS only),
hands it to the updater, which verifies it like an upload, and then activates it. The page shows download
progress (`GET /api/v1/updates/remote` → `staging`), then waits on `/health` for the new version and reloads.
The API keeps the steps separate (stage, then `POST /api/v1/updates/activate`). Only a release whose
`nibe-gateway-<version>.tar` asset sits in the configured repository and is newer than the running version is
offered. Pre-releases and drafts are ignored. `--update-repository OWNER/NAME` (or `"update_repository"` in the
JSON config) points a gateway at a fork; `none` / `null` turns the check off. Nothing is fetched in the background.

| Operation | Endpoint |
|---|---|
| Last check result (no network) | `GET /api/v1/updates/remote` |
| Ask GitHub for the latest release | `POST /api/v1/updates/remote/check` |
| Download and stage it | `POST /api/v1/updates/remote/stage` `{"version":"x.y.z"}` |

The Updates page uploads/stages, then separately asks for activation. Older signed
semantic versions require the explicit downgrade checkbox, upload query flag
`?confirm_downgrade=true`, or local updater client `--confirm-downgrade`.
A repeated manual rollback with the previous slot already active is rejected. All reads
and writes below require an admin cookie; every POST also requires
`X-Requested-With: nibe-gateway`. Initial setup proof cannot authorize these.

| Operation | Endpoint |
|---|---|
| Status / availability | `GET /api/v1/updates/status` |
| Verify and stage signed tar | `POST /api/v1/updates/upload`, `application/octet-stream` |
| Activate staged release | `POST /api/v1/updates/activate` |
| Restore previous application | `POST /api/v1/updates/rollback` |
| Restart gateway | `POST /api/v1/system/restart` |
| Read/authorize diagnostic lease | `GET/POST /api/v1/diagnostics/developer` |
| Sanitized support JSON | `GET /api/v1/diagnostics/support` |
| Sensitive replay capture (lease required) | `GET /api/v1/diagnostics/capture` |

Activation/rollback/restart return 202 after admission; activation/rollback intent
is durable before acknowledgment. The independent updater completes after the
HTTP process exits, revoking its sessions. Log in again and check the final phase:
`probation`, then `complete`, `rolled_back`, or `failed`. Probation ends
after 30 seconds of continuous readiness plus one error-free poll round, or after
`--probation-seconds` (600) of continuous readiness if no round completes.
An outage resets the window, 30 seconds unavailable or a fatal wire error causes
rollback, and `--probation-max-restarts` (default 2) limits process/boot changes.
An interrupted probation is conservatively rolled back at updater restart. `rolled_back` after activation is a failed
deployment, even when service is restored. Failed recovery of the old release
is `failed` / `rollback_not_ready`. Explicitly inspect and repair that case at the
console. A successful readiness probe requires live core heartbeat, version and
payload hash; an open TCP port alone is insufficient. `process_instance` changes
on every restart; `device_fatal` and wire recovery prevent readiness.
Config's `min_reader_version:2` permits future additive schemas without losing
unknown fields on rollback. Incompatible schemas remain fail-closed; ship a
reader that understands schema 3 before any release writes it. `/health` remains usable
without a session and reports `device_degraded` separately from app readiness.

For SSH development, first opt into a key-only, verified-host deployment account
and narrowly scoped updater-client sudo access. SSH is disabled in factory images;
these scripts do not enable it or grant privileges. No `sudo` password is accepted
interactively. Keep private keys local:

```bash
export NIBE_SSH_HOST=my-pi-zero
export NIBE_SIGNING_KEY=/secure/nibe-release.key
just deploy   # cross-build, UI build, sign locally, scp, stage, activate, readiness
just rollback # same updater + post-rollback health/release-identity check
just logs     # follow /var/log/messages over authenticated SSH
just debug    # read-only services, updater/OS state, health, network, memory, recent warnings; boot traces: scripts/pi-diagnostics.sh traces
```

The scripts enforce batch/key auth and strict host-key checking. `deploy` never
copies unsigned executables into slots and never replaces the trusted updater.
A bounded wait (`NIBE_UPDATE_WAIT_SECONDS`, default 2100) allows probation and
reports auto-rollback as deployment failure. The SSH health check
uses loopback without proxies and verifies the installed manifest identity.
Gateway HTTP debug authorization is still required for raw capture/dev API use;
SSH access does not turn off HTTP auth. In the Debug page authorize five minutes,
or with a valid cookie:

```bash
curl -fsS -b /tmp/nibe-cookies -H 'X-Requested-With: nibe-gateway' -H 'Content-Type: application/json' \
  -d '{"seconds":300,"confirmation":"AUTHORIZE_DIAGNOSTICS"}' http://127.0.0.1:8080/api/v1/diagnostics/developer
```

Use `seconds:0` to stop early. Leases max out at 900 seconds; verbose filters reset
within one second after expiry. Diagnostic authorization never enables hardware
commands. The gateway must have been started with `--record-capture NEW.jsonl`
for export. The recording path is fixed at startup and never supplied by HTTP;
The bounded recorder services the wire independently. Export snapshots contain
only complete lines and validate as capture-jsonl v1; a recording over 8 MiB must
be retrieved/reviewed through the authorized console. Captures retain potentially
sensitive device bytes and provenance, so review before sharing. Support JSON
excludes captures, raw logs, identity, arbitrary strings, network config and secrets.

Local verification requires no Pi, systemd, SSH or privileged host changes:

```bash
cargo test -p nibe-updater
./tests/integration/update-smoke.py
# Includes real TCP/auth, signed artifact, separate updater/fake service helper,
# real gateway process restarts, manual rollback, failed-readiness auto-rollback,
# v3 additive migration rollback, probation/restart guard, downgrade confirmation,
# persisted evidence, status, security headers, leases and secret canaries.
./scripts/smoke-updater-pi.py target/pi-musl/arm-unknown-linux-musleabihf/release/nibe-updater /tmp/nibe-empty-sysroot
```

`nibe-updater serve|recover` accepts explicit `--root`, `--key`, `--socket`,
`--target`, and loopback `--health-address`. Tests can supply an absolute
`--service-helper` executable instead of the init system; this is launch configuration,
never a remote/socket parameter. The `ServiceManager` and durable `Checkpoint`
traits support temp-root fake services and power-loss tests. Never run the
production updater as the gateway user or place its trust key in a writable slot.
Physical SD power cuts, update failure paths and serial timing remain in `HARDWARE_PENDING.md`.
