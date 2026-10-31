# Architecture decision records

An ADR records **why** a decision was made: the forces at play, what was chosen, the alternatives rejected, and
what follows from it. It exists so that nobody, human or agent, undoes a decision without knowing its reasons.

An ADR is **not** documentation of how something works. Paths, file names, parameters, limits, commands, sizes
and verification results belong in code, tests, `docs/dev/`, `docs/research/` or the progress files. They change
as the implementation changes, and ADR copies of them go stale.

## Rules
- About 20 lines. If it is longer, it is probably describing the implementation.
- Never rewrite a decision in place. Add a short dated amendment (why it changed), or write a new ADR and mark the
  old one `superseded by NNNN` / `retired`.
- Keep numbers and filenames stable: code comments and docs link to them.
- Status values: `proposed`, `accepted`, `superseded by NNNN`, `retired`, `completed`.

## Template
```markdown
# ADR NNNN: <decision as a short statement>

Date: YYYY-MM-DD · Status: accepted

## Context
The problem and the forces/constraints. Facts only.

## Decision
What was chosen, in one or a few sentences. No implementation detail.

## Why
The reasons this option wins.

## Rejected
Alternatives and why not (optional).

## Consequences
What gets easier or harder, and what follow-up it forces.
```

Texts before 2026-10-26 contained implementation detail that has since been removed. The old versions are in
git history (`git log -- docs/decisions`).

