# ADR 0120: Keep storing the Wi-Fi passphrase (derived WPA2 key considered, not adopted)

Date: 2026-10-26 · Status: accepted (owner decision)

## Context
The station passphrase is stored as given, on root-only storage with no encryption at rest (ADR 0107). A one-way
hash cannot work, because the Pi must use the secret to join the network. Storing the derived WPA2 key instead of
the passphrase would work.

## Decision
Not adopted. Keep the passphrase.

## Why
- The derived key still lets anyone holding the card join the network. It only hides the passphrase, which matters
  only when that passphrase is reused elsewhere.
- The owner's Pi is on a dedicated IoT network, inside the heat pump.
- Support reports and logs never contain the passphrase.
- The derived key would rule out WPA3-only networks and require a profile migration.

## Revisit when
- the Pi must join a network whose passphrase is reused elsewhere; or
- other people run the gateway on their own networks.
