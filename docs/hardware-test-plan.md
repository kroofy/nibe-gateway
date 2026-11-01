# Hardware test plan — first physical sessions

Status (updated 2026-10-25): **mostly historical.** Written by WS08 on 2026-10-22, before any hardware work.
Since then the gateway has replaced NibePi on the owner's F730, and the Pi runs Alpine Linux in A/B OS slots. Sessions A, C and D are done or retired: the live trial ran on a bench card (HP-26) instead of the
NibePi OS, and the NibePi card was reused for the appliance image, so there is no NibePi fallback. The pi-gen image
of Session B is retired; the Alpine bench steps are HP-40. **For what is still open, use the summary at the top of
[`docs/progress/HARDWARE_PENDING.md`](progress/HARDWARE_PENDING.md).** The NibePi, systemd and pi-gen commands
below are kept as a record and are not valid on today's appliance (use `rc-service`, `rc-status` and
`/var/log/messages`).

This is the single ordered procedure for the owner's first physical work. It consolidates WS01 HW-0…HW-5
(`docs/research/feasibility.md` §5) and every item in
[`docs/progress/HARDWARE_PENDING.md`](progress/HARDWARE_PENDING.md). HARDWARE_PENDING remains the
itemised checklist; each step below names the items (`HP-xx`) it closes.

## Ground rules (apply to every step)

1. **One bus master at a time.** NibePi (Node-RED) and the gateway never both have `/dev/ttyAMA0` open.
   The gateway (serialport-rs) takes `TIOCEXCL` plus a non-blocking exclusive `flock`, and refuses
   a port that another process already holds with `flock`. node-serialport's default `lock: true` should
   do the same, but that is **unverified on NibePi**. The lock is advisory, so always check with `fuser`
   before you start (step A5).
2. **Writes only through named, verified controls** (updated 2026-10-25). There is no raw-write endpoint.
   The appliance config allowlists 47011 and 48132, plus the alarm reset (45171 as a 0 → 1 edge), 0113. Never write 48852. A new control is verified one register at a time with the owner watching.
   (Original rule: no writes at all, never write 45171.)
3. **The production card is never reflashed** (updated 2026-10-25). It moves forward by OS A/B updates only;
   bench work uses a spare Pi and card. (Original rule: keep NibePi recoverable. That card now holds the
   appliance image.)
4. **Owner approval** is needed at every point marked ⛔. Approval means the owner (or the person on
   site) agrees at the time, having read that step's abort criteria.
5. **Stop at the first surprise.** Any new pump alarm (other than 251 during a switch-over), a change in
   heating behaviour, a NAK storm or a gateway error means you roll back that step at once and record
   what happened.
6. **Keep secrets out of the repo.** Setup tokens, AP and admin passwords, cookies, NibePi
   `flows_cred.json`, MQTT and Wi-Fi credentials, and raw captures go in a private folder. Commit
   captures only after sanitising them.

## Sessions at a glance (as planned on 2026-10-22; outcome in the status note at the top)

| Session | Where | Pump risk | Needs | Approx. time |
|---|---|---|---|---|
| **A** First visit (this plan in detail) | The existing NibePi Pi | Steps A1, A3, A4: none. A2: pump runs without its accessory for ≤ 30 min. A5: ≤ 10 min read-only live trial | SSH to NibePi, someone at the pump display, a PC with an SD reader | 3.5–5 h |
| **B** Bench appliance | A **spare** Pi Zero W/WH and spare SD cards, F730 never connected | none | Image built on a rootful-Docker host; phone and laptop; a disposable Wi-Fi router | 1–2 days, can be split |
| **C** Longer trial on the NibePi OS | NibePi Pi | Hours of live read-only service, with fallback | Session A passed; a fallback watchdog unit (not written yet, see "What's left" in PROGRESS.md) | ½ day plus soak |
| **D** Migrate to the new image | NibePi Pi, new SD | Read-only service on the new image | Sessions B and C passed | 1–2 h plus soak |

Do sessions in this order. Session A gives real captures, which the developers turn into replay tests
before Session C. Session B is independent of the pump and can run any time after its image is built.

---

## Before Session A: preparation on the development machine (no hardware, about 1–2 h)

Done by the developer or the owner from a clone of this repository.

