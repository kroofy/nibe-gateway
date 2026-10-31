# ADR 0130: Open setup AP without device credentials

Date: 2026-10-31 · Status: accepted (owner decision); amends ADR 0060

## Context
An unprovisioned gateway opens a setup access point. Its WPA2 password and the token needed to create the first
admin were generated on the device and stored only in a root-only file there. No label carries them, so a fresh
card without the boot-partition file (ADR 0108) could not be set up. The owner decided that joining the setup AP
and creating the first admin must not need a password.

## Decision
- The setup and recovery AP is an open network.
- While no admin exists, setup requests that arrive over the AP need no device token. "Over the AP" means the
  client address is in the AP's subnet and the AP is up at that moment.
- From anywhere else the token is still required until an admin exists. After that, only an admin session counts.
- Unchanged: admin password rules, login limits, the station Wi-Fi password, and no AP after an outage.

## Why
Without a way to hand the credentials over, they only lock the owner out. Being in radio range while the AP is
up replaces the proof the token gave. Keeping the token everywhere else stops a device on the home LAN from
claiming an unprovisioned Pi there, including on a LAN that uses the same subnet.

## Rejected
- A password on a label: no label exists, and the owner decided against one.
- The password or token on the boot partition: anyone with the card has it, and it duplicates ADR 0108.
- A fixed default password: the same on every device, which ADR 0060 rules out.
- No token anywhere: any LAN device could claim an unprovisioned Pi.

## Consequences
Anyone in radio range while the setup AP is up can claim an unprovisioned gateway first and can read the setup
traffic (plain HTTP on an open network). Create the first admin right after the first boot or a factory reset,
and check that the account is yours. The recovery AP is open too, but an admin exists then, so it still needs the
admin login. The boot-partition file is still the way to provision without the AP.

## Amendment 2026-10-31
Sharing an open AP needs NetworkManager's `wifi.share.open` polkit action, so the service account's rule allows it
(the protected one stays). A refused AP no longer stops the gateway: it stays reachable, reports offline with the
error, and retries the AP with a backoff. Found on hardware in HP-47; the simulated network never asks polkit. The device still
stores an unused AP password: releases before this one need it to open their data, and an update to one of them
or a rollback must keep working.