## Index
| ADR | Decision | Status |
|---|---|---|
| [0010](0010-ws01-register-catalog-sources.md) | Register catalog from NibePi and NIBE's export; conflicts recorded, not applied | accepted |
| [0011](0011-ws01-read-only-modbus40-emulation.md) | Emulate a read-only MODBUS40 accessory | accepted; deliberately relaxed by ADR 0101 (reads), 0104 (named writes), 0105 (alarm reset) |
| [0012](0012-ws01-nibepi-migration-strategy.md) | Migrate away from NibePi in reversible phases | completed (NibePi retired 2026-10-25) |
| [0020](0020-ws02-bounded-core-and-adapter-contract.md) | Hardware-independent core, cached state, isolated device adapters | accepted |
| [0021](0021-ws02-local-simulation-and-control-policy.md) | Deterministic simulation; simulated controls can never become real ones | accepted |
| [0030](0030-armv6-cross-toolchain.md) | Pinned user-level ARMv6 cross toolchain with a binary gate | accepted; libc target superseded by ADR 0117 (static musl) |
| [0031](0031-image-layout-and-supervision.md) | Read-only root, separate persistent data, health-gated supervision | accepted; partition layout extended by ADR 0106, base OS replaced by ADR 0117 |
| [0040](0040-ws04-read-only-wire-task.md) | The wire task is isolated, bounded and prefers replies over data | accepted; recovery refined by ADR 0090, priority by ADR 0091, writes added by ADR 0104 |
| [0041](0041-ws04-catalog-decoding-and-discovery.md) | Catalog claims, observations and confirmed support stay separate | accepted; hardware read gate relaxed by ADR 0101 |
| [0050](0050-admin-static-client-and-unsupported-actions.md) | Small same-origin admin UI that never overstates what it knows | accepted |
| [0060](0060-ws06-auth-and-durable-provisioning.md) | Per-device first-owner claim, local sessions, durable and reversible provisioning | accepted |
| [0070](0070-signed-application-slots-and-updater.md) | Signed A/B application slots with an independent, privileged updater | accepted; OS updates in ADR 0106, probation in ADR 0096/0109 |
| [0071](0071-diagnostics-leases-and-support-allowlist.md) | Expiring diagnostic leases and allowlisted support reports | accepted |
| [0090](0090-wire-deadlines-and-recovery.md) | Tolerate isolated reply misses; bounded UART recovery, then escalate | accepted |
| [0091](0091-wire-thread-priority.md) | A modest scheduling advantage for the UART thread, not realtime | accepted |
| [0092](0092-hardware-appliance-profile.md) | Images boot as the hardware/LAN appliance; Wi-Fi does not depend on the gateway | accepted; amended 2026-10-25 (Wi-Fi fallback) |
| [0093](0093-nibepi-runtime-fallback.md) | Automatic return to NibePi during the migration trial | retired (NibePi retired 2026-10-25) |
| [0094](0094-migration-captures-and-local-soak.md) | Faithful capture import and an explicit local resource budget | accepted; NibePi import is historical (NibePi retired) |
| [0095](0095-minimum-config-reader.md) | Config declares the oldest version that may read it | accepted |
| [0096](0096-update-probation.md) | Probation after activation, with conservative rollback | accepted; completion rule superseded by ADR 0109 |
| [0097](0097-source-login-limits.md) | Login backoff per source address, plus a global cap | accepted |
| [0098](0098-register-evidence-overlay.md) | Durable per-installation register evidence | accepted |
| [0099](0099-http-maintenance-guards.md) | Deliberate downgrades, browser hardening headers, truthful status | accepted |
| [0100](0100-waveshare-rs485-can-hat.md) | Treat the owner's Waveshare RS485 CAN HAT as auto-direction | accepted |
| [0101](0101-opt-in-hardware-read-requests.md) | Opt-in read requests on the live bus | accepted (owner decision); relaxes ADR 0011 and 0041 |
| [0102](0102-continuous-register-polling.md) | Continuous polling on free read tokens | accepted; builds on ADR 0101 |
| [0103](0103-websocket-reading-stream.md) | Read-only WebSocket stream of readings for integrations | accepted |
| [0104](0104-gated-hardware-writes.md) | Writes only through named, individually verified and enabled controls | accepted (owner decision); relaxes ADR 0011; named-only rule superseded by ADR 0125 (gates still apply with open writes off) |
| [0105](0105-alarm-reset.md) | Dedicated alarm reset, and automatic reset of alarm 251 only | accepted (owner decision); amended 2026-10-25 |
| [0106](0106-os-ab-partitions-and-tryboot.md) | Full-system A/B slots with firmware tryboot | accepted (owner decision), verified on hardware (HP-29) |
| [0107](0107-persistent-system-state.md) | Persistent system state is declared in one list; no encryption at rest | accepted (owner decision) |
| [0108](0108-boot-partition-provisioning.md) | Optional provisioning file on the boot partition | accepted (owner decision) |
| [0109](0109-evidence-based-probation.md) | Probation ends on evidence, not on a timer | accepted (owner decision); supersedes the completion rule of ADR 0096 |
| [0110](0110-api-keys-and-access-page.md) | Scoped API keys, separate from the admin account | accepted (owner decision); amended 2026-10-25 |
| [0111](0111-system-settings-helper.md) | Separate root helper for system settings | accepted, live on hardware |
| [0112](0112-slim-appliance-image.md) | Slim the Raspberry Pi OS root | superseded by ADR 0117 |
| [0113](0113-data-and-controls-no-automation.md) | The gateway offers data and controls, not automation | accepted (owner decision); exceptions ADR 0105, 0123 |
| [0114](0114-history-storage.md) | On-device history in SQLite, committed every minute | accepted (owner decision) |
| [0115](0115-mqtt-home-assistant.md) | Optional MQTT with Home Assistant discovery | accepted (owner decision); "no register-by-number topic" amended by ADR 0126 |
| [0116](0116-further-image-slimming.md) | Further slimming of Raspberry Pi OS, squashfs root, no Python | superseded by ADR 0117 (squashfs root and no Python carried over) |
| [0117](0117-alpine-base.md) | Alpine Linux base, built up from an explicit package list | accepted, live on hardware (HP-38); supersedes ADR 0112/0116's base |
| [0118](0118-alpine-boot-and-openrc-services.md) | Alpine slot boot and services: every failure returns to a good slot and leaves evidence | accepted, live on hardware (HP-38) |
| [0119](0119-register-notes.md) | Per-installation register notes, separate from the catalog and the evidence | accepted |
| [0120](0120-wifi-passphrase-storage.md) | Keep storing the Wi-Fi passphrase (derived WPA2 key considered, not adopted) | accepted (owner decision) |
| [0121](0121-admin-design-system-and-storybook.md) | The admin UI is built from a documented component library | accepted |
| [0122](0122-colour-tokens-and-contrast.md) | One role-based colour palette, with all text readable to WCAG AA | accepted |
| [0123](0123-heating-offset-control-lease.md) | A writer may lease the heating offset; the gateway restores it when the lease lapses | accepted (owner decision); second exception to ADR 0113; durable-intent and rejected-supersession amendments 2026-10-29 |
| [0124](0124-github-releases-update-source.md) | GitHub Releases is an optional update transport; the signature stays the only trust | accepted (owner decision); amendments 2026-10-31: one-action install; fresh-card image and changelog per release |
| [0125](0125-open-register-writes.md) | Every register is writable through the API; the caller decides what to write | accepted (owner decision); supersedes ADR 0104's named-only rule |
| [0126](0126-register-access-on-every-surface.md) | Every integration surface reads and writes registers by number | accepted (owner decision); extends ADR 0125 to MQTT, amends ADR 0115; runtime poll choices amendment 2026-10-31 |
| [0127](0127-setpoint-leases.md) | The heating-offset lease extends to a few setpoint registers | accepted (owner decision); extends ADR 0123 |
| [0128](0128-audit-log.md) | The gateway keeps an audit log of every change | accepted (owner request) |
