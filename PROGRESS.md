# NIBE Gateway — development progress

Brief: [docs/brief/00-index.md](docs/brief/00-index.md) · Agent rules: [AGENTS.md](AGENTS.md) · **Open hardware items: [docs/progress/HARDWARE_PENDING.md](docs/progress/HARDWARE_PENDING.md) (summary at the top)** · Original hardware test plan (mostly historical): [docs/hardware-test-plan.md](docs/hardware-test-plan.md) · Decisions: [docs/decisions/](docs/decisions/) · Overview: [README.md](README.md)

Status legend: `todo` · `in-progress` · `done` · `done-sim` (done locally, needs hardware verification) · `done-hw` (verified on the owner's Pi/F730; any leftovers are listed in HARDWARE_PENDING) · `blocked-hw` (cannot proceed without physical hardware) · `retired` (artifact removed from the repo or no longer applicable)

## Workstreams

| ID | Workstream | Milestone | Wave | Status | File |
|---|---|---|---|---|---|
| WS01 | Research & feasibility (NibePi, protocol, registers) | M0 | 1 | done-hw (HW-1…HW-4 confirmed on the F730 2026-10-24; NibePi-side checks retired) | [WS01](docs/progress/WS01-research.md) |
| WS02 | Core, simulator, API, gateway app | M1 | 1 | done-hw; heating-offset lease (ADR 0123) released as 0.1.25 and HP-42 steps 1–6 verified on the F730 2026-10-29; open register writes (ADR 0125) and register read/write on HTTP + MQTT (ADR 0126) done-sim 2026-10-31, HP-44; fault/power-cut checks still open | [WS02](docs/progress/WS02-core-api-sim.md) |
| WS03 | Tooling: justfile, CI, cross-compile, Alpine packaging | M4 | 1 | done-hw; pi-gen / Raspberry Pi OS / systemd parts **retired** 2026-10-25 | [WS03](docs/progress/WS03-tooling-packaging.md) |
| WS04 | Protocol crate, F730 adapter, replay, register catalog/discovery | M2 | 2 | done-hw; register notes (ADR 0119) live with the owner's seed (HP-41) | [WS04](docs/progress/WS04-protocol-f730.md) |
| WS05 | Admin UI (React/Vite) + Playwright | M1/M3 | 2 | done-hw (phone-width pass on the Pi: HP-19); design system + Storybook 2026-10-27 (ADR 0121) | [WS05](docs/progress/WS05-admin-ui.md) |
| WS06 | Network provisioning, auth, persistence | M3 | 2 | done-hw for file provisioning, station, mDNS, auth; setup AP done-sim (HP-13/14, HP-40) | [WS06](docs/progress/WS06-network-auth-persistence.md) |
| WS07 | Updater, OTA A/B, deploy/debug workflow | M5 | 3 | done-hw (app + OS A/B); failure-path tests deferred (HP-16/17/22); GitHub Releases update source done-sim (ADR 0124, HP-43) | [WS07](docs/progress/WS07-ota-deploy.md) |
| WS08 | Integration review, hardware test plan, docs | M6/M7 | 3 | done | [WS08](docs/progress/WS08-integration-review.md) |
| WS10 | Updater, auth, API and persistence hardening | M3/M5/M7 | 4 | done-hw for probation/auth; rollback and power-cut paths done-sim (HP-22) | [WS10](docs/progress/WS10.md) |
| WS09 | Runtime resilience, appliance profile and migration tooling | M4/M6/M7 | 4 | done-hw (wire policy, hardware/LAN profile); NibePi fallback watcher **retired** (HP-24) | [WS09](docs/progress/WS09.md) |

## Milestones

| Milestone | Status | Notes |
|---|---|---|
| M0 Research & feasibility | done | Feasible; source facts since confirmed on the F730 (HP-03/05/06 via HP-26). WS04 spec: docs/research/protocol-summary-for-implementers.md |
| M1 Local foundation | done | Clone, `cargo run -- --device simulated`, UI + API + OpenAPI, deterministic scenarios, contract/integration/Playwright tests, all without hardware |
| M2 Protocol & register system | done | Accepted 2026-10-24 with real F730 captures; 2026-10-25 register survey (576 readable registers answered) and display-photo verification of units/scales (HP-34) |
| M3 Provisioning & administration | done-hw | On the owner's Pi: boot-partition provisioning (HP-30), station Wi-Fi, mDNS, LAN auth, API keys and the Access page (ADR 0110/0111). Setup-AP onboarding and factory reset only in simulation (HP-13/14, HP-40) |
| M4 Pi appliance | done-hw | Alpine Linux in both A/B slots on the owner's Pi (ADR 0117/0118, HP-38), static musl gateway, 35 MiB RAM in use. The fresh-card image is built but not flashed (HP-40, bench) |
| M5 OTA & dev workflow | done-hw | App OTA with probation (0.1.1 → 0.1.23) and OS A/B updates with tryboot (HP-29, HP-35…HP-38) on hardware. Failure-path and power-cut tests deferred by the owner (HP-16/17/22) |
| M6 Real F730 integration | done-hw | Two-speed polling, WebSocket stream, history, writes 47011 / 45171 (0 → 1 edge) / 48132 verified (HP-27/28/31/32). MQTT tested against a Docker broker only. Disconnect-recovery and logic-analyser timing are nice to have (HP-07/21) |
| M7 Stabilization & release | in-progress | Live since 2026-10-24. Open: power-cut matrix, update failure tests, 24 h memory record, release/image publishing |

## What's left (2026-10-26)

The complete hardware list is the summary at the top of
[docs/progress/HARDWARE_PENDING.md](docs/progress/HARDWARE_PENDING.md). In short:
1. **Owner at the pump:** HP-34 leftovers: 43147 with a clamp meter, energy counters against myUplink (fan modes done 2026-10-26).
2. **Bench Pi / spare card:** HP-40 (flash the fresh Alpine card, setup AP, first OS update, live Wi-Fi fallback
   trigger), then HP-13/14 (AP onboarding, recovery, factory reset) and the Alpine supervisor watchdog (HP-11 / HP-38a step 2).
3. **Deferred by the owner:** OS- and app-update failure paths (HP-16, HP-22, HP-29/38 unhealthy slot and network
   gate) and the SD power-cut matrix (HP-17, HP-18, HP-33 step 5).
4. **Nice to have:** HP-07/21 physical timing and disconnect recovery, HP-08, HP-19/20/23 leftovers, HP-25 photos,
   history numbers (HP-33), MQTT against the owner's broker and Home Assistant, Access-page toggles under OpenRC.

**Firmware watchdog for OS trials** (proposal in ADR 0118 amendment, needs a small `os.rs` change and one supervised
test): closes the one trial failure mode that needed a power cycle.

**Open owner decisions:**
- Which control, if any, to verify next (one register at a time, ADR 0104/0113).
- Whether and when to point MQTT at the owner's broker (`.local` names do not resolve in the musl build; use an IP or DNS name).
- Public redistribution of the catalog text (ADR 0010): decided 2026-10-31, published with attribution and a NIBE disclaimer.
- Wi-Fi passphrase storage: decided to keep the passphrase as is (ADR 0120); revisit if the Pi joins a network
  whose passphrase is reused elsewhere.

**Could do locally now:** run the GitHub CI workflow remotely once (linted, never dispatched).

## Orchestrator log
- 2026-10-22: Brief split into `docs/brief/`, tracking created, wave 1 launched.
- 2026-10-22: WS02 done-sim (committed 66025fb). WS05 + WS06 launched early (depend only on WS02).
- 2026-10-22: WS03 done-sim (3e5db2d), WS01 done (6777090, feasible; hardware steps HW-0..HW-5). WS04 launched.
- 2026-10-22: WS05 done-sim (committed). WS06 done-sim (uncommitted; shares files with in-flight WS04, commit together). WS07 launched.

- 2026-10-22: WS04 done-sim / M2: bounded protocol/property tests, F730 catalog/decoding, isolated serial/replay/capture, authenticated explorer and read-probe framework. Fixture-to-state/curl/serial-PTY verification and workspace tests/clippy pass. Hardware probes and every write remain disabled; ADRs 0040–0041 and WS01-linked hardware handoff recorded.
- 2026-10-22: WS04 final shared-checkout recheck: own tests/clippy + replay/PTY curl pass; latest global tests/clippy temporarily fail in concurrent WS07 OpenAPI/debug expectations and updater-test unchecked read. Earlier WS04 workspace checkpoints passed; leave WS07 edits intact and rerun globally after that workstream settles.

- 2026-10-22: WS07 done-sim / M5: signed release tooling, independent A/B updater with readiness/automatic rollback and crash recovery, authenticated API/UI maintenance, SSH deploy/debug and expiring diagnostics/capture/support. Workspace default/all-feature tests + strict clippy, UI/browser, full real HTTP update/manual+automatic rollback, ARMv6/QEMU crypto/staging and image preparation verified. WS04's transient shared-checkout WS07 failures resolved; physical systemd/SSH/SD/F730 checks appended, no commits.
- 2026-10-22: WS08 done: code-level integration review against brief §6/§13/§16 and the protocol spec, with adversarial release-build/auth/secret/parser tests. Fixed F1 (decoder phantom nested frame inside escaped payload → mid-transmission ACK), F2 (frontend build script npm vs pnpm broke CI/image/deploy), F3 (no supported serial Pi build), F9/F12/F13. Open findings are in docs/progress/WS08-integration-review.md (F4 wire stop on deadline miss, F5 factory unit still simulated/loopback, F6 schema rollback, F7 no probation, F8 global login lockout). New docs/hardware-test-plan.md, README.md, deduplicated HARDWARE_PENDING (HP-01…HP-20). fmt/clippy/tests/UI/e2e/smokes pass; no commits.
- 2026-10-22: WS08 committed (887bee6). Wave 4 launched: WS09 (F4/F5/F11/F16, NibePi log importer, soak) and WS10 (F6/F7/F8/F10/F14/F15/F17/F20).
- 2026-10-23: WS10 done-sim: fixed WS08 F6/F7/F8/F10/F14/F15/F17/F20. Added minimum-reader schema compatibility with actual v3 rollback smoke, 600-second readiness probation and restart/boot rollback, source login backoff, atomic durable register evidence, explicit downgrade consent, all-response security headers, redundant rollback rejection and uptime/backend IP status/UI. ADRs 0095–0099; physical bench acceptance HP-22. Default/all-feature workspace tests, strict clippy, fmt, UI build, 12 browser checks, WS06/WS07 and default/dev HTTP smokes pass. SSH deploy wait now covers probation. No commits or pump writes.

- 2026-10-23: WS09 done-sim: fixed F4/F5/F11/F16; configurable reply/miss/reopen policy, UART priority and wire readiness/nonzero fatal exit; hardware/NM/LAN image profile with own-profile Polkit/Avahi and explicit simulated option; read-only-root NibePi fallback; source-format RAW importer. ADRs 0090–0094; hardware checks HP-21/23/24. Final 180-second simulated/replay soak: four SSE clients, three signed-update cycles, sampled peak 122.97 MiB monitored RSS, 16/20 KiB gateway growth. Default/all-feature workspace tests, strict clippy/fmt, UI build/12 browser checks, smokes, unit/Polkit/stage lint and ARMv6/QEMU pass. pi-gen preparation passes; full image generation still needs rootful Docker. No commits or pump writes.
- 2026-10-22: WS09+WS10 done-sim, committed 3dd606a. Workspace tests/clippy/fmt/UI re-verified by orchestrator.
- 2026-10-24: Bench session (no workstream; owner remote, F730 not connected). Stock Pi OS Trixie card for the spare Pi Zero W, ARMv6 gateway installed and simulated check passed, UART free/PL011 verified. HAT identified as Waveshare RS485 CAN HAT (ADR 0100, inventory-2026-10-24.md). New HP-25 (board/wiring photos) and HP-26 (HP-05 on the bench card). Next: image the NibePi card (HP-02), then HP-25, then the supervised read-only capture. No commits, no pump traffic.
- 2026-10-24: First real F730 traffic. With the Pi on SHIM power at the pump, a receive-only listen captured 60 s (`real-f730-passive-2026-10-24.capture.jsonl`, replay test added): F730 Cu 3x400V, version 9721, 1 s tokens, **empty LOG.SET data frames**, 0xEE version requests to 0x20. Consequence: the ACK-only trial yields identity only; register values need hardware read requests (currently blocked) or a pump LOG.SET. Owner decision needed. No transmissions, no commits.
- 2026-10-24: ADR 0101 (owner decision): `--allow-hardware-reads` opt-in lets the authenticated probe API send bounded read requests on hardware, including reads of settings; writes still impossible. Tests for the opt-in, the read-request wire path and the real-capture replay added; default/all-feature tests, strict clippy and fmt pass. No live run yet, no commits.
- 2026-10-24: M2 accepted with real captures; HP-02 waived (NibePi card untouched). Initial checkpoint commit of the bench session by owner request.
- 2026-10-24: M6 features: polling, `/api/v1/stream` WebSocket, gated writes (named controls only); heat offset verified on the F730 (−2→−1→−2). Playwright is not installed on this host since the move, so browser e2e was not run; UI typecheck/build pass.
- 2026-10-24: Alarm reset verified (251 → 0, owner confirmed); auto-reset of 251 enabled. Argon2 moved to a nice-19 thread after logins caused deadline misses; re-verified on the Pi.
- 2026-10-24: Appliance `hardware.json` now enables hardware reads, the 24-register poll list, the verified controls (47011, alarm reset + auto 251) and `stale_after_ms` 120 s. Release key generated on the build host (`~/.config/nibe-gateway/`, private, never committed); pi-gen tree prepared; rootful build pending one owner sudo step.
- 2026-10-24: First pi-gen appliance image built on rikarchy (rootful Docker, one owner sudo step). Owner decisions: OS A/B updates with tryboot (ADR 0106), declared persistent system state on p3, no encryption at rest (ADR 0107), boot-partition provisioning with Wi-Fi/admin/SSH/Tailscale (ADR 0108). Implemented: p4 slot B, per-slot boot dirs, `nibe-updater os-install/os-status/os-confirm`, `nibe-release os-sign`, `scripts/build-os-bundle.sh`, provisioning service + gateway import, pinned Tailscale. Gateway provisioning verified in simulation; image-side pending HP-29/HP-30.
- 2026-10-24: Appliance image flashed on the reused NibePi card with `nibe-provision.json` (Wi-Fi, admin, 2 SSH keys, Tailscale), pending the swap at the pump (HP-29/30). Owner's NibePi used MQTT+HA discovery and hot-water auto-luxury. 2026-10-25: automations are not ported (ADR 0113); MQTT+HA is next, 48132 becomes a plain control (HP-32).
- 2026-10-25: ADR 0109 (proposed) added to the plan: evidence-based, criteria-driven update probation.
- 2026-10-25: HP-29 passed: first A/B OS update on the Pi Zero W (tryboot trial, health-checked commit, persists across reboot). Three app OTAs (0.1.1–0.1.3) completed. Planned next: slim image (~600–700 MB less root, ~1 GB bundles), root helper for SSH/Tailscale (ADR 0111).
- 2026-10-25: Slim OS (ADR 0112) installed over the air B → A in 4 min (bundle 814 MB, root 590 MiB used, was 1581) and committed; nibe-system helper live on hardware. 0.1.6: reload banner for outdated tabs, Access layout fixes. Playwright restored (npm global; runner resolves symlinked CLI): 12/12 browser checks pass again.
- 2026-10-25 (bookkeeping summary of the day; owner remote, all on the owner's Pi at the F730): alarm 251 reset fixed as a 0 → 1 edge on 45171 (0.1.8, HP-31); writes 47011, 45171 and 48132 Temporary Lux verified (HP-27/28/31/32; no automations, ADR 0113); API keys, Access page and root helper (ADR 0110/0111); MQTT + Home Assistant discovery (0.1.10, ADR 0115; Docker broker only); history in SQLite with a History page (ADR 0114, 0.1.14); register survey of all 576 readable registers and display-photo verification (HP-34), with two-speed polling (0.1.12); evidence-based probation (ADR 0109). OS: first A/B update (HP-29), slim image (ADR 0112), further slimming, Python removed and a squashfs root (ADR 0116, HP-35…HP-37), then **Alpine Linux in both slots** (ADR 0117/0118, HP-38): static musl gateway, RAM 81 → 35 MiB, OS bundles 2.15 GB → ~52 MiB. Raspberry Pi OS, pi-gen and the NibePi card are retired. Clock follows the boot-time NTP step (0.1.22, HP-39). Wi-Fi fallback installed with alpine.7 but not yet triggered; fresh-card image built, not flashed (HP-40, deferred). Bookkeeping pass: HARDWARE_PENDING classified (7 verified, 5 retired, 14 open of 26 items) with an open-items summary; statuses, milestones and stale living docs updated.
- 2026-10-26: Fan and airflow on the owner's F730 (read-only probes; owner switched a temporary speed 3 in
  myUplink): 47260 and 43108 follow the active speed step (43108 is the step, not %), airflow 40050 scales with the
  fan (~3.7 per %) and also shows frost/defrost; 41256–41258 unused on 9721; 43124 only while the compressor runs.
  Register notes (ADR 0119, 0.1.23) with the owner's seed applied on the Pi (HP-41: 576 notes, 42 display-verified);
  catalog fixes 43108 unit and 40051 scale. Bookkeeping pass (HARDWARE_PENDING classified, living docs updated).
  Wi-Fi PSK as a derived WPA2 key considered and not adopted (ADR 0120). Gateway 0.1.23 on Alpine alpine.7.
- 2026-10-26: ADR cleanup. All 44 ADRs were rewritten to record only why: context, decision, rejected alternatives
  and consequences. Implementation detail was removed (old texts are in git history). Superseded and retired ADRs
  are marked as such: 0012/0093 NibePi, 0112/0116 Raspberry Pi OS base, 0096's completion rule. Added the index and
  template in `docs/decisions/README.md`. AGENTS.md rule 7 now states what belongs in an ADR. Numbers and filenames
  are unchanged, so existing links still hold. Documentation only; no code changed.
- 2026-10-27: Admin UI design system and Storybook (ADR 0121), role-based colour tokens with AA contrast
  (ADR 0122), negative heating offset typable. Released and deployed as **0.1.24** to the owner's Pi (app OTA,
  probation passed, 0.1.23 kept for rollback); live health, assets and readings verified.
