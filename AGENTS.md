# Rules for agents working in this repo

1. Read the notes below. `PROGRESS.md` has the current status, `docs/progress/HARDWARE_PENDING.md` the hardware tests, `docs/brief/` the original requirements, and `docs/dev/setup.md` the checks to run.
2. Keep local simulation working (`cargo run -p nibe-gateway -- --device simulated`). `cargo test --workspace` must pass when you finish.
3. Distinguish **verified facts** from **hypotheses**. Anything needing the physical Pi/F730 goes into `docs/progress/HARDWARE_PENDING.md` (append, don't rewrite) — the owner is remote, so build everything possible and leave precise hardware test steps.
4. Decisions are not written up as ADRs. Say a fact in the code if it fits there, as code or a short comment next to it. Add a note below only for a fact the code cannot show cleanly and that would otherwise send someone down the wrong path. A note says what is true, not what is forbidden, and it can be changed when the facts or the owner's mind change.
5. If your work changes the status or what is left, update `PROGRESS.md`.
6. The gateway binary targets ARMv6 with 512 MB RAM (Pi Zero W), so keep its dependencies small. Browser features are not limited by the Pi, because the admin UI runs in the user's browser; its built files still ship in the update and the Pi serves them.

## Notes: facts the code does not show

Older commits and release notes cite ADR numbers and workstream labels (WS01–WS10). The ADRs are at commit `4279e43` under `docs/decisions/`, the workstream files at commit `218bae5` under `docs/progress/`.

### The pump and the bus
- The pump talks NIBE's accessory protocol, not Modbus RTU, despite the name MODBUS40. The gateway emulates a MODBUS40 accessory.
- Once an accessory is configured, the pump expects a reply within a tight window. Without one it raises alarm 251, and the alarm stays until reset, also after the gateway's own restarts and updates. A missed reply is worse than a lost observation. On hardware a single reset did not always clear a latched 251, and the reset that worked is a 0 → 1 change.
- The owner's F730 pushes no register values, so all data comes from read requests on the pump's read tokens, about one per second. Making the pump push values would need a site visit.
- A write that times out has an unknown outcome: it may have been applied. That is why register writes are sent once and read back instead of retried. Two gateway actions do repeat, because repeating them is safe: restoring a lapsed lease, until a read-back shows the value, and the alarm 251 auto-reset, while the alarm stays.
- Degree minutes is a counter the pump changes itself. Writing back an old count would distort its control, so the owner excluded it from leases.
- The installed RS485 board is a Waveshare RS485 CAN HAT, which switches direction by itself in its factory configuration. NibePi's README links to an AB Electronics board, which early research assumed. A board with other links would need direction control added.
- Timing values on the wire are hypotheses tuned on hardware, not protocol facts.

### Where it runs
- The Pi Zero W is ARMv6 hard-float. Most "armhf" toolchains and userspaces silently target ARMv7, and that only fails on the device, possibly in the middle of a remote update.
- The Pi sits inside the heat pump and loses power whenever the pump is switched off. The owner is usually remote, so a fault that strands the Pi costs a site visit. It has no real-time clock.
- The admin UI runs in the user's browser, not on the Pi. Modern CSS and browser features cost the Pi nothing. It may be used on a network without internet, and the content security policy allows only same-origin resources plus `data:` images, so assets must be served by the gateway. Inline `<style>` blocks, `style` attributes in markup, `setAttribute('style')` and script-created `<style>` elements are blocked; styles set through CSSOM work, including React `style` props. Nothing in the build lowers CSS for older browsers. Storybook and the Vite dev server send no such policy, so a violation only shows against the gateway.

### Things that look wrong but are deliberate (owner decisions)
- With open writes on, any register number the wire can encode takes any encodable value, with no allowlist or catalog range. The exception is the heating offset 47011, which goes through its named control (-10..10). The owner is responsible for the pump's settings, and the display and myUplink allow the same changes.
- The setup AP is open, and a client on it needs no device secret while no admin exists. Nothing on the device shows the owner that secret, so radio range replaces it. Anyone in range can claim an unprovisioned gateway first.
- Plain HTTP, because a LAN appliance has no trust anchor for TLS; use a proxy if TLS is needed. Nothing is encrypted at rest, because there is no key outside the card and no secure element. The Wi-Fi passphrase is stored as given.
- The gateway has no threshold or schedule logic that changes the pump; that belongs to Home Assistant, Node-RED and scripts. The two exceptions only undo effects from outside the pump: the alarm 251 reset, and restoring a lapsed lease.
- The update check runs only when an admin asks. Installing is an explicit admin action: the UI asks for a confirmation, and the API needs separate stage and activate calls. An unattended restart of the heat-pump gateway is the owner's decision.

### Rollback: code that looks dead but is not
After an update rollback, an older binary reads what a newer one wrote. So the config declares the oldest version that may read it, the lease file keeps the original heating-offset fields, an older binary leaves a newer history schema alone, and the device still writes an AP password nobody uses, because older releases require it.

### OS and image history
- Trimming Raspberry Pi OS was tried twice: removing packages proved fragile and updates stayed around 100 MiB. Hence Alpine, built from an allowlist.
- Alpine's kernel hung silently on the first hardware trial, so slots use the Raspberry Pi OS kernel. The firmware once skipped an overlay and the serial port moved, so boot files do not depend on the firmware applying overlays.
- The shared firmware is never updated over the air and needs a reflash. FAT's non-atomic rename leaves a tiny window when an OS slot is committed.
- The owner flashes with `dd`, which writes every byte, so the fresh-card image is short and its first boot completes the partition layout. That has to happen in the initramfs: the data partition is needed before services start, and the root is read-only.
- Across real updates nothing changed after the first complete poll round, so probation can end after a short period of continuous readiness plus one error-free poll round. The long ready window is the fallback when no round completes.

### The register catalog
It comes from NibePi (MIT) and NIBE's ModbusManager export, plus 48852 and 48889 as individual facts from yozik04/nibe. That project is GPL-3.0, so copying its data would tie this one to the GPL. Disagreements are recorded as conflicts, and a few the owner checked against the pump's display are applied in the catalog builder. Text derived from NIBE's export has unclear redistribution terms.