```bash
./scripts/bootstrap-tools.sh
export PATH="$HOME/.local/share/nibe-build-tools/bin:$PATH"
just test                                         # must pass
python3 docs/research/tools/nibe_ref.py selftest  # expect "FAILURES: 0"
python3 crates/device-f730/tests/serial-pty-smoke.py
python3 crates/device-f730/tests/replay-http-smoke.py
# Read-only hardware build: serial adapter on, dev feature impossible (build-pi.sh rejects it)
NIBE_PI_FEATURES=serial NIBE_TARGET_DIR=target/pi-serial ./scripts/build-pi.sh
./scripts/build-frontend.sh                       # optional: UI for viewing values in a browser
cp target/pi-serial/arm-unknown-linux-gnueabihf/release/nibe-gateway ./nibe-gateway
sha256sum nibe-gateway > nibe-gateway.sha256
```

Pack for the visit: that binary and its `.sha256`; optionally `apps/admin/dist/` as a tarball;
`docs/research/tools/nibe_ref.py`; a spare microSD card at least as large as the NibePi card; an SD reader;
a camera; this document, printed. Optional: a logic analyser on the RS485 HAT's UART TX/RX pins to measure
real reply latency (HP-07).

**Gate:** all commands above pass and the ELF check prints `Tag_CPU_arch: v6` with highest glibc ≤ 2.31.

---

## Session A — first visit

Session A outcome: closed. HP-01/02/04 retired, HP-03/05/06 verified through the gateway's own capture and HP-26 (see HARDWARE_PENDING).

### A1. Inventory, look only (HP-01 · WS01 HW-0) · 30–45 min · pump risk: none

1. Photograph the Pi stack: HAT name and silkscreen, jumpers, whether a 120 Ω termination resistor is
   fitted on A/B, and the pump's accessory terminal wiring.
2. On the pump display: service menu (hold *Back* about 7 s) → **5.2 System settings**. List every
   ticked accessory (Modbus? RMU40 S1–S4?). Then **5.3.11 MODBUS40 settings**: write down the
   **word swap** value; you need it in A5. Write down the current alarm list and current values of
   outdoor temperature BT1, hot water top BT7, room temperature BT50, degree minutes and compressor
   operating hours/starts.
3. Over SSH (`ssh pi@nibepi` or its IP), save the output privately:
   ```bash
   cat /etc/os-release; uname -a; ldd --version | head -1
   cat /boot/config.txt /boot/cmdline.txt
   ls -l /dev/serial0 /dev/ttyAMA0 /dev/ttyS0
   systemctl list-units --type=service | grep -Ei 'node|nibe|red'
   systemctl cat <the node-red service found above>   # note the unit name, user and flows path
   systemctl is-enabled hciuart
   sudo fuser -v /dev/ttyAMA0                          # which process owns the UART
   cat /etc/nibepi/config.json                         # redact MQTT/cloud credentials
   free -m; df -h; mount | grep ' / '
   ```
4. In the NibePi UI (`http://nibepi:1880/ui`), note the enabled automations (weather, price, indoor,
   hot water, RMU, fan) and the current heat offset 47011, room setpoint 47398 and heat curve 47007.

**Expected:** Waveshare RS485 CAN HAT with factory links, i.e. auto direction (the AB Electronics
RS485 Pi was the earlier assumption), `dtoverlay=disable-bt` (or `pi3-disable-bt` /
`miniuart-bt`), no `console=serial0` in cmdline, Bullseye with glibc 2.31, word swap 0, Modbus ticked in 5.2.

**Go/no-go:** go if the UART is `/dev/ttyAMA0` (PL011), owned only by the Node-RED process, and glibc
is ≥ 2.28. **No-go** if the HAT is not an auto-direction RS485 interface, or if the console or getty is on the
UART: stop and ask the developers. **Rollback:** nothing changed.

Record the outputs in `docs/research/inventory-<date>.md` (redacted).

### A2. ⛔ Back up NibePi (HP-02) · 45–60 min · pump runs without its accessory for 20–30 min

Approval point: the pump has no MODBUS40 accessory while the card is out and will probably raise alarm
251. Heating continuing normally with alarm 251 active is a **hypothesis**. Ask the owner to accept
this, and choose a time when a short loss of NibePi automations is harmless.