- 2026-10-29: Heating-offset control lease (ADR 0123, owner decision): a leased write is restored to the read-back
  baseline if the writer stops renewing, also across restarts; second exception to ADR 0113. Done in simulation
  (tests and curl run); hardware test HP-42 pending. No Ferroamp/battery coupling.
- 2026-10-29: WS02 lease review hardening done-sim: serialized HTTP/MQTT/restore, durable intent before dispatch,
  terminal outcome plus subsequent read-back confirmation, confirmed baseline gating; all requested checks pass.
  Physical pump and power-cut acceptance remains HP-42. No commits.
- 2026-10-29: **0.1.25 deployed** to the owner's Pi (signed app OTA, slot a → b, probation complete, 0.1.24 kept).
  HP-42 steps 1–6 verified on the live F730: leased −3, lapse/renewal/restart-after-expiry all restored −2 with a
  post-write read-back; an unleased write ends the lease. Pump left at −2. First OTA attempt rolled back (stale
  binary signed; see WS02 log).
- 2026-10-31: Preparing the public repository: MIT `LICENSE`, README license/disclaimer/security section, catalog
  attribution and NIBE disclaimer, owner LAN addresses and an SSH key comment replaced with generic values. GitHub
  Releases as an admin-initiated update source (ADR 0124, WS07, HP-43) and `scripts/publish-github-release.sh`.
  Version 0.1.26.
- 2026-10-31: Repository public with one squashed commit (full history kept locally only). v0.1.26 published on
  GitHub Releases; 0.1.27 adds neutral "Online updates" wording and is the version to install on the Pi first
  (manual upload), after which later releases appear under Updates → Check for updates (HP-43).
