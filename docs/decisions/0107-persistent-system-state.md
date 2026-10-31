# ADR 0107: Persistent system state is declared in one list; no encryption at rest

Date: 2026-10-24 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
The root is read-only (ADR 0031). Wi-Fi profiles, SSH keys and Tailscale state still have to survive reboots and
OS updates. The Pi Zero has no secure element. The Pi must be able to use the Wi-Fi secret to join the network,
so it cannot be stored as a one-way hash.

## Decision
- Persistent system state lives on the data partition and is mounted at boot from one declared list.
- Nothing is encrypted at rest. Secrets are protected by file permissions.
- The station Wi-Fi passphrase is stored as given.

## Why
- **One list:** what survives is explicit and reviewable, and nothing persists by accident.
- **No encryption:** with no key outside the card, encryption would only obscure.

## Rejected
- Storing the derived WPA2 key instead of the passphrase: it still lets anyone holding the card join the network,
  rules out WPA3-only networks and needs a profile migration. It only hides a passphrase that is reused elsewhere.
  Revisit if the Pi must join such a network, or other people run the gateway on their own networks.

## Consequences
- A new persistent feature adds one entry to the list.
- Support reports and logs never contain the passphrase.
