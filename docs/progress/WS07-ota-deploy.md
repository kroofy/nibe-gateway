# WS07 — Updater, OTA A/B, dev deploy & diagnostics

- **Milestone:** M5
- **Status:** done-hw (app OTA and OS A/B on the owner's Pi); failure-path tests deferred
- **Owns:** crates/updater, scripts/deploy*, scripts/rollback*, update endpoints
- **Brief:** [11-ota-and-recovery.md](../brief/11-ota-and-recovery.md), [12-deploy-and-diagnostics.md](../brief/12-deploy-and-diagnostics.md), [13-image-and-filesystem.md](../brief/13-image-and-filesystem.md), [14-security.md](../brief/14-security.md)

## Checklist
- [x] Release artifact format: tarball + manifest (version, compat, sha256) + ed25519 signature; signing tool
- [x] updater binary: verify, stage into inactive slot, switch symlink atomically, restart, readiness check, auto-rollback; state in update-state/
- [x] Gateway update endpoints (upload, status, activate, rollback) — authenticated
- [x] just deploy / rollback / logs / debug scripts over SSH; health check after deploy
- [x] Diagnostics: temporary verbose logs (auto-expire), serial capture export, sanitized support bundle (no secrets) — tested
- [x] Fault tests: corrupt artifact, bad signature, failed readiness → rollback, power loss mid-stage (simulated)
- [x] GitHub Releases as an admin-initiated update source + `scripts/publish-github-release.sh` (done-hw; HP-43 verified 2026-10-31)

## Verify
Run from the repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir apps/admin build
pnpm --dir apps/admin test:e2e
./tests/integration/ws07-update-smoke.py
./tests/integration/http-smoke.sh default
./tests/integration/http-smoke.sh dev
./tests/integration/ws06-http-smoke.sh
./scripts/check-tooling.sh
mise exec aqua:rhysd/actionlint@1.7.12 shellcheck@0.10.0 -- actionlint
mise exec shellcheck@0.10.0 -- shellcheck -x scripts/*.sh packaging/raspberry-pi/stage-nibe/00-install/00-run.sh packaging/raspberry-pi/export-image/prerun.sh
./scripts/build-pi.sh
./scripts/smoke-updater-pi.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-updater /tmp/nibe-ws03-sysroot
./scripts/smoke-pi.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway /tmp/nibe-ws03-sysroot
# Use an actual trusted public key for a distributable image:
NIBE_RELEASE_PUBLIC_KEY=/private/release.pub ./scripts/build-image.sh --prepare-only
git diff --check
```

The full HTTP test creates private temporary keys/state and a separate updater,
uses actual signed tar uploads and gateway process restarts via a fake service
manager, verifies B activation/manual rollback, then a signed crashing binary's
automatic rollback. It asserts admin/CSRF enforcement, restart session revocation,
configuration preservation, actual verbose logging expiry, WS04 replay capture
export/lease expiry and exclusion of stored/cookie/setup secret canaries.
Rust tests cover 16 simulated crash boundaries, corrupt artifacts/manifests,
incorrect signatures, signed incompatible schema versions/targets, unsafe signed
archive entries, missing/corrupt state, concurrent updater exclusion and strict
readiness version/hash/JSON semantics. Core health uses a monotonic heartbeat,
independent of simulator time and physical freshness.

Both ARMv6 binaries pass EABI5 hard-float/glibc-2.28 ELF gates. QEMU ARM1176 runs
real Ed25519/SHA256 verification + inactive-slot staging and authenticated gateway
setup/health/status/SIGTERM. Browser suite passes all 10 checks, including truthful
updater absence and support download. pi-gen preparation includes updater, public
key, factory manifest, seeded slot-a/journal and both units. Unit syntax is verified
with the host-built updater path substituted for its image-only ExecStart path;
actual production systemd execution remains pending. See
[OTA and diagnostics](../dev/ota-and-diagnostics.md) for format, endpoints,
key management, SSH prerequisites and exact console commands.

## Open issues / hardware-pending
- Physical systemd/D-Bus/hardening, key-only SSH deploy, image boot, SD-controller
  durability/power cuts, memory and F730 timing require the three append-only WS07
  entries in `HARDWARE_PENDING.md`. No physical hardware was available.
- Full pi-gen image generation remains blocked by this host's rootless Docker;
  preparation passed with an explicit disposable test public key. A distributable
  image must use its real trust key. No OS/rootfs/updater OTA or boot rollback is
  claimed; only application slots are updated.
- Debug capture export requires an explicitly configured WS04 recording path;
  raw serial captures remain sensitive and separate from the sanitized report.
- SSH remains disabled in factory images; developer deploy requires an explicitly
  installed account/key, known-host entry and scoped updater-client sudo rights.
  Local tests do not claim actual SSH or physical radio/pump integration.

## Log

- 2026-10-22: Implemented version-1 signed release format and `nibe-release`
  keygen/sign tools; pure-Rust Ed25519/SHA256 and bounded tar validation. Added
  standalone root-owned `nibe-updater` with fixed socket operations, single
  mutation admission/process lock, fsync/rename journal, A/B staging/atomic link
  switch, independent restart/readiness/rollback, conservative boot recovery,
  missing/corrupt journal refusal and temp-root/fake-service seams. Durable intent
  precedes activation acknowledgment; work survives HTTP process/client exit.
- 2026-10-22: Added authenticated CSRF-protected update/status/activate/rollback,
  restart, explicit expiring diagnostics, sensitive WS04 capture export and
  allowlisted sanitized support endpoints. Updated admin Updates/Diagnostics/Debug
  against those endpoints, enabled WS06 secured recovery, regenerated OpenAPI/TS
  schemas, and kept command/hardware safety intact. Core readiness now checks a
  recent monotonic heartbeat and live worker; physical faults remain degraded.
  Verbose tracing is reloadable, expires within one second and logs only counters
  and connection enum. Logout/reset/restart revoke diagnostic authorization.
- 2026-10-22: Filled SSH deploy/rollback recipes with local signing, strict key/
  host verification, independent updater activation and post-operation health/
  release-ID checks. Updated WS03 cross-build, CI artifact, systemd supervisor,
  pi-gen updater/key installation and persistent slot-a/state initialization.
  Extended the ARM1176 gateway smoke to use WS06 auth/private temporary state;
  added real ARM1176 updater crypto/staging smoke. `docs/dev/ota-and-diagnostics.md` records trust, recovery and diagnostics policy.
- 2026-10-22: Default/all-feature workspace tests, strict clippy, formatting,
  frontend build, 10 real Chromium checks, all existing curl suites, new full
  HTTP update/manual+automatic rollback suite, 3 tooling tests, ShellCheck,
  actionlint, both ARMv6 ELF gates/QEMU smokes and pi-gen prepare pass. The final
  full HTTP run also proves actual verbose output stops after lease expiry.
  Shared-path exceptions were small additive edits to core heartbeat/logging,
  API/app/auth integration, admin UI, workspace/lockfile, contract/scenario/e2e
  fixtures and WS03 packaging/tooling. Re-read shared files before edits;
  preserved WS04/WS06 work and never changed raw-write/hardware gates. The
  transient WS07 contract/clippy failures recorded by WS04 are resolved.
  No commits, no hardware actions; precise remaining bench steps appended.
- 2026-10-25 (Alpine base, Rust side): new `nibe_system::init` (runtime systemd/OpenRC detection via
  `/run/systemd/system`, fixed command lists) used by the updater (gateway restart, os-install/os-confirm reboots,
  remote-access check) and the root helper (SSH/Tailscale start/stop + OpenRC runlevel). OpenRC tryboot = a detached
  child (so os-install exits 0 over SSH, like `systemctl reboot`) stops the gateway (60 s bound), syncs and
  calls reboot(2) `LINUX_REBOOT_CMD_RESTART2 "0 tryboot"`; systemd paths unchanged.
  `nibe-system supervise` enforces its own health watchdog without `NOTIFY_SOCKET` (`NIBE_START_TIMEOUT_SEC` 60,
  `NIBE_WATCHDOG_SEC` 30, SIGTERM then SIGKILL after 10 s, exit 1). `/usr/bin/mount|sync` → `/bin/mount|sync`
  (busybox paths; same binaries on merged-/usr Bookworm). `NIBE_PI_LIBC=musl scripts/build-pi.sh` builds static
  `arm-unknown-linux-musleabihf` binaries into `target/pi-musl`; `check-pi-elf.py --libc musl` checks them.
  Verify: `cargo test --workspace`, `python3 -m unittest discover -s scripts/tests`,
  `NIBE_PI_LIBC=musl scripts/build-pi.sh`, then the qemu smoke in HP-38a. Open: HP-38a on hardware; `.local`
  names do not resolve in musl builds (no nss-mdns).
- 2026-10-25 (bookkeeping): first A/B OS update with tryboot on hardware (HP-29), then slim, squashfs and Alpine bundles (HP-35…HP-38). The OpenRC paths are verified: app OTA restart and os-install from Alpine (HP-38a steps 4–5). Evidence-based probation; app OTAs up to 0.1.22. Open (deferred by the owner): unhealthy-slot and network-gate OS failures, `just rollback` on hardware, bad-signature and probation rollback (HP-16/22), power-cut matrix (HP-17).
- 2026-10-31 (done-sim): GitHub Releases as an update source. `crates/api/src/releases.rs` checks
  `https://api.github.com/repos/<repo>/releases/latest` only on **Check for updates**, offers a strictly newer
  `nibe-gateway-<version>.tar` asset from the same repository, and **Download and stage** fetches it with the image's
  `curl` (HTTPS-only, CA-verified, size-bounded) and passes it to the updater's normal upload path. Activation stays
  separate; downgrades stay manual. New `--update-repository OWNER/NAME|none` (default `kroofy/nibe-gateway`),
  endpoints `GET /api/v1/updates/remote`, `POST …/remote/check`, `POST …/remote/stage`; Updates page panel; Activate
  button moved under "Installed software". `scripts/publish-github-release.sh` builds, signs, tags and creates the
  release. Version 0.1.26. Verify: `cargo test -p nibe-api --lib releases`, `cargo test --workspace --all-features`,
  `./scripts/build-frontend.sh`, Storybook verify; a simulated gateway returned `unreachable` while the repo was
  private (GitHub 404), as designed. Open: HP-43 (first real download/stage/activate on the Pi). Shared-path edits:
  apps/gateway config/main, admin UI Updates page and stories, OpenAPI snapshots, schema.ts.
- 2026-10-31: HP-43 verified on the owner's Pi (0.1.27 → 0.1.28 via Online updates). 0.1.30 adds staging progress:
  `RemoteStatus.staging` (`downloading` with received/total bytes, then `verifying` until the updater answers),
  counted by the curl reader; the Updates page polls it once a second and shows a `ProgressBar` (new design-system
  primitive with stories). Verify: `cargo test -p nibe-api --lib releases`, Storybook "Updates / Downloading a
  release" and "Updates / Verifying a downloaded release".
- 2026-10-31 (owner feedback): Updates page redesigned around one **Software update** panel:
  installed version, plain-language status, latest version, one **Install x.y.z** button (download → verify →
  activate after one confirmation) with progress through the restart; the page waits on `/health`, reloads into the
  new UI, and shows the outcome (checking / complete / restored). The started update is kept in sessionStorage so
  the login screen can explain the restart. Upload moved under "Install from a file", updater internals under
  "Technical details". The new flow is first exercised on hardware when the Pi updates from 0.1.30 to a later release.

### 2026-10-31 — Releases carry the fresh-card image and a changelog
- `CHANGELOG.md` (sections 0.1.26–0.1.31). `scripts/changelog-notes.py VERSION` prints the section plus install
  hints and fails without one; `publish-github-release.sh` checks it before building, then also builds the OS bundle
  and fresh-card image of the same version, runs `inspect-alpine-image.sh`, and attaches the `.img.xz` + `.sha256`
  with the changelog as notes (no more `--generate-notes`). The Updates page is unaffected: it matches only
  `nibe-gateway-<version>.tar` by exact name.
- v0.1.31 (already published): image built from the tagged commit and uploaded; notes of 0.1.26–0.1.31 replaced by
  their changelog sections.
- Verify: `python3 scripts/changelog-notes.py 0.1.31` (prints the section), `… 9.9.9` (exit 1);
  `bash -n scripts/publish-github-release.sh`; image `scripts/inspect-alpine-image.sh` and
  `scripts/qemu-boot-alpine-image.sh target/alpine/nibe-gateway-alpine-0.1.31.img.xz` (all PASS lines; gateway
  health, UART and Wi-Fi are not covered in QEMU).
- Open: the published images have not been flashed on a real Pi yet (HP-40).