1. **File backup first (online, zero risk).** It contains credentials, so keep it private.
   ```bash
   ssh pi@nibepi 'sudo tar -C / -czf - etc/nibepi boot/config.txt boot/cmdline.txt <flows dir from A1> 2>/dev/null' \
     > nibepi-files-$(date +%F).tgz
   ```
2. **Full card image (offline).** On the NibePi run `sudo poweroff` and wait for the activity LED to stay
   off. Remove power, then the SD card. On the PC:
   ```bash
   lsblk                                   # identify the card, e.g. /dev/sdX; double-check it
   sudo dd if=/dev/sdX of=nibepi-$(date +%F).img bs=4M status=progress conv=fsync
   sha256sum nibepi-*.img > nibepi.img.sha256
   sudo losetup -Pr --show -f nibepi-*.img          # read-only loop, prints /dev/loopN
   sudo mount -o ro /dev/loopNp2 /mnt && ls /mnt/etc/nibepi && sudo umount /mnt && sudo losetup -d /dev/loopN
   xz -T0 -k nibepi-*.img
   ```
   Optionally write the image to the spare card (`dd of=` the spare), label it "NibePi backup <date>" and
   store it. **Never** write to the original card.
3. Put the original card back and power on. Wait about 2 minutes. Confirm that the NibePi UI loads,
   `systemctl is-active <node-red service>` is `active`, values update, and the pump alarm 251 has
   cleared. NibePi auto-resets 251 at start-up; that is its existing behaviour.

**Go/no-go:** go only when the image's checksum is recorded, the loop mount shows `/etc/nibepi`, and NibePi
is back in normal service with no persistent alarm. **No-go:** if NibePi does not come back, boot the
backup clone (or rewrite the image to a card) and stop the session.
**Rollback:** reinsert the original card (no data was changed).

*If downtime is unacceptable:* an online image is possible (`ssh pi@nibepi 'sudo nice -n19 ionice -c3 dd
if=/dev/mmcblk0 bs=4M' > nibepi.img`). With NibePi's read-only root the image is mostly consistent, but on
a single-core Zero it takes about 1 h and loads the CPU that answers the pump. Do it only with approval,
and watch for alarm 251.

### A3. Passive real-traffic capture through NibePi's own logger (HP-03 · WS01 HW-1) · 45–75 min · pump risk: none

NibePi stays bus master throughout.

1. Preferred: enable logging in the NibePi UI with the "core" and "debug" levels. This takes effect
   without a restart. Otherwise (restarting causes NibePi's usual start-up writes 45171=1 and 48852=0, which
   is its existing behaviour, not something new):
   ```bash
   sudo mount -o remount,rw / && sudo cp /etc/nibepi/config.json /etc/nibepi/config.json.ws01-bak
   node -e 'const f="/etc/nibepi/config.json",c=require(f);c.log=Object.assign(c.log||{}, {enable:true,core:true,debug:true,info:true,error:true});require("fs").writeFileSync(f,JSON.stringify(c,null,2))'
   sudo systemctl restart <node-red service>
   ```
2. Leave it running for **at least 30 min**, ideally including a hot-water cycle.
3. `cp /tmp/nibepi.log ~/nibepi-ws01-$(date +%F).log && xz -k ~/nibepi-ws01-*.log`, then `scp` it off
   the Pi. `/tmp` is RAM, so copy it before any reboot.
4. Revert logging (UI, or restore `config.json.ws01-bak` and restart), then `sudo mount -o remount,ro /`.

**Expected:** RAW lines with `92,0,32,105,0,73` (read token), `92,0,32,107,0,75` (write token),
`92,0,32,104,…` data messages, and `92,0,32,109,…` product info about every 15 s containing "F730".
Checksums validate with `nibe_ref.py`.

**Go/no-go for A5:** go if the log shows tokens and data messages addressed to `00 20` and the product string
says F730. **No-go** if there is no MODBUS40 traffic. In that case the accessory is not active; stop, because
A5 would only produce alarms. **Rollback:** disable logging (step 4).

Afterwards the developers sanitise the log into `fixtures/serial-captures/real-f730-nibepi-<date>.log`,
convert it with `python3 scripts/import-nibepi-log.py INPUT.log OUTPUT.capture.jsonl`, and add replay tests. The importer retains RX chunks/relative wall-clock time only; inspect and sanitize payloads before sharing. Backwards clock jumps require splitting the log.

