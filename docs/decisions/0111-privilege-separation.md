# ADR 0111: The gateway is unprivileged; root work goes through small helpers with fixed operations

Date: 2026-10-23 · Updated: 2026-10-31 · Status: accepted

## Context
The gateway faces the LAN. It still has to manage its Wi-Fi profiles, SSH keys, Tailscale and updates. A gateway
that cannot start must not leave a remote Pi offline.

## Decision
- The gateway runs as an unprivileged service account. Its network rights cover only its own profiles.
- Root work goes through small helpers (the updater and the system-settings helper). Each accepts a fixed,
  validated set of operations over a local socket and never runs free-form commands.
- The system helper refuses to disconnect Tailscale when the request itself arrives over Tailscale.
- A small root fallback brings up a saved station profile when nothing is connected and the gateway is unhealthy.
  It never creates profiles and never opens an access point.

## Why
- A compromised gateway cannot rewire arbitrary connections or run commands as root, and a small fixed interface
  is easy to audit.
- The owner is remote. The UI must not cut the path it is being used through, and a broken gateway must not take
  the network down with it.

## Rejected
- Wider network rights for the gateway, so that it keeps Wi-Fi up itself: the fallback does that without widening
  the gateway's privileges.
