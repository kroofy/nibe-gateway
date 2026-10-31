# ADR 0060: Per-device first-owner claim, local sessions, durable and reversible provisioning

Date: 2026-10-22 · Status: accepted

## Context
The device is a headless appliance on a home LAN with no cloud. Anyone on the LAN can reach it, and power cuts are
routine.

## Decision
- No default password: the first owner claims the device with a per-device secret.
- Passwords use a memory-hard hash.
- Cookie sessions with CSRF protection. The app does not terminate TLS.
- Configuration is written atomically to a private directory.
- Network changes are transactional and roll back on failure.
- A network outage never opens an access point automatically.

## Why
- Universal default passwords are the classic way appliances get compromised.
- Atomic writes, because power can go at any moment.
- A Wi-Fi change that strands a remote device costs a site visit, so it must roll back.
- An access point that opens on outage would let neighbours in.

## Rejected
TLS inside the app: certificates on a LAN appliance add complexity without a trust anchor. Use a proxy when it's
needed.

## Consequences
- Plain HTTP is for trusted networks.
- The first credential needs a physical handoff (later the boot-partition file, ADR 0108).
- Login limits: ADR 0097.
