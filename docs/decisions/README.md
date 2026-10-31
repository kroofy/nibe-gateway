# Architecture decision records

The code says what the gateway does. An ADR says why it doesn't do the obvious other thing, so that nobody,
human or agent, undoes a decision without knowing its reasons.

## When to write one
Write or extend an ADR only when at least one of these holds:
1. **Hard to reverse:** formats on the card or on disk, the partition layout, the update and signing format, or
   anything a released gateway or a rollback depends on.
2. **A safety or security invariant:** what may change the pump, who may claim or administer the device, or what
   crosses a privilege boundary.
3. **A rejected obvious alternative:** a future agent would plausibly propose it again, and the reason against it
   is not visible in the code.

Everything else is not an ADR:
- How something works goes in code, tests and `docs/dev/`.
- Why one piece of code is written a certain way goes in a short comment next to it.
- What was done, and when, goes in the workstream log and `CHANGELOG.md`.

If a case is borderline, ask the owner instead of writing one. A new feature does not need an ADR by default.

## Rules
- One ADR per subject. Extend the ADR that already covers a subject instead of adding a new one.
- An ADR states the decision as it stands now. When a decision changes, rewrite the ADR and move the replaced
  choice under **Rejected**, with the reason it was dropped. Don't add amendment sections; git keeps the history.
- Delete ADRs that are superseded, retired or completed. Fold whatever reasoning still matters into the ADR that
  replaced them, and record the number below. Numbers are never reused.
- Keep it short: about 20 lines, and a merged subject no more than about 40. Longer usually means it is
  describing the implementation.
- No implementation detail: no paths, parameters, limits, commands, sizes or test results. A link to the research
  note behind a decision is fine.

## Template
```markdown
# ADR NNNN: <decision as a short statement>

Date: YYYY-MM-DD · Status: accepted (owner decision, if it was one)

## Context
The problem and the forces/constraints. Facts only.

## Decision
What was chosen, in one or a few sentences. No implementation detail.

## Why
The reasons this option wins.

## Rejected
Alternatives and why not, including earlier versions of this decision.

## Consequences
What gets easier or harder, and what follow-up it forces.
```

When an ADR has been rewritten, its date line reads `Date: <first decided> · Updated: <last rewrite>`.

## Index
| ADR | Decision |
|---|---|
| [0010](0010-ws01-register-catalog-sources.md) | Register catalog from NibePi and NIBE's export; conflicts recorded, not applied |
| [0020](0020-ws02-bounded-core-and-adapter-contract.md) | Hardware-independent core, cached state, isolated device adapters |
| [0021](0021-ws02-local-simulation-and-control-policy.md) | Deterministic simulation; simulated controls can never become real ones |
| [0030](0030-armv6-cross-toolchain.md) | Pinned user-level ARMv6 cross toolchain with a binary gate |
| [0031](0031-image-layout-and-supervision.md) | Read-only root, separate persistent data, health-gated supervision |
| [0040](0040-wire-task.md) | The gateway is a MODBUS40 accessory whose replies to the pump come first |
| [0041](0041-register-knowledge-layers.md) | Catalog claims, evidence and notes about registers stay in separate layers |
| [0050](0050-admin-static-client-and-unsupported-actions.md) | Small same-origin admin UI that never overstates what it knows |
| [0060](0060-first-owner-and-sessions.md) | No default password: first owner by boot file or open setup AP; sessions; login limits |
| [0070](0070-signed-updates-and-probation.md) | Signed A/B application updates, an independent updater, and probation that ends on evidence |
| [0071](0071-diagnostics-leases-and-support-allowlist.md) | Expiring diagnostic leases and allowlisted support reports |
| [0095](0095-minimum-config-reader.md) | Config declares the oldest version that may read it |
| [0099](0099-http-maintenance-guards.md) | Strict browser headers and deliberate downgrades |
| [0101](0101-read-requests-and-polling.md) | The gateway reads registers by asking, on the pump's free read tokens |
| [0105](0105-alarm-reset.md) | Dedicated alarm reset, and automatic reset of alarm 251 only |
| [0106](0106-os-ab-partitions-and-tryboot.md) | Full-system A/B slots with firmware tryboot |
| [0107](0107-persistent-system-state.md) | Persistent system state is declared in one list; no encryption at rest |
| [0110](0110-api-keys-and-access-page.md) | Scoped API keys, separate from the admin account |
| [0111](0111-privilege-separation.md) | The gateway is unprivileged; root work goes through small helpers with fixed operations |
| [0113](0113-data-and-controls-no-automation.md) | The gateway offers data and controls, not automation |
| [0114](0114-history-storage.md) | On-device history in SQLite, committed every minute |
| [0115](0115-mqtt-home-assistant.md) | Optional MQTT with Home Assistant discovery and a minimal built-in client |
| [0117](0117-alpine-base.md) | Alpine Linux base, built up from an explicit package list |
| [0118](0118-alpine-boot-and-openrc-services.md) | Alpine slot boot and services: every failure returns to a good slot and leaves evidence |
| [0123](0123-setpoint-leases.md) | A writer may lease a setpoint; the gateway restores it when the lease lapses |
| [0124](0124-github-releases-update-source.md) | GitHub Releases is an optional update transport; the signature stays the only trust |
| [0125](0125-register-access.md) | Any register can be read and written by number, through one write path |
| [0128](0128-audit-log.md) | The gateway keeps an audit log of every change |
| [0129](0129-first-boot-completes-the-card-layout.md) | The first boot of a fresh card completes its partition layout |

## Retired numbers
These numbers appear in older commits and release notes. Where the reasoning still matters, it now lives in the
ADR shown.

| Old | Now |
|---|---|
| 0011, 0090, 0091, 0100 | 0040 |
| 0098, 0119 | 0041 |
| 0097, 0108, 0130 | 0060 |
| 0096, 0109 | 0070 |
| 0102 | 0101 |
| 0120 | 0107 |
| 0092 | 0111 |
| 0112, 0116 | 0117 |
| 0127 | 0123 |
| 0104, 0126 | 0125 |
| 0012, 0093, 0094 | deleted: the NibePi migration is complete |
| 0103 | deleted: the WebSocket origin check is explained in the code |
| 0121, 0122 | deleted: the admin design system is documented in Storybook's Foundations page |
