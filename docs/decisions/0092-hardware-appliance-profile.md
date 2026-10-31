# ADR 0092: Images boot as the hardware/LAN appliance; Wi-Fi does not depend on the gateway

Date: 2026-10-23 · Status: accepted; amended 2026-10-25 (Wi-Fi fallback)

## Context
The first image ran the simulator on loopback, which was the safe choice before authentication existed. Once auth
existed, a freshly flashed card was useless (review finding F5).

## Decision
- Images boot in hardware mode, reachable on the LAN and advertised over mDNS.
- Pump access is read-only by default. The simulated image is an explicit option.
- The build refuses binaries without the hardware features.
- The gateway's service account gets only the narrow network rights it needs for its own profiles.

## Why
- The product is the hardware appliance. Safety now comes from authentication and no writes, not from loopback.
- With least privilege, a compromised gateway cannot rewire arbitrary connections.

## Amendment 2026-10-25: Wi-Fi fallback
NetworkManager never autoconnects the gateway's own profiles: they are visible only to a user with a login session.
A gateway that cannot start therefore left the Pi offline.

A small root fallback now brings up a saved station profile when nothing is connected and the gateway is unhealthy.
It never creates profiles and never opens an access point.

**Why this approach:** it keeps the gateway's privileges unchanged, while a broken gateway no longer strands a
remote Pi.