### A4. Gateway binary on the NibePi OS, simulated, no bus access (HP-04 · WS01 HW-2) · 15–20 min · pump risk: none

```bash
scp nibe-gateway nibe-gateway.sha256 pi@nibepi:/tmp/
ssh pi@nibepi
sudo mount -o remount,rw / && install -m755 /tmp/nibe-gateway /home/pi/nibe-gateway && sudo mount -o remount,ro /
(cd /tmp && sha256sum -c nibe-gateway.sha256)
/home/pi/nibe-gateway --help >/dev/null && echo runs
install -d -m700 /tmp/nibe-gw
/home/pi/nibe-gateway --device simulated --network simulated --bind 127.0.0.1:8099 --data-dir /tmp/nibe-gw &
sleep 5; curl -fsS http://127.0.0.1:8099/health; echo
free -m; ps -o pid,rss,pcpu,cmd -C nibe-gateway
```

While the simulator runs, **create the administrator now**, so that the live trial in A5 only needs one
login and no extra password hashing. Each Argon2 hash costs about 1–2 s of CPU on the single core.

```bash
python3 -c 'import getpass,json;json.dump({"password":getpass.getpass("New admin password (12+ chars): ")},open("/tmp/nibe-gw/pw.json","w"))'
chmod 600 /tmp/nibe-gw/pw.json
TOKEN=$(python3 -c 'import json;print(json.load(open("/tmp/nibe-gw/setup-secrets.json"))["setup_token"])')
curl -fsS -c /tmp/nibe-gw/jar -H 'X-Requested-With: nibe-gateway' -H "X-Setup-Token: $TOKEN" \
  -H 'Content-Type: application/json' --data-binary @/tmp/nibe-gw/pw.json http://127.0.0.1:8099/api/v1/setup/admin
kill %1
```

**Expected:** no "Illegal instruction" and no "GLIBC_x.yy not found"; health returns HTTP 200; RSS is well below free
memory while Node-RED is also running. **Go/no-go:** a failure here stops the session; there is no
fallback build. **Rollback:** `kill %1`. Nothing touched the bus; `/tmp` vanishes on reboot.

### A5. ⛔ Supervised read-only live trial with automatic NibePi fallback (HP-05, HP-06 · WS01 HW-3, HW-4) · 30–45 min including preparation, ≤ 10 min live

Approval point: for up to 10 minutes the gateway, not NibePi, answers the pump. Prerequisites: A1–A4
passed; the owner is watching the pump display or can be reached; the word-swap value from A1 is known.

Preparation (NibePi still running):

```bash
SVC=<node-red service from A1>
WORD_SWAP=<0 or 1 from menu 5.3.11>   # never guess: use auto if unknown (32-bit values then stay raw)
# Dead-man: NibePi restarts after 15 min whatever happens to this SSH session.
sudo systemd-run --unit=nibepi-fallback --on-active=15min /bin/systemctl start "$SVC"
```

Switch-over in **one command line**, so that the gap is under 2 s. The gateway runs as a transient unit, so an
SSH drop cannot kill it half-way. It stops by itself after 10 min, and NibePi is restarted as soon as it stops:

```bash
sudo systemctl stop "$SVC" && sudo fuser /dev/ttyAMA0; \
sudo systemd-run --unit=nibe-trial --uid=pi --gid=dialout -p RuntimeMaxSec=600 \
  -p ExecStopPost="+/bin/systemctl start $SVC" \
  /home/pi/nibe-gateway --device hardware --serial /dev/ttyAMA0 --network simulated \
  --bind 127.0.0.1:8099 --data-dir /tmp/nibe-gw --word-swap "$WORD_SWAP" \
  --record-capture /tmp/nibe-gw/hw3-$(date +%F-%H%M).capture.jsonl
```

`fuser` must print nothing before the gateway starts. If it prints a PID, the old master still holds the port.
The gateway would then refuse the port, and NibePi comes back through `ExecStopPost` or the timer.
The `+` prefix runs only the NibePi restart with root privileges; the gateway itself runs as `pi`.
If your systemd rejects `ExecStopPost` in `systemd-run` (Bullseye ships 247, which should accept it),
drop that property: the 15-minute timer remains the fallback.

Observe (second SSH session), at most 10 minutes:

