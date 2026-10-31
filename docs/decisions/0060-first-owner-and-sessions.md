# ADR 0060: No default password: first owner by boot file or open setup AP; sessions; login limits

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
The device is a headless appliance on a home LAN, with no cloud and no label. Anyone on the LAN can reach it, the
owner is often remote, and power cuts are routine.

## Decision
- No default password. An unprovisioned device gets its first admin in one of two ways:
  - An optional provisioning file on the boot partition, written on a laptop. It is applied on first boot through
    the normal code paths, then deleted.
  - The setup access point, which is an open network. Over it, while no admin exists, setup needs no device
    secret. From any other network the per-device secret is required.
- Passwords use a memory-hard hash. Sessions use cookies with CSRF protection. The app does not terminate TLS.
- Logins back off per TCP peer address, with a global cap on hashing work. Forwarded headers are ignored.
- Configuration is written atomically. Network changes are transactional and roll back on failure. An outage
  never opens an access point.

## Why
- Universal default passwords are how appliances get compromised.
- Nothing on the device shows its secret to the owner. Being in radio range while the setup AP is up replaces that
  proof. Requiring the secret everywhere else stops a LAN device from claiming the Pi.
- Any laptop can write the boot partition. Deleting the file removes secrets from a filesystem without
  permissions.
- Per-address backoff stops one LAN client from locking the admin out. Forwarded headers can be spoofed, and the
  cap protects the single CPU.
- A Wi-Fi change that strands a remote device costs a site visit. An AP that opens on an outage would let the
  neighbours in.

## Rejected
- TLS inside the app: there is no trust anchor on a LAN appliance. Use a proxy when TLS is needed.
- A password on a label, a fixed default password, or the setup secret on the boot partition.
- One global login limit: any LAN client could lock the admin out.

## Consequences
- Anyone in radio range while the setup AP is up can claim an unprovisioned gateway first, and can read its
  plain-HTTP traffic. Create the admin right after the first boot or a factory reset.
- The device still generates an AP password that nobody uses, because older releases need it after a rollback.
- Clients behind one proxy share one login bucket.
