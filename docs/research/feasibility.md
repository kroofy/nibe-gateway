# M0 feasibility assessment

WS01, 2026-10-22. Inputs: [protocol.md](protocol.md), [hardware.md](hardware.md), [nibepi-analysis.md](nibepi-analysis.md),
[register-catalog.md](register-catalog.md), [toolchain.md](toolchain.md).

**Verdict: feasible.** A Rust gateway that emulates the MODBUS40 accessory on the existing NibePi RS-485 hardware
is well supported by three independent open implementations that agree on framing, checksum, escaping,
tokens and ACK/NAK. Read-only monitoring can be built and tested entirely against synthetic/replayed data.
The remaining risks are timing, 32-bit word order and the switch-over from NibePi — all testable with a
short, reversible hardware session.

## 1. Proven (from sources; not yet on our F730)
* Bus: RS-485 9600 8N1, pump is master, accessory MODBUS40 = address `00 20`.
* Frames: `5C 00 20 CMD LEN DATA CHK` from pump, `C0 CMD LEN DATA CHK` from accessory; XOR checksums (pump frame
  excludes start byte, accessory frame includes it); pump checksum `5C`→`C5`; `5C` doubled in pump data;
  LEN/CHK over wire bytes. Confirmed against third-party real-bus bytes by `tools/nibe_ref.py selftest`.
* ACK `06` / NAK `15`; read/write tokens (`69`/`6B` with LEN 0); one request per token; read response `6A`,
  write response `6C`, product info `6D`, data message `68` (≤ 20 registers).
* A missing accessory reply raises alarm 251 on F730.
* F730 register list (581 entries) from the manufacturer export, identical in NibePi.
* NibePi hardware: Pi Zero W, AB Electronics RS485 Pi (auto direction) on GPIO14/15, `/dev/ttyAMA0`.
* ARMv6 Rust targets exist with prebuilt std; `cross` images are genuinely ARMv6.

## 2. Assumptions (we proceed as if true, each with a test)
| # | Assumption | Test |
|---|---|---|
| A1 | The owner's pump/firmware speaks exactly this protocol (F730 is in NibePi's supported list and NibePi works today). | Passive capture via NibePi log (HW-1) |
| A2 | Owner's HAT = AB RS485 Pi with automatic direction. | Photo (HW-0) |
| A3 | Replying within a few ms after a frame is fast enough. | Timing capture (HW-3) |
| A4 | The ARMv6 binary (glibc 2.28 baseline) runs on NibePi's Bullseye (glibc 2.31) and on our image. | `--version` on the Pi (HW-2) |
| A5 | A short silent gap during switch-over only raises a non-critical info alarm (heating continues). | Observe display during HW-3 |

## 3. Unknowns
ACK deadline; token/data-message periods; read-response latency; behaviour for unsupported registers; whether
48852 affects data-message slots; request checksum `5C`; whether alarm 251 auto-clears; which accessories are
activated (RMU40?); LOG.SET contents; real F730 product string/firmware; whether the HAT echoes our TX.
(Details: protocol.md §9.)

## 4. Blockers
* **No blocker for M1–M5 development** (simulation, replay, packaging).
* **M6 (real F730) is blocked on hardware access**: someone must run the HARDWARE_PENDING WS01 steps. All steps
  are designed to be doable over SSH to the existing NibePi, except identifying the HAT (photo) and an optional
  SD-card swap.
* Writable controls remain blocked until per-register verification (brief §6) — out of scope for M0.

## 5. Safe migration and rollback plan from NibePi
Principles: one transmitter on the bus at any time; never leave the pump unanswered for long; never write to the
pump; every step reversible remotely; NibePi's SD card stays bootable.

**Phase 0 – Inventory (read-only, no change to bus behaviour).** Over SSH to the NibePi:
`cat /boot/config.txt /boot/cmdline.txt /etc/nibepi/config.json`, `ls -l /dev/serial0 /dev/ttyAMA0`,
`systemctl list-units | grep -Ei 'node|nibe'`, `uname -a`, `cat /etc/os-release`, `ldd --version`. On the pump
display (owner/helper): menu 5.2 activated accessories (Modbus, RMU40?), menu 5.3.11 word swap value, current
alarm list, current values of 40004/40013/40033 for later comparison. Record the registers NibePi automations
write (heat offset 47011, room setpoints 47398/47397, curve 47007, …) — readable in the NibePi UI.

**Phase 1 – Passive capture with NibePi still in charge.** Enable NibePi raw logging (nibepi-analysis.md §6),
collect ≥ 30 min of `/tmp/nibepi.log`, copy off, disable logging again. Gives real frames, periods, LOG.SET
content, product string, 48852 behaviour — **zero risk** (NibePi keeps acking). Feed into WS04 replay tests.

**Phase 2 – Short supervised trial of our gateway on the NibePi OS (no SD swap).** Copy the ARMv6 binary to
`/home/pi/nibe-gateway` (rw remount), run in read-only mode. Switch with a dead-man fallback so the pump is
never left unanswered for long:
```
sudo systemd-run --unit=nibepi-fallback --on-active=15min /bin/systemctl start nodered.service
sudo systemctl stop nodered.service && sudo /home/pi/nibe-gateway --device hardware --serial /dev/ttyAMA0 \
     --read-only --capture /tmp/gw-capture.jsonl
#   (exact service name from Phase 0; CLI flags illustrative — use the ones WS02/WS04 ship;
#    gap between stop and start must be < 2 s)
```
Watch: our ACK counters rising, no NAK storms, product info decoded, values equal to the display, no new alarm
(or only 251 — note whether it self-clears). Stop with Ctrl-C and `sudo systemctl start nodered.service`; the
fallback timer restarts NibePi anyway after 15 min. **Rollback = start NibePi's service** (seconds).

**Phase 3 – Longer trial as a systemd service on the NibePi OS**, NibePi disabled but installed, with a watchdog
unit that re-enables NibePi if the gateway is unhealthy (no valid frame for 60 s). Rollback = `systemctl disable
--now nibe-gateway && systemctl enable --now nodered`.

**Phase 4 – New image on a NEW SD card** (needs someone on site, or the A/B updater of WS07 once installed). Keep
the NibePi SD card labelled in a drawer: physical rollback = swap cards (2 min, power-cycle via pump breaker or
the Pi's supply).

What we lose by leaving NibePi and must replicate or consciously drop: NibePi's automatic alarm-251 reset, RMU40
emulation (if used), Node-RED automations (weather/price/indoor/hot water), MQTT/Home-Assistant feed, cloud ping
(intentionally dropped). Values NibePi last wrote stay in the pump — the inventory from Phase 0 lets the owner
restore manual settings on the display if desired.

## 6. Recommendations to other workstreams
* WS02/WS04: implement exactly the state machine in protocol-summary §5; serial task on its own thread;
  counters for frames/ACK/NAK/checksum errors/garbage/timeouts; capture recorder in `capture-jsonl`.
* WS04: importer for NibePi `/tmp/nibepi.log` RAW lines; config option for 32-bit word order
  (`auto` = read 48852, `swap`, `noswap`) defaulting to `auto`; refuse requests whose checksum would be `5C`
  or whose data contain `5C`; catalog loader for `fixtures/register-catalogs/f730.json` schema v1.
* WS03: keep glibc baseline ≤ 2.31 (the toolchain used 2.28) so the binary runs on NibePi Bullseye; musl static as fallback; pi-gen master/armhf; UART config in hardware.md §3.
* WS08: hardware session = HARDWARE_PENDING WS01 items in order HW-0 … HW-4.