```bash
journalctl -u nibe-trial -f            # in one terminal
curl -fsS -c /tmp/nibe-gw/jar -H 'X-Requested-With: nibe-gateway' -H 'Content-Type: application/json' \
  --data-binary @/tmp/nibe-gw/pw.json http://127.0.0.1:8099/api/v1/auth/login     # one login only
cat > /tmp/nibe-gw/show.py <<'PY'
import json, sys
d = json.load(sys.stdin)
print(d["diagnostics"]); print("last_error", d["last_error"], "word_order", d["word_order"])
for i in d["items"]:
    r = i["reading"]
    if r and i["id"] in (40004, 40013, 40033, 43005, 43416, 43420, 45001, 48852):
        print(i["id"], r["decoded"], r["error"])
PY
watch -n5 'curl -fsS -b /tmp/nibe-gw/jar http://127.0.0.1:8099/api/v1/registers/explorer | python3 /tmp/nibe-gw/show.py'
curl -fsS -b /tmp/nibe-gw/jar http://127.0.0.1:8099/api/v1/device/identity
```

To use the UI instead, copy `apps/admin/dist` to the Pi, add `--admin-dir`, and run
`ssh -L 8099:127.0.0.1:8099 pi@nibepi` from the laptop. The gateway stays loopback-only.

Compare BT1 (40004), BT7 (40013), BT50 (40033) and degree minutes (43005) with the pump display. For
HW-4, compare the compressor starts and operating time (43416/43420, 32-bit) with the display: they decode only if
the word order is known, and data-message slots stay raw when word swap is 0 (by design until verified).

**Abort immediately** (`sudo systemctl stop nibe-trial`; NibePi restarts through ExecStopPost) if:
`ack_deadline_misses` > 0 or `last_error` is set (the short first trial still aborts on a single miss; the runtime now tolerates configured transient misses and reopens on sustained failure); `naks_sent`/`checksum_errors`
keep growing (wiring/baud problem); `frames_for_us` stops increasing for 10 s; any new pump alarm other
than 251; or any change in heating behaviour.

Normal end: `sudo systemctl stop nibe-trial`. Then check `systemctl is-active "$SVC"` (`active`) and
`sudo systemctl stop nibepi-fallback.timer` if it is still pending. Confirm on the display that the alarm list is
back to its A1 state; reset 251 **on the display** if it stays. Copy off
`/tmp/nibe-gw/hw3-*.capture.jsonl` **and** its `.summary.json` (expect `"complete": true`).

**Expected:** steady ACKs, zero or rare NAKs, decoded values matching the display, `read_requests_sent`
= 0, no unsolicited TX in the capture, and `max_reply_us` well under 10 000. Record whether alarm 251
appeared and how long it took to clear. **Rollback:** NibePi restarts automatically through ExecStopPost, then
the timer at the latest. Manual: `sudo systemctl start "$SVC"`.

Not in this session: **HW-5** (request checksum `0x5C`, escaped request data). The shipped binary
cannot send those frames; it refuses them by design. They need a separately reviewed build and approval;
see HP-08.

### A6. Wrap-up · 15 min

Remove `/tmp/nibe-gw`, or reboot the NibePi, which clears `/tmp`. Store the private folder (inventory,
backup image, NibePi log, capture, notes) somewhere safe. Send the developers the redacted inventory, the
sanitised NibePi log, the capture and summary, the observed values against the display, the alarm notes and
`free -m`.

**Session A total:** about 3.5–5 h, of which the pump runs without NibePi for at most ~30 min (A2) plus
≤ 10 min (A5).

---

## Session B — bench appliance on spare hardware (F730 never connected)

Prerequisites: a spare Pi Zero W/WH; at least two spare microSD cards; a fresh-card image built
rootless as in `docs/dev/image-build.md` (`scripts/build-alpine-image.sh`, real release key; the Raspberry Pi OS
pi-gen build was retired 2026-10-25); a phone and
laptop; and a disposable router whose Wi-Fi credentials may be typed at a terminal. The owner's NibePi is not
touched.

Status 2026-10-25: B1 (HP-10) is retired and replaced by HP-40 steps 1–4; B2 is the Alpine supervisor watchdog
(HP-11 status line); B3 (HP-12) and B5 (HP-15) are verified on the owner's Pi; B4, B6–B8 stay open as described in
their HARDWARE_PENDING status lines.

