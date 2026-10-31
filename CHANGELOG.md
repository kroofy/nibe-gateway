# Changelog

User-facing changes per release. `scripts/publish-github-release.sh` publishes the section for the version being
released as the GitHub release notes, so every release needs a section here before it is published. Each release
also carries the fresh-card image (`nibe-gateway-alpine-<version>.img.xz`) for new installations and the signed
update (`nibe-gateway-<version>.tar`) for the gateway's Updates page.

## 0.1.33 — 2026-10-31

- **Faster card flashing.** The fresh-card image now holds only the boot partition and the system, about a tenth
  of the bytes, so writing a card takes a minute or two instead of about 25 minutes. The first boot completes the
  card's layout by itself (both system slots and the data partition) and then boots normally; it takes a little
  longer and leaves a trace in `nibe-trial/` on the boot partition. If that first boot fails, write the card
  again (ADR 0129).
- **Open setup network.** An unprovisioned gateway opens the Wi-Fi network `NIBE-Gateway-XXXX` without a
  password, and while no administrator exists, a phone or laptop on that network can create the first admin
  without the device setup token. No label or printout is needed any more. From the home network, Tailscale or
  anywhere else the token is still required until an admin exists. Anyone in radio range can claim an
  unprovisioned gateway first, so create the admin right after the first boot or a factory reset, and check that
  the account is yours. The recovery network is open too, but it needs the admin login. `GET
  /api/v1/setup/status` reports `setup_token_required` for the asking client (ADR 0060).
- **The gateway stays reachable if the setup network does not start.** It used to exit, taking the web page down
  with it. Now it reports `offline` with the error and retries the setup network after 30 seconds, then less
  often, up to every 5 minutes.
- Fix: cards written with the `0.1.31-firstboot.2` and `0.1.31-firstboot.3` test images could not update to a
  release (the update failed to start the gateway). This release repairs that the first time it starts, so
  update those cards to 0.1.33 or later; 0.1.32 still fails on them.
- Checked on the Pi: the first-boot layout. Not yet checked on the Pi: the open setup network (HP-47).

## 0.1.32 — 2026-10-31

- **Audit log.** A new **Audit log** page lists every change made to the gateway and the pump, newest first: who
  made it (the admin, an API key by name, MQTT, or the gateway itself when a lease lapses), what was changed, and
  whether it worked, with the error when it did not. For every register write it also shows what the pump
  answered and the value read back, whatever caused the write. Passwords and keys are never stored; reads and
  sign-ins are not listed. The newest 1000 entries are kept across restarts and updates. API:
  `GET /api/v1/audit` (admin session only) (ADR 0128).
- **Active leases** are listed at the top of the Audit log page: register, holder, leased value, the value restored
  when it lapses, and the time left. Until now only the heating-offset lease was shown, on Controls.
- The log starts with this release. Not yet checked on the pump (HP-46).

## 0.1.31 — 2026-10-31

- **Leases for hot-water and heating setpoints.** Writes to 47045 (hot water start temperature, economy), 47049
  (hot water stop temperature, economy) and 47375 (heating stop temperature) can carry `lease_seconds`, like the
  heating offset. If the writer does not renew in time, the gateway restores the value it read before the first
  leased write, so the pump goes back to normal even if the controlling system (for example Home Assistant) dies.
  A write without a lease ends the lease. HTTP: `lease_seconds` on `POST /api/v1/controls/register-write`. MQTT:
  `{"value": 45, "lease_seconds": 600}` on `<base>/register/<id>/set`. Degree minutes (43005) is deliberately not
  leasable (ADR 0123).
- The heating offset (47011) can now also be leased over MQTT. All leases share one implementation.
- `GET /api/v1/status` lists every lease in `register_leases`. `heating_offset_lease` is unchanged.
- Not yet verified on the pump (HP-45).
- **Fresh-card image for new installations** attached to the release from now on. It is checked by inspection and
  by booting it in QEMU, but has not yet been written to a real Pi's card (HP-40); report problems as an issue.

## 0.1.30 — 2026-10-31

- **One-click software updates.** The Updates page has one "Software update" panel with a single Install action.
  It downloads, verifies and activates after one confirmation, shows progress through download, verification and
  restart, and reloads into the new interface. Installing from a file and the updater details are still there,
  under "Install from a file" and "Technical details".

## 0.1.29 — 2026-10-31

- **Read, write and poll any register** from the admin UI, the HTTP API and MQTT (ADR 0125).
  - Write any register number with any encodable value, raw or catalog-scaled. Still one write at a time, sent
    once, read back. On by default; `--no-open-writes` or `"open_writes": false` keeps only the named controls.
  - Read any register now and get the pump's answer.
  - Choose per register whether it is polled fast, slow or not at all, from the Register explorer, the API or
    MQTT. Choices survive restarts and updates.
  - MQTT (with "Accept commands" on): `<base>/register/<id>/set`, `/set_raw`, `/get` and `/poll`.
- Home Assistant discovers newly polled registers without a restart.

## 0.1.28 — 2026-10-31

- CI fixes. First release installed through Online updates on the owner's pump (HP-43).

## 0.1.27 — 2026-10-31

- Updates page: neutral wording for online updates.

## 0.1.26 — 2026-10-31

- Initial public release: the gateway, admin UI, HTTP API, MQTT with Home Assistant discovery, history, signed
  A/B application and OS updates, and online updates from GitHub Releases.