| Step | HARDWARE_PENDING items | What | Time | Gate to continue |
|---|---|---|---|---|
| B1 | HP-10 | Flash spare card, boot, inspect partitions/overlay/mounts/health from a local console | 1–2 h | armv6l, glibc 2.36, overlay root read-only lower, persistent p3, `/health` 200 |
| B2 | HP-11 | systemd readiness, watchdog (SIGSTOP child only), stop/start, journald limits | 1 h | watchdog restarts a hung child; no restart loop |
| B3 | HP-12 | Persistent storage, reboot persistence, UART routing (`/dev/serial0` → ttyAMA0, no getty/BT) | 1 h | markers persist correctly; PL011 free |
| B4 | HP-13, HP-14 | ⛔(own network) Secured first-boot AP, wrong-password rollback, station connect, admin setup, offline/recovery/factory reset | 3–4 h | WPA2 AP, no secrets in public status, no AP on ordinary outage |
| B5 | HP-15 | mDNS + LAN listener + NM Polkit for the service user | 1–2 h | `nibe-gateway.local` resolves; unauthenticated API returns 401 |
| B6 | HP-16, HP-17 | Updater isolation, key-only SSH deploy/rollback, power-loss matrix during staging/activation | 3–5 h | interrupted updates recover to a ready slot; no `current` corruption |
| B7 | HP-18 | Power cuts on persistent config/secrets; Argon2 login budget and rate limiting on ARMv6 | 2 h | config intact or fails closed; login latency recorded |
| B8 | HP-19 | Admin UI on phone/desktop over the real radio; footprint (RSS/CPU) during SSE + register search + update | 1–2 h | no OOM, responsive UI |

Each step's exact commands are in the referenced HARDWARE_PENDING item. **Rollback** for all of Session B:
re-flash the spare card; nothing touches the NibePi or the pump.

## Session C — longer read-only trial on the NibePi OS (retired: the gateway has run the pump continuously since 2026-10-24; HP-07/HP-20 remain as nice to have, HP-24 retired)

Prerequisites: Session A passed; the developers have turned the A3/A5 captures into replay tests and fixed
any findings; and a **fallback watchdog unit** exists that restarts NibePi if the gateway reports no valid
MODBUS40 frame for 60 s or stops. The watcher is implemented in `packaging/nibepi/` (installation/re-arm in its README; HP-24). Its service names and 60000 ms threshold must be configured for the inventoried NibePi OS. Leave NibePi enabled and the gateway disabled at boot; arm the timer only after the trial receives a valid MODBUS40 frame. Run the gateway as a systemd service
on the NibePi OS with NibePi disabled but installed, for hours and then days. During this period, run
HP-07 (physical reply latency with a logic analyser, wire diagnostics under HTTP/SSE load, disconnect/
reconnect via the reviewed procedure) and HP-20 (diagnostic lease, capture export, replay off-device).
**Rollback:** `systemctl disable --now nibe-gateway && systemctl enable --now <node-red service>`.

## Session D — migrate to the new image (done: appliance image on the owner's card 2026-10-24/25, Alpine since HP-38)

Only after B and C pass. Flash the release image on a **new** card. Keep the NibePi card labelled in a
drawer: the physical rollback is to swap the cards and power-cycle, which takes about 2 minutes. The image now defaults to the read-only hardware/LAN profile with serial and
NetworkManager compiled in. Confirm region, installed NM Polkit and
mDNS on the bench (HP-23); use the shipped simulated/loopback drop-in for no-UART
bench work, then remove it before the approved hardware trial. Writable controls stay disabled until each control's register identity, type,
scaling, range, acknowledgement and failure semantics are verified and the owner approves enabling it.

## Approval points summary

| Where | What the owner approves |
|---|---|
| A2 | Pump without its accessory (alarm 251 likely) for 20–30 min while the card is imaged |
| A5 | Gateway answers the pump for ≤ 10 min, read-only, with automatic NibePi fallback |
| B4 | Use of a disposable router/network for provisioning tests (never the household network config) |
| C | Hours-to-days of read-only gateway service with the fallback watchdog |
| D | Moving the pump to the new image; keeping the NibePi card as rollback |
| HP-08 | Any frame the shipped binary refuses to send (request checksum 0x5C, escaped request data): separate build, review and approval |
| Later | Enabling any writable control, one control at a time, after verification |
