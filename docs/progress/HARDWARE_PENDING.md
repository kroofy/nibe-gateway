# Hardware-pending verification (itemised)

## Open hardware items (summary, 2026-10-25)

Bookkeeping pass on 2026-10-25: every item below was checked against the result entries. Each
`HP-nn` now has a "Status 2026-10-25" line. Of the 26 checklist items open before this pass, 7 are verified, 5 are
retired (NibePi, pi-gen or systemd only) and 14 stay open, several reworded for the Alpine base. This is the
complete open list:

**Needs the owner at the pump** (no bench needed; read-only except where noted)
- HP-34 (c): 43147 compared with a clamp meter on the compressor supply while it runs, probed in the same minute.
- HP-34 (b): energy counters 41846/41848/41850 compared with the myUplink energy log.
- ~~HP-34 (a): fan modes~~ — done 2026-10-26 (speed 3 test: 47260 and 43108 follow the step, 40050 follows the fan; panel-verification doc).
- HP-42: heating-offset control lease: steps 1–6 **verified on the F730 2026-10-29** (0.1.25). Still open: the fault/race/power-cut checks in the HP-42 amendment and addenda.

**Needs a bench Pi or a spare card** (never the production card or a live bus)
- HP-40 steps 1–5: build, flash and first-boot the fresh Alpine card (unprovisioned setup AP, provisioned boot, first OS update into slot B).
- HP-40 step 6: trigger the Wi-Fi fallback (`nibe-wifi-fallback`) live.
- HP-47: fresh card whose first boot completes the partition layout, flash time and setup AP.
- HP-13 / HP-14: setup-AP onboarding, wrong-password rollback, outage without AP, recovery AP, factory reset (after HP-40 step 3).
- HP-11 (Alpine equivalent) = HP-38a step 2: supervisor watchdog via SIGSTOP and OpenRC respawn, clean stop, bounded `/var/log`.

**Deferred by the owner**
- OS-update failure tests: an unhealthy new slot failed by os-confirm, and the network-gate forced failure (HP-29, HP-38 step 4).
- App-update failure tests: `just rollback` on hardware, bad signature or timeout, probation rollback after restarts or a reboot, schema `min_reader_version`, updater isolation under OpenRC (HP-16, HP-22).
- Power-cut matrix on a spare card: update and OS staging (HP-17), config and secrets plus the Argon2 login budget (HP-18), history (HP-33 step 5), evidence rename (HP-22).

**Nice to have**
- HP-07 + HP-21: logic-analyser reply latency, a reviewed disconnect/reconnect check, wire priority on Alpine.
- HP-08: request encodings with 0x5C (needs a separately reviewed build).
- HP-20: diagnostics lease and capture export on the live bus.
- HP-19 + HP-23 remainder: UI at phone widths on the Pi, support JSON check, 24 h memory record, Polkit denial checks.
- HP-33 steps 2, 3, 6, 7: history write volume, fdatasync p99, 1-year query timing, history after an app rollback.
- [-] Not applicable to this install (owner, 2026-10-29: no Home Assistant or MQTT broker in this home; the Neo optimiser controls over HTTP): MQTT on the Pi against a broker and Home Assistant (HP-38 step 3 MQTT line, HP-38a step 6 `.local`). So far MQTT has run only against a Docker broker; test it if an install ever uses MQTT.
- HP-38a step 3: SSH key and Tailscale toggles from the Access page under OpenRC.
- HP-25: HAT and wiring photos for the record.

Numbering notes: HP-09 was never assigned. Three results had been appended under the next
section and were moved to their own sections on 2026-10-25: HP-32 (it was under HP-33), the HP-34 follow-up (under
HP-35) and HP-36 (under HP-37). HP-38a precedes HP-38 on purpose (it runs inside the HP-38 trial).

---

Items that are implemented and verified locally but need the physical Pi Zero WH, RS485 HAT or F730.
The owner is remote, so each entry has exact steps.

**Run them in the order of [docs/hardware-test-plan.md](../hardware-test-plan.md)**, which adds the
go/no-go gates, rollbacks, approval points and time estimates. This file is the deduplicated checklist, renumbered and ordered on 2026-10-22. The pre-curation text is in git history (commit 9dac130).

Format: `- [ ] HP-nn What · Why hardware · Steps · Expected · Safety`. Marks: `[ ]` open, `[x]` verified on
hardware, `[-]` retired (applied only to NibePi, pi-gen, systemd or Raspberry Pi OS; the text is kept as history).
Commands in older items assume systemd. On the Alpine appliance use `rc-service`/`rc-status` and
`/var/log/messages` (or `scripts/pi-diagnostics.sh`) instead.

Global safety (updated 2026-10-25): one bus master at a time; pump writes only through the named, allowlisted
controls (47011 heat offset, 48132 Temporary Lux, alarm reset 45171 as a 0 → 1 edge), never a
raw register write and never 48852; a new control is verified one register at a time with the owner watching; never
power-cut or SIGSTOP the gateway while it services the live F730 (use a bench Pi); never reflash the production
card; disposable credentials only in terminals; never commit secrets or unsanitised captures. (Until 2026-10-24
the rule was "no register writes, never write 45171, preserve the NibePi SD card". That card has since been reused
for the appliance image, so there is no NibePi fallback.)

## Session A — first visit (existing NibePi; see plan steps A1–A6) — closed 2026-10-25 (verified or retired; see status lines)

- [-] **HP-01** [HW-0] Inventory of the existing NibePi and pump (read-only) · Only the physical install shows the HAT model, pump menus and OS config · Steps: plan **A1** (photos of HAT/jumpers/120 Ω/terminal wiring; pump menus 5.2 accessories, 5.3.11 word swap, alarm list, BT1/BT7/BT50/degree minutes/compressor hours; SSH `os-release`, `uname`, `ldd`, `config.txt`, `cmdline.txt`, serial device links, Node-RED unit name (`systemctl cat`), `hciuart`, `fuser -v /dev/ttyAMA0`, `/etc/nibepi/config.json`, `free`, `df`, root mount; NibePi UI automations and 47011/47398/47007) · Expected: AB Electronics RS485 Pi (auto direction), `disable-bt`/`miniuart-bt` overlay, no serial console, Bullseye/glibc 2.31, word swap 0 (NibePi writes 0 at every start), Modbus ticked in 5.2 · Safety: look only. Write to `docs/research/inventory-<date>.md` with Wi-Fi/MQTT credentials redacted.
  - **Status 2026-10-25:** **RETIRED** (NibePi-specific). The NibePi OS/Node-RED inventory no longer applies: the NibePi card was reused for the appliance image (PROGRESS log 2026-10-24) and NibePi does not run anywhere. The useful answers came from elsewhere: HAT = Waveshare RS485 CAN HAT, not AB Electronics (bench inventory 2026-10-24; photos still open as HP-25); Modbus active and 48852 = 0 read from the pump (HP-26 run 1); display/myUplink values cross-checked (2026-10-24 ~20:45 entry, HP-34).
- [-] **HP-02** Full backup of the NibePi SD card before any other change · The brief requires the working install to stay recoverable · Steps: plan **A2** (online file backup of `/etc/nibepi`, boot config and the Node-RED flows dir; then `poweroff`, `dd` the card on a PC, record its `sha256sum`, verify with a read-only loop mount that `/etc/nibepi` exists, `xz`; optional clone to a labelled spare card; reinsert the original and confirm NibePi is healthy) · Expected: verified image plus checksum; NibePi back in service with no persistent alarm · Safety: ⛔ owner approval (pump runs without its accessory for 20–30 min, alarm 251 likely). Never write the original card. The backup holds credentials: keep it private.
  - **Status 2026-10-25:** **RETIRED**. Waived by the owner on 2026-10-24 (entry below). The waiver relied on the untouched NibePi card as the recovery path; that card was later reused for the appliance image, so no NibePi install is left to back up. Recovery today is the A/B OS slots plus a fresh-card image (HP-40).
- [x] **HP-03** [HW-1] Passive real-traffic capture through NibePi's own logger (zero bus risk) · Real frames, periods, product string, LOG.SET content and 32-bit word order exist only on the live bus · Steps: plan **A3**. Prefer UI log levels "core"+"debug"; otherwise back up `/etc/nibepi/config.json`, set `log` enable/core/debug/info/error with the given `node -e` one-liner, and restart the Node-RED unit (this triggers NibePi's usual start-up writes 45171=1 and 48852=0, which is existing behaviour). Run ≥ 30 min, ideally across a hot-water cycle. Copy `/tmp/nibepi.log` off before any reboot, revert logging, remount ro. Developers sanitise it to `fixtures/serial-captures/real-f730-nibepi-<date>.log` and convert it to capture-jsonl · Expected: RAW lines `92,0,32,105,0,73` (read token), `92,0,32,107,0,75` (write token), `92,0,32,104,…` (data, ~80-byte payload), `92,0,32,109,…` product info every ~15 s containing "F730"; checksums validate with `docs/research/tools/nibe_ref.py`; measure token/data periods from timestamps · Safety: NibePi stays master; turn logging off afterwards (it fills RAM).
  - **Status 2026-10-25:** **VERIFIED (superseded)**. The goal was met without NibePi's logger: the gateway's own receive-only capture on 2026-10-24 ~19:00 (real frames, product string F730/9721, token/data periods; `real-f730-passive-2026-10-24.capture.jsonl` with a replay test) and HP-26 run 1 (48852 = 0, high word first). LOG.SET is empty on this pump, so there is no data-message content to capture. The NibePi-logger method is retired.
- [-] **HP-04** [HW-2] ARMv6 binary runs on the NibePi OS without touching the bus · glibc/CPU compatibility on the real Zero · Steps: plan **A4**. Build with `NIBE_PI_FEATURES=serial NIBE_TARGET_DIR=target/pi-serial ./scripts/build-pi.sh`, copy and verify its sha256, run `--device simulated --network simulated --bind 127.0.0.1:8099 --data-dir /tmp/nibe-gw`, `curl /health`, record `free -m` and RSS with Node-RED running. Create the admin account here (setup token from `/tmp/nibe-gw/setup-secrets.json`, password from a 0600 file) so the live trial needs only one login · Expected: no "Illegal instruction", no `GLIBC_x.yy not found`, health 200, RSS well below free memory · Safety: simulated only; NibePi keeps the port; never `--device hardware` here.
  - **Status 2026-10-25:** **RETIRED** (NibePi OS). The ARMv6 binary ran on the bench card (Bench 2026-10-24) and the static musl build runs the appliance on Alpine (HP-38, 2026-10-25).
- [x] **HP-05** [HW-3] Supervised ≤ 10-min read-only live trial with automatic NibePi fallback · ACK behaviour, real decoding and switch-over need the live pump · Prerequisites: HP-01..04; word swap from 5.3.11; owner watching the display. Steps: plan **A5**. Arm `systemd-run --unit=nibepi-fallback --on-active=15min systemctl start <node-red>`. In one command line, stop Node-RED, check `fuser /dev/ttyAMA0` is empty, and start the gateway as a transient unit (`--uid=pi`, `RuntimeMaxSec=600`, `ExecStopPost=+systemctl start <node-red>`) with `--device hardware --serial /dev/ttyAMA0 --network simulated --bind 127.0.0.1:8099 --data-dir /tmp/nibe-gw --word-swap <0|1|auto> --record-capture /tmp/nibe-gw/hw3-<ts>.capture.jsonl`. Log in once and poll `/api/v1/registers/explorer` (diagnostics, last_error, word order, 40004/40013/40033/43005/43416/43420/45001/48852) and `/api/v1/device/identity`; compare with the display for five samples 10 s apart. Stop with `systemctl stop nibe-trial`, confirm Node-RED is active, cancel the timer, and copy the capture plus `.summary.json` (`complete: true`) · Expected: steady `acks_sent`, zero or rare NAKs, `read_requests_sent` 0, `ack_deadline_misses` 0, `frames_for_other_addresses` silent (no RMU replies), product F730, values match after scaling, no unsolicited TX in the capture; record whether alarm 251 appeared and how long it took to clear · Safety: ⛔ owner approval. Never two programs on ttyAMA0, no writes. Abort at once on any deadline miss or `last_error`, NAK/checksum growth, frames stopping for 10 s, a new alarm other than 251, or changed heating behaviour. Reset 251 on the display only (never write 45171). A deadline miss or UART error stops the wire task permanently, so NibePi must take over.
  - **Status 2026-10-25:** **VERIFIED via HP-26 run 1** (2026-10-24 19:19–19:29: 1727 ACKs, 0 NAKs, 0 deadline misses, max reply 1884 µs, 8/8 reads) and the long run from ~19:46. It ran on the bench card instead of the NibePi OS, so the Node-RED fallback steps are retired.
- [x] **HP-06** [HW-4] 32-bit word order (48852) and data-message behaviour · Sources conflict (protocol.md 7.3) · Steps: during HP-05, or from the HP-03 log, observe 48852 (menu 5.3.11 is the primary source; current hardware builds refuse active read probes with 403, so supply `--word-swap`). Compare 43420 (compressor operating time, s32) and 43416 (starts) from read responses or 0x68 slots N/N+1 against the display · Expected: 48852 = 0 (set by NibePi); read responses high word first; data-message slots low word at N. If the data messages differ, the developers update the decoder policy (0x68 with word swap 0 currently stays raw on purpose) and protocol.md · Safety: never write 48852.
  - **Status 2026-10-25:** **VERIFIED** in HP-26 run 1: 48852 = 0, read responses high word first. Corroborated by 42075 `FF FF 00 80` (2026-10-24 ~20:45 entry) and by 43416/43420/43424 matching the display (HP-34 follow-up). Data-message slot order cannot be tested on this pump (all 0x68 frames are empty).

## Later in Session A follow-up or Session C (live pump, after captures are replayed locally)

- [ ] **HP-07** Physical reply latency, wire isolation under HTTP/SSE load, disconnect/reconnect and stale-state UI · Recorder timestamps measure userspace receipt and submission only; physical behaviour needs the bus · Steps: during a longer approved trial (Session C), put a logic analyser on the HAT UART pins and measure last-RX-bit → first-TX-bit for frames addressed to 0x20 (target < 10 ms; the real pump deadline is unknown). Open `curl -N /api/v1/events` and SIGSTOP **only that curl client**; issue 100 `/api/v1/status` requests and keep two UI clients on Live readings and the register search. Check that the explorer diagnostics show no deadline misses, drops or capture loss. Exercise transport loss and reconnect only with the reviewed procedure. Check `/health` stays 200 with `device_degraded`, that the UI shows stale warnings while keeping last values and clears them after recovery, and that SSE reconnects after a phone tab goes to background or a brief LAN outage · Expected: ACK timing unaffected by clients; freshness and degradation shown on every measurement; recovery yields fresh readings (note: today a fatal UART error needs a service restart, see F4) · Safety: never SIGSTOP the gateway or serial task on a live bus; no electrical hot-plug without the approved procedure; fallback to NibePi armed.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**, reworded for today. There is no NibePi fallback and no systemd; the appliance runs Alpine/OpenRC. If the gateway stops, the pump raises alarm 251 and keeps heating. Done: reply time measured in software (max 1884 µs, HP-26), deadline misses during Argon2 logins fixed (2026-10-24 entry). Remaining: logic-analyser RX-final-bit → TX-first-bit, a reviewed disconnect/reconnect check (the M6 "disconnection recovery" item), stale UI and recovery after a bus loss. Owner present.
- [ ] **HP-08** [HW-5] Unverified request encodings (low priority) · No source agrees on request checksum 0x5C or on escaping 0x5C in request data (protocol.md 3.8/3.9) · Steps: **needs a separately reviewed build.** The shipped encoder refuses these frames, and hardware probes return 403. Only inside an approved trial, send each of these at most once, with ≥ 1 token cycle between them: a read of 40043 with checksum `C5` (`C0 69 02 6B 9C C5`); if there is no 0x6A within 10 s, the `5C` variant; a read of 48732 raw (`C0 69 02 5C BE`+XOR), and if unanswered, escaped with LEN 02 and with LEN 03. Record ACK/NAK/silence/0x6A. Also note how the pump answers a read for an absent accessory · Expected: one variant answers, or the pump NAKs/ignores; update `nibe_ref.py` policy · Safety: ⛔ approval; reads only; stop on any alarm; never escalate to writes.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**, unchanged. It needs a separately reviewed build. All shipped reads and writes avoid 0x5C encodings by design, so nothing depends on it.

- [ ] **HP-21** Physical deadline/recovery/scheduling policy · Supersedes the old HP-05/07 statement that one miss permanently stops the wire. Defaults are hypotheses in `wire`: deadline 10 ms, three tolerated consecutive misses (fourth recovers), five reopens at exponential 250 ms backoff capped at 5 s, frame timeout 60 s, thread nice -5. During HP-05 log misses but still abort the first short trial on any miss; in approved Session C use a logic analyser and measure RX-final-bit → TX-first-bit while Argon2, capture, SSE and signed updates run. Inspect `ps -L -o pid,tid,ni,comm -p <gateway-child>` (UART -5, HTTP/capture 0, updater 10) and `/health.wire_priority_applied`; exercise the approved disconnect/reconnect procedure, check HTTP 503/recovering, resumed ACKs and total reopen count, and permanent-loss nonzero exit/systemd bounded restart. Expected: no single transient miss permanently silences the wire, no late queued reply, no competing UART owner. Record actual pump timeout and tune config only from evidence. Safety: no SIGSTOP gateway/electrical hotplug on a live bus; NibePi fallback armed, no writes.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**, reworded for Alpine. Do it together with HP-07. "systemd bounded restart" is now the supervisor exit → OpenRC supervise-daemon respawn. Check thread nice values with `ps -o pid,nice,args` (busybox `ps -L` may not exist) and `/health.wire_priority_applied`. Seen so far: 0 deadline misses in normal polling after the Argon2 fix (2026-10-24).

## Session B — bench appliance (spare Pi Zero W/WH and spare SD cards; F730 never connected) — Alpine bench work now runs through HP-40

- [-] **HP-10** Build, flash and boot the Bookworm ARMv6 image · Local generation is blocked by rootless Docker; QEMU cannot verify firmware/initramfs/SD boot · Steps: on a trusted rootful Docker host with loop devices and ARM binfmt: `./scripts/build-image.sh --check`, `./scripts/bootstrap-tools.sh`, `./scripts/build-frontend.sh`, `./scripts/build-pi.sh`, then `NIBE_RELEASE_PUBLIC_KEY=/private/release.pub ./scripts/build-image.sh` (the UI exists now, so no `NIBE_ALLOW_MISSING_UI`). Keep `<pi-gen>/deploy/*.img.xz`, the logs and the SHA-256; run `xz --test`. Flash a **spare** card with Raspberry Pi Imager (Pi Zero WH), verify it, and boot on the bench. Use the temporary offline debug-shell procedure in `docs/dev/image-build.md` (factory SSH/password logins are disabled), and remove it afterwards. From a root console: `uname -m`, `getconf GNU_LIBC_VERSION`, `lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINTS`, `findmnt /`, `findmnt /boot/firmware`, `findmnt /var/lib/nibe-gateway`, `readlink /var/lib/nibe-gateway/current` (`releases/slot-a`), ownership of `releases/` (root) and `update-state/` (0700), `systemctl status nibe-updater nibe-gateway`, `curl -fsS http://127.0.0.1:8080/health` · Expected: armv6l, glibc 2.36, three partitions, overlay root with read-only lower and ≤ 96 MiB RAM upper, read-only boot, persistent ext4 p3, HTTP 200 with ready/version/release_id. The factory unit serves the **simulator on loopback only** · Safety: never reflash the NibePi card or connect to the F730. If the overlay fails, inspect initramfs/firmware logs offline; do not disable the data-mount requirement.
  - **Status 2026-10-25:** **RETIRED**. The pi-gen/Raspberry Pi OS build path was removed from the repo on 2026-10-25 (commit c2caee1). For the record, a pi-gen appliance image was built on 2026-10-24 and booted on the owner's Pi (HP-30 run 1, HP-29). The Alpine fresh-card equivalent is HP-40 steps 1–4.
- [ ] **HP-11** systemd readiness, watchdog, stop/start and journald bounds · systemd behaviour needs the appliance · Steps: `systemctl show nibe-gateway -p ActiveState -p SubState -p MainPID -p WatchdogUSec -p NRestarts`; `journalctl -u nibe-gateway --no-pager`; `findmnt /var/log`; `journalctl --disk-usage`. Find the child with `pgrep -P "$(systemctl show -p MainPID --value nibe-gateway)"`, send **SIGSTOP to that child only**, wait 45 s, then check `NRestarts` and `/health`. Run `systemctl stop nibe-gateway`, verify `pgrep -x nibe-gateway` is empty, and start it again · Expected: ready only after health; the watchdog restarts a hung child; graceful stop leaves no process; journals are volatile and bounded; no restart loop · Safety: simulator only.
  - **Status 2026-10-25:** The systemd/journald text is **RETIRED** (Alpine has no systemd). **STILL OPEN as the Alpine equivalent (bench Pi; never SIGSTOP the gateway on the live bus)** = HP-38a step 2: `rc-service nibe-gateway status` started; SIGSTOP the gateway child → `nibe-supervise: watchdog: gateway unhealthy for 30 s` in `/var/log/messages` and supervise-daemon respawns it (new `process_instance` in `/health`); `rc-service nibe-gateway stop` leaves no `nibe-gateway` process; `/var/log` is the 16 MiB tmpfs with busybox syslogd rotation (`df -h /var/log`, `ls -l /var/log`). Verified so far: OpenRC restart and readiness through app OTA (0.1.20–0.1.22, HP-38) and the supervisor watchdog under QEMU (HP-38a).
- [x] **HP-12** Persistent mounts, reboot persistence and UART routing · Mount ordering and PL011 routing cannot be proven remotely · Steps: `findmnt /etc/NetworkManager/system-connections`, `findmnt /var/lib/NetworkManager`, `findmnt /etc/ssh`, `stat -c '%U %a' /var/lib/nibe-gateway/state`. Write a non-secret marker under `state/`, `sync`, reboot: it persists, while a marker under `/etc/` disappears. Check that SSH host keys survive reboot **if SSH is deliberately enabled**. Check `readlink -f /dev/serial0`, `stty -F /dev/serial0 -a`, `cat /proc/cmdline`, `systemctl is-enabled hciuart`, `systemctl is-enabled serial-getty@ttyAMA0` · Expected: p3 bind sources, `nibe-gateway` 700 state, PL011 ttyAMA0 free of console, getty and Bluetooth; compare pins and baud with HP-01 before any F730 connection · Safety: no bus connection.
  - **Status 2026-10-25:** **VERIFIED**. p3 persistence across reboots and OS updates: SSH host key, Tailscale identity, Wi-Fi profile and app slots carried over (HP-29 and the 0.1.1 reboot tests, 2026-10-25). On Alpine: the four p3 bind mounts, unchanged host key SHA256:LUZ0lZXS…, and `/dev/ttyAMA0` = PL011 serving the pump (HP-38 21:21, alpine.6 21:39). The systemd checks (`hciuart`, `serial-getty@`) are retired; `disable-bt applied` is checked by `scripts/alpine-trial-report.sh` (HP-38 step S).
- [ ] **HP-13** Secured first-boot AP, single-radio station transaction and UI onboarding · NM/driver/regulatory behaviour and AP→station transitions need the radio · Prerequisites: local console (never a remote host's active Wi-Fi), NM + dnsmasq-base, the correct regulatory country, and HP-12 keyfile mounts. Build `NIBE_PI_FEATURES=network-manager ./scripts/build-pi.sh`. Stop the packaged service and run as root from the console: `install -d -m700 /var/lib/nibe-gateway/bench` and `nibe-gateway --device simulated --network network-manager --network-interface wlan0 --bind 0.0.0.0:8080 --data-dir /var/lib/nibe-gateway/bench --admin-dir /var/lib/nibe-gateway/current/admin`. (Amended 2026-10-31: the AP is open and a client on it needs no setup token while no admin exists; there is no AP password.) On a phone (320–390 CSS px) and a desktop, join the open network `NIBE-Gateway-XXXX` and open `http://192.168.4.1:8080/`. Confirm the setup page shows no "Device setup token" field, `GET /api/v1/setup/status` reports `setup_token_required: false` on the AP and `true` from the LAN, public status has no secrets, and device/discovery/control/debug requests without a session return 401 (POSTs need `X-Requested-With` even when testing denial). Scan; submit a wrong password for the disposable router, then rejoin the AP and confirm state `ap` with an error and no saved SSID. Submit the correct password, rejoin the LAN, find the IP, and confirm `connected`. Finish admin setup (12+ chars); the old setup token must stop working, and a second phone on the AP must get 401 on `GET /api/v1/network/scan` · Expected: open AP (no `802-11-wireless-security`), 192.168.4.1/24 DHCP, no internet needed, wrong-password rollback on one radio, no plaintext station password in app JSON or logs. Record NM version, scan-while-AP behaviour, transition times and any driver retry · Safety: disposable router only; enter real credentials through the UI or a 0600 file passed with `--data-binary @FILE`; never publish screenshots or traces containing tokens.
  - **Status 2026-10-25:** **STILL OPEN (bench Pi)**. The owner's Pi was provisioned from `nibe-provision.json` (HP-30), so the setup AP and UI onboarding never ran on hardware. On Alpine: HP-40 step 3 (unprovisioned first boot → setup AP), then this item's wrong-password rollback and setup-token checks. Stop the gateway with `rc-service nibe-gateway stop`, not systemd.
- [ ] **HP-14** Offline behaviour, explicit recovery and factory reset · Physical outages are not simulated states · Steps: after HP-13, stop the router for ≥ 5 min. Check `nmcli -f GENERAL.STATE dev show wlan0` and `/api/v1/network/status` (saved admin cookie): state `offline`, no AP. Reboot while the router is absent: admin stays claimed and no AP opens. Restore the router and record reconnection. Restart with `--recover-network` (same data dir), join the secured recovery AP, and log in with the existing admin password. Test `POST /api/v1/network/recovery` while the station is reachable (`recovery_ap`, SSID and admin preserved). On the bench directory only: `POST /api/v1/setup/factory-reset` with `{"confirmation":"ERASE"}`; read the newly rotated credentials, rejoin, and check that the old cookie and token fail, admin and network config are null, and only gateway-owned NM profiles were removed · Expected: an outage never exposes an AP; recovery preserves ownership; reset rotates the proof; failed AP activation reports offline/setup-required · Safety: never on the original install; no GPIO reset exists.
  - **Status 2026-10-25:** **STILL OPEN (bench Pi)**. Seen so far: a reboot with the router present reconnects without an AP (0.1.1 reboot tests, 2026-10-25). The ≥ 5 min outage, the recovery AP and factory reset remain.
- [x] **HP-15** LAN mDNS, authenticated LAN listener and service-user NM permissions · Multicast and confinement need a second client · Steps: set hostname `nibe-gateway`, install `crates/network/resources/nibe-gateway.service` as `/etc/avahi/services/` (port matching the listener), and reload avahi. From another LAN machine: `avahi-resolve-host-name nibe-gateway.local`, `avahi-browse -rt _http._tcp`, `curl -fsS http://nibe-gateway.local:8080/health`, and unauthenticated `/api/v1/device` → 401. Write and review the minimal Polkit grants for the service user (checkpoint, scan, protected sharing, owned profiles), re-run HP-13/14 as that user, and deny unrelated users. Validate Secure cookies behind any TLS proxy (`--secure-cookie`) · Expected: working `.local` discovery, no secret TXT records, NM fails closed without permission · Safety: no public internet exposure.
  - **Status 2026-10-25:** **VERIFIED**. `nibe-gateway.local` resolves from the LAN on Alpine (alpine.6, 2026-10-25 21:39; avahi on wlan0 only). The gateway (uid 995) activates its own NM profile through Polkit after reboots (0.1.1 reboot tests). The owner uses the LAN login every day. Not exercised: Polkit denial for unrelated users and `--secure-cookie` behind TLS; they are carried in HP-23.
- [ ] **HP-16** Updater isolation, key-only SSH deploy, rollback, logs and debug · Real systemd sandboxing/D-Bus/SSH · Steps: on the HP-10 card, as `nibe-gateway`, confirm the updater socket connects but writing any slot, `current`, the trust key or the journal fails; the updater must not read gateway state, NM or SSH mounts. Configure key-only SSH, a known-host entry and scoped sudo for `nibe-updater client` on this bench Pi only. Run `NIBE_SSH_HOST=bench-pi NIBE_SIGNING_KEY=/private/release.key just deploy`; check post-deploy `/health` (ready/version/release_id = manifest sha256), then `just rollback`, `just logs`, `just debug`. Test timeout and bad signature without `current` changing. Record activation/rollback timing and restart ordering · Expected: deploy and rollback succeed without reflashing; failures leave the active slot intact · Safety: private keys never on the Pi, image or logs; factory SSH stays disabled.
  - **Status 2026-10-25:** **Partly VERIFIED**: key-only SSH deploy with the health/release-id check, app OTAs 0.1.1 → 0.1.22, including through OpenRC (first-OTA entry below, HP-38). **STILL OPEN (deferred by the owner; update failure tests)**: `just rollback` on hardware; a bad signature or timeout leaves `current` unchanged; updater isolation under OpenRC (the systemd sandbox checks are retired). `just logs`/`just debug` now read `/var/log/messages` through `scripts/pi-diagnostics.sh`.
- [ ] **HP-17** SD power-loss matrix for updates, and readiness rollback · Host fsync tests prove only software ordering · Steps: spare card only, with recovery media at hand. Install a known-good signed A with fixture admin/network config. Cut power repeatedly during: inactive-slot deletion, file writes, manifest/dir fsync, staged journal, activation intent, symlink replacement, restart/readiness, rollback intent/switch, final journal fsync. After each boot inspect `update-state/state.json`, `current`, slot contents and `/health`. Interrupted staging keeps the old slot and discards the candidate; an interrupted switch/check/rollback restores the old slot; a durably completed activation stays. Repeat to prove idempotence. Compare config hash/SSID with the fixture (without logging values). Upload a signed artifact whose gateway exits or never gets ready → automatic rollback, `rolled_back`. Fail both slots → `failed`/`rollback_not_ready`, not success · Expected: never an unbootable `current`; honest status · Safety: spare card only; restore it from recovery media afterwards.
  - **Status 2026-10-25:** **STILL OPEN (deferred by the owner; power-cut matrix)**. Still applies on Alpine, on a spare card. Add OS-bundle staging (`nibe-updater os-install` writing the inactive slot and `tryboot.txt`) to the cut points. Partial evidence: a power cycle during a hung OS trial returned to the committed slot (HP-38 alpine.2, 2026-10-25 19:49).
- [ ] **HP-18** Config/secret durability under power cuts, schema migration and ARMv6 Argon2 budget · SD controller durability and Argon2 cost need the Pi · Steps: `stat -c '%a %U %n'` on the bench data dir (700) and `config.json`/`setup-secrets.json` (600); NM keyfiles root 600. Create admin and connect, reboot: the password and SSID survive and old cookies do not. In a **copy** of the data dir, write a v1 config (`schema_version:1`, `admin_password_hash`, `wifi_ssid`); start and stop the binary; check migration to v2 keeps the hash, SSID and unknown keys. A corrupt config or `schema_version:99` must fail startup without resetting ownership or starting the AP. Cut power repeatedly during config save, connection and after factory reset, and during a throwaway state write : check config validity, NM profiles, `journalctl -b -p warning`, fsck results and recovery. Measure `ps -o pid,rss,pcpu`, `free -m`, poll latency and login time for 5 bad logins plus a 6th within 60 s (expect 429 + Retry-After); `/health` polling continues during Argon2 (19 MiB, t=2, p=1). Repeat for 30 min, then grep the logs for the disposable password/token/cookie canaries · Expected: bounded memory, durable versioned config, no per-request SD writes, recorded login latency · Safety: never power-cut an active F730 master or the original card. Tune Argon2 only from these measurements, through a reviewed decision.
  - **Status 2026-10-25:** **STILL OPEN (deferred by the owner; power-cut matrix)**. Seen so far: config, admin and SSID survive reboots and OS updates (HP-29, HP-38); Argon2 runs on a nice-19 thread, so logins no longer cause deadline misses (2026-10-24 entry). Remaining: power cuts during config save and factory reset, measured login latency and 429 backoff on the Pi. On Alpine use `grep -i warn /var/log/messages` instead of `journalctl`.
- [ ] **HP-19** Admin UI on real devices, and appliance footprint · Desktop tests do not establish Pi CPU/RAM behaviour · Steps: inspect overview, readings, register table, Wi-Fi and updates at phone widths and on desktop (focus, scrolling, no clipped forms or overflow). Download the support JSON and confirm it has no credentials, sessions, SSIDs, serial, raw values or free-form logs. Run the default ARMv6 release binary simulated on loopback for 30 min with one SSE stream and `/api/v1/device` + `/openapi.json` once per second; repeat with the SSE curl SIGSTOPped, then SIGCONT, then SIGTERM. Record RSS/CPU every minute and `free -m`. Keep one desktop and one phone UI open for 10 min during register search, and record 24 h `MemoryCurrent` under simulated load · Expected: bounded memory, no OOM or backlog, responsive cached HTTP, shutdown within the grace period · Safety: simulation only.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**. The owner uses the UI on the Pi (Access, Integrations and History pages live); RAM in use is 35 MiB on Alpine (HP-38). Remaining: a phone-width pass on the real Pi, a check of the support JSON content, and a 30 min/24 h RSS record (`ps -o rss`, `free -m`; `MemoryCurrent` was systemd-only).

- [ ] **HP-22** Bench probation, rollback-compatible schema/evidence durability and backend addresses · Ten-minute readiness and two restarts are hypotheses, configurable through updater `--probation-seconds` (600) and `--probation-max-restarts` (2). On the spare Pi/SD with simulated pump: activate a signed candidate, confirm `probation` for ten continuous ready minutes before `complete`; interrupt readiness, restart the gateway twice, and reboot during probation, checking rollback to the old slot each time. In a disposable copied state, exercise additive schema 3 with `min_reader_version:2`; old reader keeps admin/SSID/unknown fields and starts, while min_reader_version 3 fails closed. Repeat HP-17/18 power cuts around register-verification file rename; evidence must be old or new valid JSON, 0600, and repeated observations must not change its mtime. In the bench NM profile compare `/api/v1/status.ip_addresses` with NM IPv4/IPv6 AddressData and confirm uptime resets on restart. Run two-client login/backoff and update/UI load while measuring readiness and memory; tune probation only from recorded measurements. Expected: correct rollback, no ownership loss, atomic evidence and truthful network data. Safety: no live pump or original NibePi SD; no writes.
  - **Status 2026-10-25:** **Partly VERIFIED**: app probation completes on hardware (first OTA: phase `complete`, 0 restarts, and every later OTA). **STILL OPEN (deferred by the owner; update failure tests)**: rollback after two restarts or a reboot during probation, the schema `min_reader_version` checks on a copied state, an evidence-file rename under power cut, `ip_addresses` against NM. On Alpine, restart with `rc-service nibe-gateway restart`.

- [ ] **HP-23** Hardware/LAN image and 512 MiB resource envelope · Supersedes the old HP-10 factory-simulator note: hardware/LAN is now default, simulated/loopback an explicit bench drop-in. Hypotheses in `/etc/nibe-gateway/hardware.json`, unit nice limit, and image `NIBE_WIFI_COUNTRY=SE` (override for physical region). On a spare Pi/SD with no pump, build `NIBE_PI_FEATURES=serial,network-manager ./scripts/build-pi.sh`; create the image on rootful Docker. Confirm UART routing and country before using the hardware profile. Install the shipped simulated drop-in for no-UART provisioning/soak tests; remove it only with a PTY/bench UART peer. Repeat HP-13/14 as nibe-gateway through installed Polkit (not root), confirm own-profile save/delete, checkpoint rollback, denied modify.system/open sharing and unrelated-user denial; confirm mDNS `nibe-gateway.local`, LAN authentication and secured first-boot AP. Run `scripts/soak-local.py --duration-seconds 1800 --report /tmp/pi-soak.json` with <=96 MiB/gateway, <=256 MiB monitored total and <=16 MiB post-warmup growth; repeat 24 h, record system MemoryCurrent/free RAM and timing. Expected: correct region, discoverable authenticated appliance, no OOM or fallback to simulation. Safety: spare card, disposable router, per-device credential handoff; no F730/no writes.
  - **Status 2026-10-25:** **Mostly VERIFIED** on the owner's Pi: the hardware/LAN profile is live (pump and LAN UI), regulatory domain SE (HP-35 step 2), mDNS (alpine.6), own NM profile through Polkit (0.1.1 reboot tests), RAM 35 MiB (HP-38). **STILL OPEN (nice to have)**: Polkit denial for unrelated users and `modify.system`, the secured first-boot AP (HP-40 step 3), and a 24 h memory record. Python is not on the appliance, so record with `ps`/`free -m` over SSH instead of running `soak-local.py` on the Pi.

## Session C — live diagnostics (after HP-05 passed) — no NibePi fallback any more

- [ ] **HP-20** Physical diagnostics lease, capture export and resource limits during an update · Needs the live bus · Steps: start with a new private `--record-capture` path. In the Debug page, authorise 5 min: safe numeric/enum verbose events appear; requests without a cookie or lease cannot export captures; the lease cannot exceed 900 s; verbosity stops within 1 s after expiry, disable or logout (a restart revokes it). Export a ≤ 8 MiB capture, then replay it off-device with `--device replay --capture … --word-swap <known>` and compare provenance, timestamps and summary. Search the support JSON and journals with disposable canaries. Under an authorised update + two UI clients + recording, measure peak gateway/updater RSS/CPU, login and readiness latency, free RAM, dropped frame/capture counters and physical ACK latency (artifact max 48 MiB; one upload admitted) · Expected: no OTA, HTTP or diagnostics job delays wire replies · Safety: abort to the NibePi fallback on any timing or readiness failure; controls stay disabled.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**. There is no NibePi fallback any more; to abort, end the lease or stop the gateway. It needs a gateway started with `--record-capture` (not in the appliance's `nibe_gateway_args`).

- [-] **HP-24** NibePi OS fallback watcher · Configure actual legacy unit, gateway unit and loopback port using `packaging/nibepi/README.md`; install while root writable, leave legacy enabled/gateway disabled at boot, then remount read-only. In an approved Session C trial arm the timer after the first valid frame. Confirm healthy frames cause no systemctl handoff; gateway death or >=60 s frame silence (configurable 60000 ms; up to 6 s poll slack) stops the gateway completely before NibePi starts. Check the private runtime latch, deliberate re-arm, timer retry after stop failure and reboot defaulting to NibePi. Expected: exactly one serial owner, legacy resumes ACKs and alarm state returns to baseline; no SD writes during fallback. Safety: owner observing pump; independent short-trial dead-man retained; stop the trial immediately on abnormal heating/alarm; no writes from gateway.
  - **Status 2026-10-25:** **RETIRED**. The NibePi OS no longer runs (its card was reused for the appliance image), so the fallback watcher has no target. `packaging/nibepi/` is still in the repo.

## Bench 2026-10-24 (stock Pi OS Trixie on the bench Pi; HAT disconnected from the F730)

Results: [docs/research/inventory-2026-10-24.md](../research/inventory-2026-10-24.md). Verified on the device: Pi Zero W
Rev 1.1, `/dev/serial0 -> ttyAMA0`, GPIO14/15 = TXD0/RXD0, no UART owner, no serial console/getty, Bluetooth off,
`get_throttled` 0x0, ARMv6 build runs (simulated health/UI/401 check). This is not the appliance image, so HP-10…HP-19,
HP-22 and HP-23 stay open. The HAT is a **Waveshare RS485 CAN HAT**; HP-01's AB Electronics expectation is superseded.

- [ ] **HP-25** [bench 2026-10-24] Physical check of the Waveshare RS485 CAN HAT and pump wiring before the first live trial · Revision, links and wiring are only visible on the board · Steps: photograph both sides of the HAT on the Pi and the pump accessory terminal with the existing NibePi wiring; record board revision, whether a 120 Ω RS485 termination is fitted or linked, the RSE/GPIO4 solder links, which wires go to A, B and GND, and the Pimoroni Wide Input SHIM stacking and 12 V input wiring (owner confirmed the SHIM on 2026-10-24; this HAT takes 3.3 V from the header only); at the pump run `sudo vcgencmd get_throttled` before and after the trial · Expected: factory links (automatic direction), A/B to the pump's A/B, 12 V only on the SHIM input terminals, `get_throttled` 0x0 · Safety: look only; no-go for HP-05 if the links select GPIO4 direction control (the gateway has no direction option) or if 12 V reaches the HAT/header.
  - **Status 2026-10-25:** **STILL OPEN (nice to have)**. Its go/no-go purpose has lapsed: the HAT has carried the bus since 2026-10-24 with 0 NAK/checksum errors (HP-26 and later), the owner reported A/B on terminals 10/11, and the Pi runs on SHIM power from the pump. Photos for the record (board revision, termination, RSE links) are still missing. `vcgencmd` may not exist on the Alpine image; check before relying on it.
- [x] **HP-26** [bench 2026-10-24] HP-05 on the bench card instead of the NibePi OS · The NibePi card is not on this Pi, so HP-05's software fallback to Node-RED does not exist · Steps: as HP-05 with the gateway run from `~/nibe/current` as a transient unit (`RuntimeMaxSec=600`, `--device hardware --serial /dev/ttyAMA0 --network simulated --bind 127.0.0.1:8099`, `--record-capture` under `~/nibe/captures`); fallback is powering the Pi off and refitting the original NibePi card · Expected and abort criteria: as HP-05 · Safety: ⛔ owner at the pump display; HP-02 (NibePi card image) and HP-25 done first; no writes.
  - **Status 2026-10-25:** **VERIFIED**: run 1 on 2026-10-24 19:19–19:29 and the long run from ~19:46 (entries below).
- 2026-10-24 passive listen (owner remote; owner reports HAT A/B on pump terminals 10/11, same HAT/SHIM/wiring NibePi used for years): receive-only UART read (O_RDONLY, raw, no echo, 9600 8N1) for 60 s and then 600 s, nothing transmitted. Result **0 bytes**; `/proc/tty/driver/ttyAMA` `tx:0 rx:0` with no framing/break counts; GPIO14/15 ALT0, GPIO4 input high; `get_throttled` 0x0. The RX line stayed idle. Untested causes: Pi not actually on the pump bus or not powered from it, Modbus 40 not enabled in menu 5.2, wiring/terminal change, HAT receiver fault. Next check (needs someone at the pump): menu 5.2 and alarm 251, then refit the NibePi card and see whether NibePi receives frames.
- 2026-10-24 follow-up: owner confirms it is the same Pi/HAT/SHIM that ran NibePi, but it is now **powered from a separate supply**, not the pump's 12 V through the SHIM, so the pump GND is probably not shared. NibePi sources (`/dev/ttyAMA0` only, no GPIO use) rule out a GPIO4/RSE difference in software. Prime hypothesis for the silent RX: missing common ground/reference with A/B only. Next: restore SHIM power from the pump (12 V + GND, separate supply removed), then repeat the passive listen.
- 2026-10-24 ~19:00, after the Pi was moved to SHIM power at the pump (Wi-Fi `Rikard`): receive-only listen **received traffic** (2035 bytes/60 s, `tx:0`, one `oe` overrun from the host poll loop). Confirms the silent RX was the separate supply/missing common ground. Decoded: `F730 Cu 3x400V`, product-info version 9721; read token 0x69 and write token 0x6B every 1.0 s, 0x68 data every 3.1 s, 0x6D every 6 s, **0xEE to 0x20 every 1.5 s** (not in our expected list); no other addresses. **All 0x68 frames are empty (LOG.SET not configured)**, so a passive/ACK-only gateway sees no register values; NibePi gets values via read requests, which the gateway blocks in hardware mode (`discovery.rs`). Capture: `fixtures/serial-captures/real-f730-passive-2026-10-24.capture.jsonl`, replay test in `crates/device-f730/tests/replay.rs` (161 frames, 0 checksum errors, identity F730/9721). Partially satisfies HP-03 (real frames, product string, periods); LOG.SET content is "empty"; 32-bit word order still unknown (needs a 48852 read).
- 2026-10-24: HP-26 now has an opt-in read step. Steps: run as in HP-26 with `--allow-hardware-reads`; first 10 min ACK-only (note whether 0xEE to 0x20 stops and alarm 251 clears); then log in and `POST /api/v1/discovery/probe` with `{"register_ids":[48852,40004,40013,40033,43005,47011,47007,47398]}` (header `X-Requested-With: nibe-gateway`), poll `/api/v1/discovery/probe/status` and `/api/v1/registers/explorer`. Expected: each ID `observed`, `naks_received` 0, `ack_deadline_misses` 0, `read_requests_sent` = IDs (+ retries), values match myUplink/display (40004 outdoor temp, 47011 heat offset), 48852 gives the word order. Abort on any new alarm, NAK growth, deadline miss or `last_error`.
- 2026-10-24 19:19–19:29 **HP-26 run 1 (ACK-only 10 min, then one read session), owner watching myUplink.** Bench card, release `bench-20261024-reads-55f1e9c45e62`, transient unit `nibe-trial` (`--device hardware --allow-hardware-reads --word-swap auto`). Result: 1735 frames for 0x20, 1727 ACKs, 0 NAKs, 0 checksum errors, 0 deadline misses, max reply 1884 µs; 8 read requests, 8 pump ACKs, all 8 IDs `confirmed-readable`; capture complete (6520 records, 0 dropped); `get_throttled` 0x0. Readings: 48852=0 (**high word first**), 40004 BT1 9.6 °C, 40013 BT7 40.1 °C, 40033 BT50 20.7 °C, 43005 DM 1.2, 47007 heat curve 5, 47011 heat offset −2, 47398 room setpoint 20.0 °C (owner to compare with myUplink). Pump behaviour with ACKs: 0x6D product info slowed from 6 s to ~15 s, 0x68 data from 3.1 s to ~2.1 s, 0xEE to 0x20 from 1.5 s to ~4.7 s but **did not stop** (hypothesis: it wants NibePi's version reply). Alarm: Modbus communication alarm active since 19:00, raised while no accessory answered (before the trial); owner reset/recurrence result pending. Capture kept privately on the build host (`~/nibe-img/captures/trial-20261024-191916.*`), not yet in fixtures.
- 2026-10-24: **HP-02 waived by the owner.** The original NibePi SD card was never written or used for the bench work (the bench Pi runs a separate card), so the untouched card is itself the recovery path. NibePi feature parity is derived from the NibePi sources instead of the owner's flows.
- 2026-10-24 ~19:46 onward: **long run started** as `nibe-gateway-bench.service` (bench card, `allow_hardware_reads`, 24 poll registers). First 12 min: 1979 frames, 420 poll reads, 0 poll timeouts, 0 NAK/checksum errors, 2 reply-deadline misses (max 12.0 ms vs 10 ms), both coinciding with admin logins (Argon2 on the single core); tolerated by the 3-miss policy. Alarm register 45001 reads 251 (Modbus alarm still active).
- [x] **HP-27** First verified write: heat offset S1 (47011) · 2026-10-24 ~20:00, owner watching myUplink · `POST /api/v1/controls/heating-offset` −1 then −2: each write sent once on a write token, pump 0x6C accepted within ~2 s, read-back matched, owner confirmed −1 in myUplink, restored to −2. 0 NAKs, 0 deadline misses. Next controls follow the same steps one register at a time.
- [x] **HP-28** Alarm reset · 2026-10-24 ~20:30, owner in myUplink · `POST /api/v1/controls/alarm-reset`: 45171=1 sent once, pump accepted in 1.6 s, 45001 read back 251 → 0, owner confirmed the Modbus alarm cleared. Auto-reset of 251 (≤ 1 per 10 min) then enabled on the bench. Before this the long run had ~29 min with 996 poll reads, 0 timeouts, 0 NAK/checksum errors.
- 2026-10-24: reply-deadline misses (12–17 ms vs 10 ms) coincided with admin logins: Argon2 (19 MiB, t=2) took the single core. Hashing now runs on a short-lived thread at nice 19 (serial thread −5). Six back-to-back logins afterwards: 0 misses, max reply 0.67 ms.
- 2026-10-24 ~20:45 value cross-check with the owner in myUplink: indoor 20.7 = BT50 40033 20.7–20.8 °C; hot water 41→42 °C = BT7 40013 41.5–41.6 °C; outdoor ("climate adjustment") 10.1 ≈ BT1 40004 10.0 °C; ventilation 55 % = 47265 Exhaust fan speed normal 55 %; heat offset and alarm as in HP-27/28. "Fan speed current" (41256–41258, 43108) all read 0 % (purpose unclear on this unit). Lifetime energy: used for ventilation 1972.2, hot water 10183.9, heating 19964.6 kWh; myUplink "energy log 3949 kWh" is a period total and has no direct register match. "Hot water amount 9 min" has no register in the F730 catalog (myUplink/firmware-computed hypothesis). 42075 (EME20, absent) returns `FF FF 00 80` = −32768 high-word-first, which corroborates 48852=0; the decoder now treats it as unavailable.

## Appliance image with A/B OS, persistent state and provisioning

- [x] **HP-29** A/B boot and tryboot on the Pi Zero W · `os_prefix` for kernel/initramfs/cmdline/overlays and `reboot "0 tryboot"` are unverified on BCM2835 firmware · Steps: flash the image; confirm boot from `slot-a/` (`cat /proc/cmdline` shows `nibe.slot=a`, `root=PARTUUID=…-02`); build a bundle with `scripts/build-os-bundle.sh IMAGE KEY VERSION OUT.tar`, copy it to p3, `sudo nibe-updater os-install OUT.tar`; after the reboot confirm `nibe.slot=b`, wait for `os-confirm` (≤ 10 min) and `nibe-updater os-status` = committed; reboot normally and confirm slot b stays. Failure path: install a bundle whose gateway never becomes ready (or block :8080) and confirm the Pi returns to slot a with phase `failed`; pull power during the try and confirm a normal boot selects the committed slot · Expected: no site intervention needed in any path · Safety: run on the bench before relying on it at the pump; the shared firmware in p1 is never rewritten.
  - **Status 2026-10-25:** **VERIFIED** (result entries below). Failure paths: a power cycle during a hung trial returned to the committed slot with `failed` (HP-38 alpine.2, 19:49), and smoke boots return by themselves (HP-38 step S). An unhealthy new slot failed by os-confirm, and the network gate, are **STILL OPEN (deferred by the owner; OS-update failure tests)**.
- [x] **HP-30** First boot with `nibe-provision.json` · Steps: flash, create the file on `bootfs` with Wi-Fi `Rikard`, admin password, the build host's SSH key and a single-use Tailscale key; boot · Expected: file gone from `bootfs`; gateway on `Rikard` without the setup AP; web login with the provisioned password; `ssh nibe-admin@<ip>` works key-only and `sudo` works; `tailscale status` shows the node; after two reboots SSH host keys and Tailscale identity are unchanged; without the file the setup AP starts as before.
  - **Status 2026-10-25:** **VERIFIED**: runs 1–2 and the reboot tests on 2026-10-25 (entries below) cover Wi-Fi, admin, SSH (after the mode fix) and Tailscale. The Rust provisioner replaced the Python one and passed in HP-36 step 5. Not exercised: "without the file the setup AP starts"; that is HP-40 step 3.
- 2026-10-25 HP-30 run 1 (appliance card at the pump): boot from slot A, `nibe-provision.json` consumed; joined Wi-Fi `Rikard` (no setup AP) and Tailscale as `nibe-gateway` (100.98.91.96); owner logged in to the web UI with the provisioned password; `/health` ready, pump connected, not degraded. **SSH failed**: key offered, `Permission denied (publickey)`. Cause: the image created p3 `system/ssh` (bind-mounted to `/etc/ssh`) with mode 0700, and sshd opens `authorized_keys` with the user's uid. Fixed in the image (0755, and `nibe-persist` now enforces declared modes every boot). The running card needs a one-off `chmod 755 system/ssh` on its `nibe-data` partition (card in a reader, or local console) because no remote root path exists without SSH. HP-29 waits for SSH.
- 2026-10-25 HP-30 run 2 (after the `/etc/ssh` fix on the card): the Pi did not come back after a restart. Logs (once online): the gateway started before NetworkManager and exited twice (`Error: Unavailable`), then its single restore attempt failed with "device is strictly unmanaged" and was never retried; NetworkManager did not auto-activate the private (`permissions=user:nibe-gateway:`) profile. Recovered on site by adding a system-wide NM profile (`nibe-system-wifi`, root 0600) to `system/connections` from the laptop. Fixed in 0.1.1: `NetworkManager::new` waits up to 90 s for a managed device, and the provisioner retries the saved station every 30 s while Offline (never starts an AP from Offline).
- [x] **First OTA app update on hardware** (part of HP-16/HP-22): 0.1.0 → 0.1.1 via `scripts/deploy-release.sh` (signed with the build-host release key), staged to slot B, activated, phase `complete`, 0 restarts; health ready with matching version/release id; pump connected throughout; afterwards the gateway's own `nibe-gateway-station-Rikard` profile was active. The deploy script's final check could not read the 0750 slot manifest as `nibe-admin`; now reads it via `sudo`.
- 2026-10-25 reboot tests on 0.1.1: (1) with the `nibe-system-wifi` safety net: back in ~2 min, slot B kept, gateway activated its own profile (uid 995, success), no errors; (2) safety net deleted, reboot: back in ~2 min with only `nibe-gateway-station-Rikard`, activated by the gateway, 0 gateway errors, health ready, pump connected. HP-30 passes for Wi-Fi, admin, SSH (after the mode fix) and Tailscale; open: an image built with the fixes still has to be flashed or installed as an OS update (HP-29).
- 2026-10-25 pre-install check of the second image caught an HP-29 blocker: the new root's fstab mounted `/boot/firmware` and the data partition by the *new image's* PARTUUIDs, which do not exist on the installed card; the data mount (no `nofail`) would have dropped the trial boot into emergency mode with the watchdog still fed, i.e. no fallback and a site visit. Fixed: fstab mounts by `LABEL=bootfs`/`LABEL=nibe-data`; `build-os-bundle.sh` refuses roots that mount by PARTUUID; emergency/rescue mode now reboots after 180 s (on a trial boot this returns to the committed slot).
- 2026-10-25 **HP-29 passed** (first OS update over the air): bundle `2026.10.25-1` (2.15 GB) streamed zstd-compressed over the LAN (`zstd -dc | sudo nibe-updater os-install /dev/stdin`, ~2.5 MB/s, 15.5 min; over Tailscale the Pi's software WireGuard managed ~0.4 MB/s). An earlier aborted stream ended `failed` ("root image checksum mismatch") with no tryboot armed. After install: tryboot booted slot B (`root=…-04`, `nibe.slot=b`) with per-slot `os_prefix` kernel/initramfs/cmdline/overlays and label-based fstab; `nibe-persist` enforced `/etc/ssh` 0755; `nibe-os-confirm` committed after 600 s (`config.txt` → `os_prefix=slot-b/`, `tryboot.txt` removed); a normal reboot stayed on slot B; app slots, Wi-Fi profile, SSH keys and Tailscale identity carried over. Not yet exercised: the failure paths (unhealthy new slot, power cut during the trial).
- 2026-10-25 second OS update (slim image, B → A): bundle `2026.10.25-2` 814 MB (root image shrunk 824 → 756 MiB, 590 MiB used; boot files 20 MB, ARMv6 only), 224 MiB over the wire with zstd, install 4 min 17 s (was 15.5 min). Tryboot into slot A, `nibe-system` active, ssh/tailscaled/NetworkManager active, no failed units; `os-confirm` (now also requiring SSH and Tailscale) committed after 600 s (`os_prefix=slot-a/`). RAM in use 81 MB of 427 MB. The Access page's SSH and Tailscale panels showed live data from the helper on hardware.


## HP-31 — Alarm 251 reset as a 0 → 1 edge on 45171 (0.1.8, 2026-10-25)
**Status 2026-10-25: VERIFIED.** First trial passed at 13:35 (result below). After later gateway and OS restarts the alarm reads 0 (HP-38 21:21: "alarm 0").
Hypothesis: the F730 ignores 45171=1 while 45171 already reads 1; the myUplink reset cleared 251 when the
gateway's accepted writes did not. Steps: with 0.1.8 deployed and 251 raised (any gateway/OS restart raises it),
`journalctl -u nibe-gateway | grep -E "45171|45001|write outcome"`. Pass: "alarm reset register 45171 read" shows
the value, and 45001 changes 251 → 0 within a minute or two of an auto-reset, without the app.
Result 2026-10-25 13:35 (first trial, PASS): 0.1.7 auto-reset at 12:34:21Z (45171=1, accepted) left 251 latched.
After the 0.1.8 restart: 251 seen 12:35:44Z → 45171=0 sent → 45171=1 accepted 12:35:47Z → 45001=0 at 12:35:48Z;
45171 read back 1. Keep watching the next few 251s before calling it settled.

## HP-32 — 48132 Temporary Lux write (2026-10-25) — PASSED
**Status 2026-10-25: VERIFIED** (result below, moved here from the HP-33 section where it had been appended).
Prerequisite: 48132 is polled (it is) and reads 0. With the owner watching the pump display or myUplink, temporarily
add 48132 to `writable_registers`, then `POST /api/v1/controls` write 48132=4 (one-time increase). Pass: 0x6C
accepted, 48132 reads back 4, the display shows the hot-water boost. Then write 0 and check it reads 0 and the
boost ends. Only then ship 48132 in the image's `writable_registers`.

Result 2026-10-25 (PASS, owner watching myUplink): with 48132 temporarily allowlisted on the Pi,
`POST /api/v1/controls/temporary-lux {"mode":"one_time_increase"}` at 15:37:49 → 0x6C accepted, 48132 read back 4,
myUplink "Hot water boost" showed "One time increase". `{"mode":"off"}` at 15:39:48 → accepted, read back 0, app
showed off. 48132 is now in the image's `writable_registers` (next OS bundle); the Pi's runtime change lives in the
RAM overlay until the next reboot.

## HP-33 — On-device history storage (proposed, 2026-10-25)
**Status 2026-10-25: PARTLY VERIFIED.** History writes on the Pi and continues across the Alpine OS switch (HP-38 21:21: 239 ms flush). The boot-clock case (step 4) is covered by HP-39 (pre-sync samples dropped correctly). Alpine has chrony and no `timedatectl`/`journalctl`; read `/var/log/messages` instead. STILL OPEN: steps 2, 3, 6, 7 (write volume, fdatasync p99, query timing, history after an app rollback; nice to have); step 5 belongs to the power-cut matrix (deferred by the owner). Step 1 baseline was not recorded.
Run once the history store exists, on the bench Pi first, then at the pump. Numbers to compare against:
docs/research/history-storage.md §5.
1. Baseline before enabling history: `findmnt /var/lib/nibe-gateway` (expect ext4 `noatime`), `sudo tune2fs -l
   /dev/mmcblk0p3 | grep -E "Journal size|Reserved block count|Block size"`, `df -h /var/lib/nibe-gateway`,
   card identity `cat /sys/block/mmcblk0/device/{name,manfid,oemid,date}`.
2. Write volume: read field 7 (sectors written ×512 B) of `/sys/block/mmcblk0/mmcblk0p3/stat` and of
   `/sys/block/mmcblk0/stat`, wait 1 h, read again; once with history off, once with `flush_secs` 60, once with
   300. Pass: added p3 writes ≤ 40 MiB/day at 60 s (estimate 7–24); otherwise switch the default to 300.
3. Latency: history status shows `fdatasync` p99 < 500 ms; `journalctl -u nibe-gateway` shows no more reply-deadline
   misses than without history over the same hour.
4. Boot clock: boot with Wi-Fi unavailable; note `date` and `timedatectl show -p NTPSynchronized`, whether
   `/run/systemd/timesync/synchronized` exists, then restore Wi-Fi. Pass: no history records with pre-sync wall
   times; buffered samples appear re-anchored after sync; status shows clock synced.
5. Power cuts (bench, 10×, random moments, incl. right after a flush and at midnight rollover): after boot,
   `dmesg | grep -i ext4` shows only journal recovery, gateway ready, history status reports the recovered tail,
   and the last stored sample is ≤ flush interval + 1 round before the cut.
6. Query timing with 2 years of synthetic files copied to `state/history/`: `time curl` of a 1-year 40004 graph
   (auto resolution) < 2 s; gateway RSS grows < 10 MB during the query.
7. After the next OS update and an app rollback: history before the update is still served.

## HP-34 — Register survey: compare open values with the display/myUplink (2026-10-25)
**Status 2026-10-25: PARTLY VERIFIED.** Items 1 (normal mode), 2, 3, 5 and 6 are resolved (results below, including the follow-up that had been appended under HP-35). **STILL OPEN (needs the owner at the pump)**: fan modes 1–4 (47260/47261–47264), energy counters 41846/41848/41850 against the myUplink energy log, and 43147 with a clamp meter while the compressor runs.

Context: `docs/research/register-survey-2026-10-25.md` (all 576 readable registers answered, 0 timeouts). These
need the owner to read the same value in myUplink or on the display at the same time as
`GET /api/v1/registers/explorer` (read-only), ideally once while heating and once while charging hot water:
1. **Fan speed exhaust air** (app) vs 47265 (55 %) and 47260 fan mode. Then set a temporary fan speed on the
   display: expect 47260 to change to 1–4 and the app to show 47261–47264. 41256–41258/43108 always read 0 %.
2. **Time factor add heat** (app/service menu) vs 43081 (1438.7 h) and 43239 (236.8 h).
3. **Flow sensor BF1** (app) vs 40072, which always reads 5.0 l/min. If the app shows nothing or 5.0, there is no BF1.
4. **Compressor power/current**, if shown anywhere (display service info): 43141 raw 76–79 at 35 Hz (survey says
   0.76–0.79 kW, i.e. scale 100), 43147 raw ~5200 (mA hypothesis).
5. **Smart price adaption**: is it enabled in myUplink, and what heating offset does it show now? Compare with
   44896 (raw 1), 41929 (level 1), 44908 (state 30).
6. **Degree minutes** (display menu 4.9.3) vs 43005 and 40940 (they disagree).
Pass: each value matches the register (after the scale noted) or the register is marked not meaningful.
Result 2026-10-25 (PARTIAL, display photos of menu 3.1 Serviceinfo pages 1–16, 16:01–16:03 CET, compared with the
survey, the 16:05 poll and a 32-ID read-only probe at 16:07; details in
`docs/research/panel-verification-2026-10-25.md`):
- Item 1: display "fläkthastighet 55 %" + "ventilation normal" = 47265 (55) with 47260 = 0 → MATCH; 41256–41258/43108
  are not the live fan speed. Modes 1–4 still untested.
- Item 3: display "BF1 5.0 l/min" = 40072 → the gateway reads the pump's own value; whether 5.0 is measured is open.
- Item 5: Serviceinfo page 10 shows smart price adaption status 30 (= 44908), värme 0.1 (= 44896 raw 1, so ×0.1 is
  right), kyla 0.0, pool 0.0, varmvatten 0 (= 44897).
- Item 6: display "gradminuter 1" while 43005 read raw 14 (1.4, frozen during the HW charge) and 40940 read 0 →
  43005 ×0.1 MATCH, 40940 not maintained.
- Also VERIFIED: heat meters 44298/44300/44306/44308 (kWh ×0.1, display truncates), phase currents L1/L2/L3 =
  40083/40081/40079 (A ×0.1), 47212 6.5 kW, 47214 16 A, 43122 20 Hz, 32260 = 1, 43124 180.7, 40050, 43009,
  43086 20 = "varmvatten", 47260 0 = "normal", and all temperatures by trend.
- Still open (not on menu 3.1): items 2 and 4. Next photos, each with the time noted: 3.2 kompressorinfo (hours,
  HW hours, starts vs 43420/43424/43416; any power/current line vs 43141/43147), 3.3 tillsatsinfo (time factor vs
  43081/43239), 1.9.1 kurva (vs 47007 = 5, 47011 = −2), 5.1.1 varmvatteninställningar (vs 47043–47049), and for
  43141/43147 a photo of Serviceinfo page 5 and 7 *while the compressor runs*, taken together with a read of
  43141/43147/40079–40083 (`POST /api/v1/discovery/probe` with those IDs) within the same minute.
Result 2026-10-25 follow-up (second photo set 16:21:49–16:23:09 CET: menus 3.2, 3.3, 1.9.1.1, 5.1.1; compressor
stopped; compared with gateway 0.1.13 `/api/v1/device` and a 14-ID read-only probe at 16:26:40; details in
`docs/research/panel-verification-2026-10-25.md` §2 "Follow-up photos"):
- VERIFIED: 43416 starts 32325 (×1), 43420 46953 h and 43424 14054 h (h ×1), 47007 curve 5 and 47011 offset −2
  (×1), 47043–47049 HW start/stop temps (°C ×0.1; eco 43/47, normal 47/51, lux 50/54, periodic stop 55).
- Item 2 PASS: display 3.3 "tidfaktor 1438.7 / av vilken varmvatten 236.8" = 43081 / 43239 (×0.1). The value is a
  time factor (no unit on the display), not plain hours.
- Item 4 still OPEN by display: no photographed screen shows compressor power or current. 43141/43375 kW ×0.01 is
  strongly supported by cross-checks (41848 rose ~0.3 kWh over a ≤ 25 min HW charge; L2 current delta ~3.4 A ≈
  0.78 kW / 230 V). 43147 (mA?) does not match the L2 delta and stays a hypothesis. 41846/41848/41850 are not on
  the display; compare with the myUplink energy log if a display/app comparison is wanted.
- Remaining steps: (a) fan modes 1–4 (set a temporary speed in menu 1.2, read 47260 and Serviceinfo page 4/16);
  (b) myUplink energy log vs 41846/41848/41850; (c) for 43147 only an external clamp meter on the compressor
  supply/motor lead while it runs, read in the same minute as a probe of 43147, would settle it.

## HP-35 — Image slimming and squashfs root (steps 1–4 accepted and implemented, squashfs proposed; 2026-10-25)
**Status 2026-10-25: VERIFIED.** Step 2 passed (bundle -4, result below), step 3 passed as HP-37, step 5 numbers are recorded. Step 1 **RETIRED**: Alpine bundles install in ~45 s, so the ext4-era bottleneck no longer matters. Step 4 **RETIRED**: the ext4/Raspberry Pi OS bundles are off the card, and Alpine slot-to-slot updates (alpine.5 → .6 → .7) exercise both directions. The `systemctl` commands in step 2 are historical.

Context: `docs/research/image-size-2026-10-25.md`. Step 1 needs no new build and can run now; steps 2–5 run once an
image with the slimming steps is built. Read-only unless stated; OS updates as in HP-29 (tryboot, os-confirm).
1. Bottleneck of an OS install today (no changes to the system; uses p3 scratch space, delete afterwards). On the Pi:
   `time sha256sum` of a 200 MB file on `/var/lib/nibe-gateway`; `time zstd -dc file.zst > /dev/null` for a
   200 MB zstd-3 file copied from the host; from the host `time ssh … 'cat > /dev/null' < 200MB-random-file` (wire +
   sshd only); `top -b -n 5 -d 5` during the next real `os-install` to see sshd / zstd / nibe-updater CPU share.
   Also `NetworkManager --print-config | grep -i dhcp` and `nmcli -f DHCP4 device show wlan0` (expect internal DHCP;
   if `dhclient` is in use, keep `isc-dhcp-client`). Pass: the limiting stage is identified and noted here.
2. OS update to an image with the slimming steps 1–4 (ext4 still): the exact checks are in "HP-35 step 2" below.
3. OS update to the first squashfs bundle: `findmnt -o SOURCE,FSTYPE /` lower layer shows squashfs (or
   `cat /proc/mounts`), `grep -i fsck /run/initramfs/fsck.log; journalctl -b | grep -iE 'fsck|squashfs'` shows no
   failure, `systemd-analyze` total vs the ext4 slot (note both), `free -m` vs the ext4 slot; os-confirm commits.
4. Rollback: `sudo nibe-updater os-install` of the previous ext4 bundle into the other slot, boot it, confirm; then
   install the squashfs bundle again. Both directions must commit; a deliberately broken squashfs bundle (wrong
   `rootfstype`) must fall back to the running slot without manual action.
5. Record install time and bytes on the wire for each bundle next to HP-29's 4 min 17 s / 225 MiB.
### HP-35 step 2 — first OS update with slimming steps 1–4 (implemented 2026-10-25, not yet built)

What changed in the image: stripped Tailscale; no kernel/initramfs/boot-firmware copies in the root; Wi-Fi
firmware reduced to the Zero W 43430 files; udev hwdb sources gone; 68 packages purged/autoremoved (Perl,
ICU/libxml2/shared-mime-info, ModemManager, locales → `LANG=C.UTF-8`, libmagic, console/keyboard, X11/xauth,
python3-requests family, strace, htop, wget, ppp, parted, pciutils/usbutils, …). Kept: libmm-glib0, gnupg,
ncurses-term, isc-dhcp-client, python3. Build-time guards already passed if the image exists (nibe-slim package
check, nibe-prune "73 required runtime files present" in the build log — check it is there).

Install from the host (bench Pi first if available, then the pump Pi), timing it:
`time NIBE_SSH_HOST=nibe-admin@nibe-gateway.local bash -c 'source scripts/ssh-maintenance.sh && nibe_os_install BUNDLE.tar'`
Record: bundle size, `zstd -19 -c BUNDLE.tar | wc -c`, install time (compare 4 min 17 s / 225 MiB, HP-29 log).

After the tryboot reboot (`nibe.slot` flipped), over SSH, all read-only:
1. `cat /proc/cmdline` (new slot) · `sudo nibe-updater os-status` → `trying`, then `committed` after ≤ 10 min.
2. `systemctl --failed` empty; `systemctl is-active NetworkManager wpa_supplicant ssh avahi-daemon polkit
   systemd-timesyncd tailscaled nibe-gateway nibe-updater nibe-system` all `active`;
   `systemctl is-active fake-hwclock nibe-persist` `active` (oneshot, exited); `systemctl list-units --all | grep
   -iE 'ModemManager|console-setup|keyboard-setup'` shows nothing loaded or only "not-found inactive".
3. Wi-Fi: `nmcli -t -f DEVICE,STATE device` → `wlan0:connected`; `cat /sys/module/cfg80211/parameters/ieee80211_regdom`
   → `SE`; `dmesg | grep -iE 'brcmfmac|firmware'` shows `brcmfmac43430-sdio` loaded and **no** "Direct firmware
   load … failed" for a file the driver actually needed (a failed optional lookup followed by a successful load is
   fine — note the exact lines).
4. DHCP client (decides isc-dhcp-client): `NetworkManager --print-config | grep -i dhcp` and
   `journalctl -b -u NetworkManager | grep -iE 'dhcp(4|6)? .*(init|started)|dhclient|internal'`. If NM uses the
   internal client and `pgrep -a dhclient` is empty, isc-dhcp-client can be purged in a later image.
5. Tailscale: `sha256sum -c /usr/share/nibe-gateway/tailscale.sha256sums` → both OK; `tailscale version` →
   1.104.2; `tailscale status` shows the node online; the Access page's Tailscale panel shows live data.
6. SSH: key login works from the laptop over LAN and over Tailscale; `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`
   unchanged from before the update (host key persisted on p3).
7. Locale: `locale` prints `LANG=C.UTF-8` with no "Cannot set LC_*" warnings (if the laptop forwards `LANG`/`LC_*`
   via SendEnv, warnings about en_* locales are expected and harmless — note them); `journalctl -b -p warning`
   has no locale or missing-binary errors (`grep -iE 'not found|No such file|locale'`).
8. mDNS: `avahi-resolve -n nibe-gateway.local` from the laptop (or open http://nibe-gateway.local:8080).
9. Gateway: `/health` ready, pump connected, live readings updating; `free -m` (compare 81 MB used in HP-29's
   second update; expect a few MB less without ModemManager).
10. Root size: `df -h` unchanged in form (overlay root), and on the host `tar -tvf BUNDLE.tar` → `rootfs.img`
    size (expect ~400 MiB instead of 757 MiB).
11. Rollback still possible: leave the previous slot untouched; if any check above fails, reboot without confirming
    (or wait for os-confirm to fail it) and confirm the Pi returns to the old slot.
Pass: 1–9 hold and os-confirm commits. Record numbers here.

Result 2026-10-25 (HP-35 step 2, bundle 2026.10.25-4, PASS): slimmed ext4 root (373 MiB image, 412 MB bundle,
was 616 / 814) installed over Wi-Fi with `nibe_os_install` (zstd -19) in 2 min 30 s (was 4 min 17 s); tryboot into
slot A, committed. All services active (nibe-gateway, NetworkManager, ssh, tailscaled, nibe-system, avahi, timesyncd);
LANG=C.UTF-8; regulatory domain SE; only brcmfmac43430 firmware; no /boot/vmlinuz in the root; tailscale 1.104.2
with both stripped binaries matching tailscale.sha256sums; p3 430 MiB free; RAM used 74 MiB (was 81). No dhclient
process while wlan0 holds its LAN address: NetworkManager uses its internal DHCP client, so isc-dhcp-client can be
purged in a later bundle (kept out of the Python-removal bundle to keep that change isolated).

## HP-36 — Python removed: Rust supervisor and provisioning (2026-10-25)
**Status 2026-10-25: VERIFIED** (bundle -5, result below, moved here from the HP-37 section). The Raspberry Pi OS slot it ran on is retired; `nibe-system supervise`/`provision` continue on Alpine.
Install the bundle built after the Python-removal commit (`nibe_os_install`), then on the Pi:
1. `command -v python3` prints nothing; `systemctl show nibe-gateway -p ExecStart` names `nibe-system supervise`.
2. `systemctl show nibe-gateway -p StatusText -p ActiveState -p NRestarts` → "Gateway HTTP health ready", active,
   0 restarts after 10 minutes (the watchdog is refreshed by healthy probes only).
3. `ps -o rss=,args= -C nibe-system` → the supervisor's RSS (Python used 15 MiB).
4. `sudo systemctl restart nibe-gateway` stops cleanly (no SIGKILL after TimeoutStopSec) and becomes ready again.
5. Provisioning path without changing anything: `echo '{}' | sudo tee /boot/firmware/nibe-provision.json` after
   `sudo mount -o remount,rw /boot/firmware` (then remount ro), reboot → journal shows
   `nibe-provision: applied nothing`, the file is gone and /boot/firmware is mounted read-only again.
   Do NOT put keys in the test file: provisioning replaces the authorized_keys file.
6. OS confirm commits; app OTA (`deploy-release.sh`) still passes its health check.
Result 2026-10-25 (HP-36, bundle 2026.10.25-5, PASS): 378 MB bundle / 340 MiB root installed in 2 min 15 s, slot B
committed. 1: no python3; ExecStart is `nibe-system supervise`. 2: "Gateway HTTP health ready", active, 0 restarts.
3: supervisor RSS 2.1 MiB (Python: 15 MiB); RAM used 66 MiB (81 before the slimming bundles). 4: manual restart
clean, ready again, no watchdog/SIGKILL lines. 5: `{}` provisioning file → "nibe-provision: applied nothing", file
deleted, /boot/firmware back to ro. 6: host-side deploy health check matches the running release (0.1.15).

## HP-37 — Squashfs root slot, mixed slots and rollback (implemented 2026-10-25, not yet built)
**Status 2026-10-25: VERIFIED** for steps 1–3 (result below). Step 4 **RETIRED** (the ext4 Raspberry Pi OS bundle is off the card; see HP-35). Step 5 is a host-side refusal covered by tests (WS03 log), not a hardware step.

The first squashfs bundle is installed by the updater already on the Pi (ext4 slot); the
bundle format is unchanged. Expect a ~95–100 MiB `rootfs.img` (373 MiB ext4 before). Build log must show
`nibe-squash: kernel mount check passed` (or the WARNING line), `nibe-squash: <bytes> bytes, files=…` and
`nibe-prune: 70 required runtime files present` twice (the mounted root, then the unpacked squashfs).
On the host before installing: `tar -tvf BUNDLE.tar` (rootfs.img ~100 MiB) and
`tar -xOf BUNDLE.tar rootfs.img | head -c 4 | od -c` → `h s q s`.

1. Install ext4 → squashfs: `time NIBE_OS_ZSTD_LEVEL=1 NIBE_SSH_HOST=nibe-admin@nibe-gateway.local bash -c 'source
   scripts/ssh-maintenance.sh && nibe_os_install BUNDLE.tar'`. Record time and bundle size (compare 2 min 30 s /
   412 MB). If the Pi does not come back within ~5 min: power-cycle it once; it must come back on the old slot
   (tryboot is one-shot; `panic=10` should already have rebooted it) with `os-status` = failed.
2. On the new slot (all read-only):
   - `cat /proc/cmdline` → `rootfstype=squashfs panic=10 nibe.slot=<new>`;
     `cat /proc/mounts | grep -E ' / |squashfs'` → `/` is `overlay`; `ls /run/initramfs` / `dmesg | grep -i squashfs`
     shows the squashfs mount; `sudo blkid /dev/mmcblk0p2 /dev/mmcblk0p4` → one `TYPE="squashfs"`, one `ext4`.
   - fsck: `cat /run/initramfs/fsck.log 2>/dev/null; journalctl -b | grep -iE 'fsck|squashfs'` → no failure
     (expected: "fsck.squashfs … not found; ignore" or nothing); `systemctl --failed` empty.
   - Services: `systemctl is-active NetworkManager wpa_supplicant ssh avahi-daemon polkit systemd-timesyncd
     tailscaled nibe-gateway nibe-updater nibe-system` all active; `systemctl is-active nibe-persist fake-hwclock`.
   - Persistence (unchanged by design): `findmnt /etc/ssh /var/lib/NetworkManager /var/lib/tailscale
     /etc/NetworkManager/system-connections` → all bind mounts from mmcblk0p3; SSH host key fingerprint
     (`ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`) unchanged; `cat /etc/machine-id` non-empty;
     `ls -l /etc/fake-hwclock.data /var/lib/systemd/timesync/clock` exist (in the RAM upper, as before).
   - Write test of the lower layer (must fail, read-only by design): `sudo touch /test && ls /test` works (overlay),
     reboot later shows it gone.
   - `systemd-analyze` and `free -m` — note both and compare with the ext4 slot (expect boot within ±15 s).
   - App OTA still works: `scripts/deploy-release.sh` (or wait for the next app release) passes.
3. `sudo nibe-updater os-status` → committed after ≤ 10 min (os-confirm: nibe-system, SSH, Tailscale healthy).
4. Rollback squashfs → ext4: install bundle `2026.10.25-5` (ext4, Python-free) again with `nibe_os_install` into the
   other slot → it boots `rootfstype=ext4` from that slot, commits. Then install the squashfs bundle again → boots
   squashfs, commits. Both directions must work without touching the card.
5. Optional failure path (bench only): a squashfs bundle whose slot cmdline says `rootfstype=ext4` must be refused
   by `nibe-release os-sign` on the host (and by `os-install` on an updater with this change) before anything is
   written.
Pass: 1–4 hold. Record install times, sizes and `systemd-analyze` numbers here.
Result 2026-10-25 (HP-37 steps 1–3, bundle 2026.10.25-6, PASS): squashfs root 93 MiB (ext4 was 340 MiB), bundle
118 MB, installed with NIBE_OS_ZSTD_LEVEL=1 in 1 min 16 s (ext4 bundles: 2 min 15 s – 4 min 17 s); tryboot into slot A,
committed. cmdline carries rootfstype=squashfs panic=10; / is the overlay; p2 squashfs, p4 ext4 (bundle -5) side by
side. systemctl --failed empty; nibe-gateway, NetworkManager, ssh, tailscaled, nibe-system, avahi, timesyncd active;
p3 bind mounts present; SSH ED25519 host key SHA256:LUZ0lZXS… unchanged; RAM used 69 MiB; app health matches.
Not yet run: step 4 rollback round trip (ext4 -5 ↔ squashfs -6) — deferred; the Alpine trial (HP-38) exercises
the same fallback path.

## HP-38a — Rust side of the Alpine/OpenRC base, part of the HP-38 trial
**Status 2026-10-25: PARTLY VERIFIED.** Step 1 **RETIRED** (Raspberry Pi OS slot gone; the musl releases ran there before the switch, HP-38 preconditions). Step 4 VERIFIED (0.1.20 activated through OpenRC in ~70 s, then 0.1.21 and 0.1.22). Step 5 VERIFIED (alpine.6 and alpine.7 installed from the committed Alpine slot, tryboot, committed in 85 s / 95 s). **STILL OPEN**: step 2 = HP-11 Alpine equivalent (bench Pi); step 3 SSH key and Tailscale toggles from the UI under OpenRC (nice to have); step 6 MQTT `.local` (nice to have, with the MQTT check).
Added 2026-10-25. Built and tested locally; nothing here has run on a Pi or under a real OpenRC.
Verified locally: init detection and the OpenRC command lists (unit tests), the supervisor's own watchdog (unit and
Python tests, plus the musl binaries under `qemu-arm-static -cpu arm1176`: a SIGSTOPped simulator gateway was
stopped 17 s later, i.e. 6 s watchdog + 10 s SIGTERM grace, exit 1). Alpine paths/names were read from the
Alpine 3.24 armhf packages (openrc 0.63.2: `/sbin/rc-service`, `/sbin/rc-update`, `/sbin/supervise-daemon`;
busybox 1.37.0: `/sbin/reboot`, `/bin/mount`, `/bin/sync`; init scripts `/etc/init.d/sshd`, `/etc/init.d/tailscale`).
Hypotheses to check on the bench Pi (musl build: `NIBE_PI_LIBC=musl scripts/build-pi.sh`):
1. Raspberry Pi OS slot with the musl binaries: `systemctl restart nibe-gateway` → ready, `WATCHDOG=1` still sent
   (no watchdog kills in `journalctl -u nibe-gateway`); SSH key add/remove and Tailscale join/disable from the UI
   behave as before; `sudo nibe-updater os-status` works.
2. Alpine slot: `ls -d /run/systemd/system` fails (OpenRC detected). `rc-service nibe-gateway status` → started.
   `kill -STOP $(pgrep -f 'nibe-gateway --')` → within ~45 s a `nibe-supervise: watchdog: gateway unhealthy for 30 s`
   line in the gateway log and supervise-daemon respawns it (`rc-service nibe-gateway status` started, new
   `process_instance` in `/health`).
3. Alpine slot: UI SSH key removal → `rc-service sshd status` stopped, existing SSH session survives; adding a key →
   started. Tailscale join → `rc-service tailscale status` started; disable → stopped. Reboot: both follow the
   provisioning markers (image's job, see the init scripts), not the per-boot `rc-update` state.
4. Alpine slot: app OTA activate → `rc-service nibe-gateway restart` by nibe-updater, probation passes.
5. Alpine slot: `nibe_os_install BUNDLE` (scripts/ssh-maintenance.sh) → os-install exits 0 (the SSH command
   reports success), then a detached child stops the gateway and reboots via reboot(2) RESTART2 `"0 tryboot"`
   into the other slot (`nibe.slot=` in `/proc/cmdline`). On failure a `nibe-updater: reboot ... failed` line is
   in `dmesg`.
   `os-confirm` then commits (gateway, `rc-service sshd status`, `rc-service tailscale status`) or plain-reboots
   via busybox `reboot` back to the committed slot.
6. MQTT broker given as `*.local`: musl has no NSS (`libnss-mdns`), so expect it not to resolve on the musl build
   (both bases). Use an IP or DNS name, or record the failure.
Pass: 1–5 hold; record 6.

## HP-38 — Alpine OS bundle in the inactive slot: trial, checks, commit or rollback (added 2026-10-25)
**Status 2026-10-25: VERIFIED**: Alpine is the live base in both slots (alpine.5 21:21, alpine.6 21:39, alpine.7 at HP-39). From step 4, "OS rollback to Raspberry Pi OS" is **RETIRED**. **STILL OPEN**: the forced-failure network gate and an unhealthy-slot os-confirm failure (deferred by the owner; OS-update failure tests); the setup AP and `{}` provisioning on Alpine (HP-40 steps 3–4, bench Pi); the MQTT/Home Assistant check on the Pi (nice to have).

**Updated 2026-10-25 after the failed first trial (result at the end).** Order: step S (smoke
bundle) → read the traces → steps 0–4 with the real bundle. Both use the Raspberry Pi OS kernel `6.12.109+rpt-rpi-v6`
(the one slot A runs, pinned in `packaging/alpine/kernel.lock`) under the Alpine userspace; no `quiet` (HDMI shows
the boot).
- Smoke (current: alpine.5, after the alpine.4 UART finding): `NIBE_SMOKE=1
  NIBE_TAILSCALE_DIR=~/.cache/nibe-gateway/pi-gen/stage-nibe/00-install/files/payload/tailscale
  scripts/build-alpine-os-bundle.sh ~/.config/nibe-gateway/release.key 2026.10.25-alpine.5-smoke`
  → `target/alpine/nibe-os-alpine-2026.10.25-alpine.5-smoke.tar` (51.8 MiB). Never commits, reboots back by itself.
  Full overlays directory and disable-bt pre-applied to the dtb.
- Real (only after the smoke passes): `NIBE_TAILSCALE_DIR=~/.cache/nibe-gateway/pi-gen/stage-nibe/00-install/files/payload/tailscale
  scripts/build-alpine-os-bundle.sh ~/.config/nibe-gateway/release.key 2026.10.25-alpine.5` →
  `target/alpine/nibe-os-alpine-2026.10.25-alpine.5.tar` (~51.8 MiB, same content minus `nibe.smoke=1`).
- Traces: `NIBE_SSH_HOST=nibe-admin@nibe-gateway.local scripts/alpine-trial-report.sh [--fetch ~/nibe-trials]` from the
  slot that is running afterwards (read-only).
Already verified without the Pi (QEMU raspi0 with this kernel, initramfs and squashfs): initramfs stages written to
`p1:/nibe-trial/a-1.log`; PARTUUID root, 96 MiB overlay, label mounts (`/boot/firmware` ro), eudev
`/dev/disk/by-partuuid`, nibe-persist bind mounts, all services start; boot-logs on p3 every 30 s and at shutdown;
a "trying" trial whose gateway stays unhealthy is failed by os-confirm and rebooted (shutdown snapshot kept); a smoke
boot skips os-confirm and reboots 120 s after the default runlevel; the static musl gateway answers `/health` ready
(simulated profile, earlier run). NOT verifiable in QEMU: the firmware → kernel stage (QEMU loads the kernel itself —
where alpine.2 hung), Wi-Fi/brcmfmac, the setup AP, the UART/HAT, the hardware watchdog, tryboot, real timings/RAM.
HP-38a (Rust side) runs inside this trial.
The Raspberry Pi OS slot stays committed until step 4; never touch the Pi while the pump is in a critical phase.

**0. Preconditions (on the Raspberry Pi OS slot, read-only checks first)**
1. App release must be static musl: the gateway binary in `current/` comes from p3, not from the bundle, and a glibc
   build cannot run on Alpine (no `ld-linux-armhf.so.3`). On the host: `python3 packaging/alpine/alpine_image.py
   check-elf target/pi-musl/arm-unknown-linux-musleabihf/release/nibe-gateway`; deploy that build with
   `scripts/deploy-release.sh` (HP-38a step 1 covers it on Raspberry Pi OS). On the Pi:
   `ldd /var/lib/nibe-gateway/current/nibe-gateway` → "not a dynamic executable"; also check `previous` if present.
2. Ids: `id nibe-gateway` → `uid=995 gid=992`, `stat -c %u:%g /var/lib/nibe-gateway/state` → `995:992`. If not,
   rebuild with `NIBE_GATEWAY_UID=… NIBE_GATEWAY_GID=…`.
3. Boot config: `cat /boot/firmware/config.txt` has `auto_initramfs=1`, `enable_uart=1`, `dtoverlay=disable-bt`, the
   `# nibe-slot` marker; `cat /proc/cmdline` → note the committed slot (expected `b`) and the disk id;
   `sudo nibe-updater os-status` → not `trying`; `df -h /boot/firmware` has ≥ 20 MiB free.
4. Record the "before" values: `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`; `tailscale version; tailscale
   status | head -3`; `nmcli -t -f NAME,TYPE,DEVICE con show --active`; `free -m`; `curl -s localhost:8080/health`;
   `ls /var/lib/nibe-gateway/state/history | tail -2`; MQTT status in the UI; `sudo nibe-updater client status`.
5. Tailscale version gap: Alpine ships 1.98.10, Raspberry Pi OS 1.104.2 (state on p3 is shared). To avoid the
   downgrade, build with `NIBE_TAILSCALE_DIR=~/.cache/nibe-gateway/pi-gen/stage-nibe/00-install/files/payload/tailscale`
   (51.1 MiB bundle). Recommended for the first trial.

6. Optional, bench Pi only (needs a Rust change first, not in these bundles): early firmware watchdog for the trial,
   see the research doc §7. Test on the bench by adding `kernel_watchdog_timeout=60` to `tryboot.txt` by hand
   right after an `os-install --no-reboot` of the smoke bundle, then `sudo systemctl reboot --reboot-argument="0 tryboot"`; look for "Enabling
   early watchdog" on HDMI / `watchdog.open_timeout=60` in the Alpine `/proc/cmdline` (smoke boot-log meta.txt).

**S. Smoke boot first (owner at the HDMI screen if possible; nothing is committed)**
1. Preconditions 0.1–0.4 below hold (a smoke boot runs the gateway too). The gateway that runs is the app release
   in p3 `current/`, NOT the bundle's factory copy: to run the musl build with bf656d6 (serial errors name the port),
   deploy it first on Raspberry Pi OS (`scripts/deploy-release.sh`, musl build from `NIBE_PI_LIBC=musl
   NIBE_PI_FEATURES=serial,network-manager scripts/build-pi.sh`) and check `/health` there.
2. `time NIBE_OS_ZSTD_LEVEL=1 NIBE_SSH_HOST=nibe-admin@nibe-gateway.local bash -c 'source scripts/ssh-maintenance.sh &&
   nibe_os_install target/alpine/nibe-os-alpine-2026.10.25-alpine.5-smoke.tar'`.
3. Expected: the Pi tryboots the Alpine slot, reaches the default runlevel, takes its final snapshot once
   NetworkManager is connected and `/health` is ready (or 120 s after the default runlevel) and reboots by itself
   into Raspberry Pi OS: back on SSH within ~5–8 min. The boot guard reboots it if the default runlevel is not
   reached in 300 s, and after 900 s in any case. HDMI shows kernel + OpenRC messages (`quiet` removed).
   If nothing answers after 15 min: power-cycle once (it then boots the committed slot).
4. Back on Raspberry Pi OS: `sudo nibe-updater os-status` → `failed` ("tried slot did not boot" — expected for
   a smoke boot, it never commits). Then `NIBE_SSH_HOST=nibe-admin@nibe-gateway.local scripts/alpine-trial-report.sh
   --fetch ~/nibe-trials` and record:
   - `p1 nibe-trial/<slot>-<n>.log` present? Absent → the boot never reached our initramfs (firmware/kernel stage;
     look at the HDMI screen next time). Present but no "handing over" → read its FATAL line and dmesg tail.
   - p3 `boot-logs/<n>-<slot>-smoke/`: timeline (default runlevel reached? `nm=connected`? `health=200 ready=yes`?),
     services not started, `dmesg` lines for brcmfmac/firmware/ttyAMA/regdom, the reason in `nibe-reboot` lines.
   - the report's UART line: `disable-bt applied: yes (serial0=/soc/serial@7e201000, /dev/ttyAMA0)`; the initramfs
     line `device tree: serial0=… bluetooth=disabled v3d=…` (v3d `okay` means the firmware applied
     vc4-kms-v3d, i.e. overlays work from the slot now; `disabled` means only the pre-applied dtb saved the UART);
   - the timing line: default runlevel, "NetworkManager first connected" (uptime or never) and the time away.
5. Pass: initramfs trace complete, `disable-bt applied: yes`, default runlevel reached, NetworkManager connected
   (same IP, the gateway activated its profile), `/health` ready, no unexpected stopped services, back on Raspberry
   Pi OS by itself. Only then step 1 (rebuild the real bundle as `2026.10.25-alpine.5` with `NIBE_TAILSCALE_DIR`).

**1. Install into the inactive slot (over SSH)**
`time NIBE_OS_ZSTD_LEVEL=1 NIBE_SSH_HOST=nibe-admin@nibe-gateway.local bash -c 'source scripts/ssh-maintenance.sh &&
nibe_os_install target/alpine/nibe-os-alpine-2026.10.25-alpine.5.tar'` → os-install verifies the signature and
`rootfstype=squashfs`, writes p2 (the inactive slot), unpacks `slot-a/`, writes `tryboot.txt` and reboots once with
`0 tryboot`. Record the install time (compare 2 min 15 s for 378 MB).

**2. How a failed trial returns to Raspberry Pi OS (no one at the pump)** — tryboot is one-shot; every reboot that is
not a tryboot uses `config.txt`, i.e. the committed Raspberry Pi OS slot:
- kernel/initramfs cannot find or mount the root → initramfs exits → kernel panic → `panic=10` reboot (≈ 1 min);
- kernel panic later → `panic=10`; a hang after the watchdog daemon started (boot runlevel) → bcm2835 reset in 15 s;
- data partition not mounted → nibe-boot-guard reboots after 180 s;
- network not connected within 10 min while station profiles exist → the os-confirm network gate reboots;
- gateway/SSH/Tailscale unhealthy during the 600 s probation → os-confirm marks failed and reboots;
- anything else that leaves the trial uncommitted (e.g. OpenRC stuck before os-confirm) → nibe-boot-guard reboots
  30 min after boot;
- every one of these reboots goes through `nibe-reboot`: orderly, then a hardware-watchdog reset if shutdown hangs;
- in all cases read the traces afterwards with `scripts/alpine-trial-report.sh` (p1 initramfs stages, p3 boot-logs
  incl. `shutdown/`).
Not covered: a hang between the firmware and the watchdog daemon (sysinit) that does not panic — this is what
alpine.2 did; it needs a power cycle until the firmware watchdog (step 0.6) is verified. If the Pi is not back on
either slot within 40 min, power-cycle it once. After a fallback: `cat /proc/cmdline` shows the old slot,
`sudo nibe-updater os-status` → `failed`; run `scripts/alpine-trial-report.sh` and note the time it took.

**3. Checks on the Alpine slot before commit (within the 10 min probation; all read-only)**
- Base: `cat /etc/alpine-release`; `cat /proc/cmdline` → `rootfstype=squashfs panic=10 nibe.slot=<the slot that was
  inactive>` and the matching PARTUUID (`-02` for a, `-04` for b); `ls -d /run/systemd/system` fails; `grep -E ' / |squashfs|nibe|firmware' /proc/mounts` → `/` overlay,
  `/boot/firmware` vfat **ro**, p3 ext4 and four bind mounts; `ls -l /dev/disk/by-partuuid/`; `rc-status -a`
  (expected stopped only: sshd/tailscale if not provisioned); `cat /var/log/rc.log | grep -i error`; `free -m`;
  `uptime` (boot time); `dmesg | grep -iE 'brcmfmac|regdom|cfg80211|watchdog|ttyAMA'`; `ls -l /dev/ttyAMA0`
  (must exist, the PL011 on GPIO14/15 — if only ttyAMA1 exists, the overlay/alias assumption is wrong);
  `lsmod`; `ls /dev/watchdog` and `pgrep -a watchdog`.
- Wi-Fi station: `nmcli -t -f STATE general` → connected; `nmcli dev` → wlan0 connected to the station profile
  activated by the gateway; same IP as before; `iw`-less check of the regdom: `dmesg | grep regdom`.
- mDNS: from the laptop `ping nibe-gateway.local` and `avahi-browse -rt _http._tcp` (or `dns-sd -B _http._tcp`) →
  "NIBE Gateway on nibe-gateway", port 8080; no `_ssh._tcp` advertised. (Our musl binaries cannot resolve `.local`
  names themselves — expected.)
- SSH: login works key-only with the same host key fingerprint as step 0.4 (no host-key warning); `sudo -n true`;
  password login refused; `rc-service sshd status` started; `scp` works (internal-sftp).
- Tailscale: `tailscale version`, `tailscale status` shows the node with the same name/IP; reachable via its
  100.x address; `rc-service tailscale status` started.
- Gateway on the RS485 bus: `/health` ready, `device_degraded` false, `last_frame_age_ms` small; UI live values and
  `GET /api/v1/registers/explorer` diagnostics: no new NAKs/deadline misses; `ps -o pid,nice,args | grep nibe` →
  gateway nice 0, updater/system nice 10; `grep nibe-gateway /var/log/messages | tail`.
- History: new points are appended (`state/history`, UI chart continues across the switch).
- MQTT: broker connected in the UI and Home Assistant receives updates (use an IP or DNS name, not `.local`).
- Provisioning (no secrets): `sudo mount -o remount,rw /boot/firmware; echo '{}' | sudo tee
  /boot/firmware/nibe-provision.json; sudo mount -o remount,ro /boot/firmware`, then later after the commit a
  reboot → `grep nibe-provision /var/log/messages` "applied nothing", file gone, `/boot/firmware` ro.
- Setup AP (only on the bench Pi or at the end, it drops the station link): UI "forget Wi-Fi"/start AP →
  SSID `nibe-gateway-setup` visible, client gets 192.168.4.x from dnsmasq, UI reachable at 192.168.4.1:8080, then
  re-join the station. On the production Pi skip this until a bench Pi has passed it.

**4. Commit** — `sudo nibe-updater os-status` → `committed` after the probation (check `grep nibe-os-confirm
/var/log/messages`: "network connected" then the commit). Then on the committed Alpine slot:
- reboot once: comes back on the Alpine slot without tryboot; host keys, Tailscale identity, Wi-Fi unchanged.
- app OTA: `scripts/deploy-release.sh` (musl release) → `rc-service nibe-gateway restart` by nibe-updater, probation
  passes; `scripts/rollback-release.sh` works.
- HP-38a steps 2–5 (supervisor watchdog via SIGSTOP, SSH key add/remove and Tailscale enable/disable from the UI,
  follow the markers after reboot).
- OS rollback: install the last Raspberry Pi OS bundle (2026.10.25-5 or the squashfs one) with `nibe_os_install` from
  the Alpine slot → tryboots slot b (`0 tryboot` via reboot(2) RESTART2), commits; then the Alpine bundle again →
  commits. Both directions without touching the card; record the times.
Pass: 1, 3 and 4 hold; a forced failure (bench Pi only: switch the Wi-Fi access point off during the trial → the network gate
reboots after 10 min) returns to Raspberry Pi OS by itself. Record RAM (`free -m`, compare 66 MiB),
boot time and install time here.
Result 2026-10-25 (HP-38 first trial, bundle 2026.10.25-alpine.2, FAILED — safely rolled back): installed into slot B
in 42 s (51 MiB), tryboot at 19:49. The Pi never answered (no ping/SSH/Tailscale); owner power-cycled after ~11 min;
it came back on Raspberry Pi OS slot A, os-status `failed: tried slot did not boot`. No p3 file changed during the
trial, and no self-reboot happened (so no initramfs panic): the hang was before userspace — most likely the Alpine
6.18 kernel with the Pi firmware, a stage QEMU skips. Changes before the next attempt: Raspberry Pi OS kernel under
the Alpine userspace, boot markers on p1 and logs on p3, a smoke boot that reboots back by itself, no `quiet`.
Owner: call a trial failed after 3 minutes without an answer and power-cycle.
Result 2026-10-25 20:48 (HP-38 step S, smoke bundle 2026.10.25-alpine.4-smoke, BOOT PASS / gateway+Wi-Fi FAIL):
Raspberry Pi OS 6.12.109 kernel under the Alpine userspace boots on the Pi: initramfs handed over at 4.6 s, default
runlevel reached, NetworkManager + wpa_supplicant + brcmfmac 43430 up, smoke snapshots on p3 and p1, self-reboot after
150 s back to slot A (the owner's power cut at ~3 min came just as Raspberry Pi OS answered again — not needed).
Two faults: (1) disable-bt not applied on the Alpine slot → PL011 = ttyAMA1 with the Bluetooth serdev attached, no
/dev/ttyAMA0, gateway exits ("No such file or directory"; errors now name the port, bf656d6); the Alpine slot carried
4 overlay files vs 366 on Raspberry Pi OS, all compared files md5-identical. (2) Wi-Fi never connected: the station
profile has permissions=user:nibe-gateway and is activated by the gateway at startup, which had crashed.
Traces: ~/nibe-trials/20261025-205348 on the build machine.
Result 2026-10-25 21:16 (HP-38 step S, smoke bundle 2026.10.25-alpine.5-smoke, PASS): default runlevel 26.4 s,
NetworkManager connected 33.9 s, gateway 0.1.19 /health ready (device_degraded=false) by 41 s, disable-bt applied
(serial0 = PL011, /dev/ttyAMA0), SSH answered as Alpine at uptime 31 s; smoke reboot at 41.8 s, back on Raspberry
Pi OS without a power cycle (whole round < 2.5 min). Warnings to check in the trial: avahi chroot/resolv.conf.
Traces: ~/nibe-trials/20261025-211927.
Result 2026-10-25 21:21 (HP-38 real trial, bundle 2026.10.25-alpine.5, PASS — Alpine is the live base):
installed in ~45 s, Alpine answered at uptime 30 s, os-confirm committed slot B at uptime ~72 s. Verified after
commit: kernel 6.12.109+rpt-rpi-v6 on squashfs + overlay; RAM used 35 MiB (Raspberry Pi OS 66 MiB); SSH host key
unchanged (SHA256:LUZ0lZXS…); Tailscale up at 100.98.91.96; /dev/ttyAMA0; pump bus 68 reads / 0 timeouts, 46 live
registers, alarm 0; history writing (239 ms flush); Wi-Fi connected (Rikard-IoT); app OTA 0.1.20 activated through
OpenRC in ~70 s. Raspberry Pi OS (bundle -6) stays in slot A as the fallback.
Open (next Alpine bundle): avahi announced nibe-gateway-2.local (name conflict, limit to wlan0); OpenRC polkit
service "failed" while D-Bus-activated polkitd runs; nibe-os-confirm shows "crashed" after a clean commit (oneshot);
a gateway restart re-activates the station profile (brief Wi-Fi reconnect).

**Next OS update: 2026.10.25-alpine.6** (51.8 MiB, pinned Tailscale), installed like alpine.5
from the committed Alpine slot. Extra checks after it commits:
- `ps | grep avahi-daemon` → "running [nibe-gateway.local]" (not `-2`); from the laptop `ping nibe-gateway.local`;
  `grep avahi /var/log/messages` shows only wlan0 joining (no lo, no tailscale0); the chroot/resolv.conf warnings
  remain and are harmless.
- `rc-status -a` → no `failed`/`crashed` entries except intentionally stopped `sshd`/`tailscale` when not
  provisioned; polkit is not listed; `pgrep -a polkitd` shows one D-Bus-activated polkitd after the gateway has
  talked to NetworkManager; `grep -c 'respawned.*polkitd' /var/log/messages` → 0.
- `rc-service nibe-os-confirm status` → started; `grep nibe-os-confirm /var/log/messages` → the `committed` line.

Result 2026-10-25 21:39 (alpine.6 into slot A, PASS): committed 85 s after the reboot; both slots are now Alpine
squashfs (Raspberry Pi OS retired from the card; bundle -6 kept on the build machine). avahi runs as
nibe-gateway.local and resolves to the Pi's LAN address; rc-status clean (all started, polkit only via D-Bus
activation, one polkitd, no respawns); RAM used 35 MiB; pump 63 reads / 0 timeouts. Gateway 0.1.21 restart kept
Wi-Fi up (no NetworkManager re-activation).

## HP-39 — Gateway clock catches up after boot-time NTP (0.1.22, 2026-10-25)
**Status 2026-10-25: VERIFIED** (result below).
SystemClock is monotonic but jumps forward once when the wall clock is > 60 s ahead (the NTP step after a boot that
started with an old time). After the next reboot or OS update: `/api/v1/device` `now_ms` within a few seconds of
real time once chrony has synced; register explorer timestamps current; history `samples_dropped_clock` stays 0;
poll timeouts at most +1 around the step.

Result 2026-10-25 22:21 (HP-39, PASS, on the alpine.7 OS update reboot): swclock restored a time 646 s behind,
chronyd stepped the system clock by 646.09 s at 21:21:39Z, and the gateway clock followed: `now_ms` within 0.24–0.75 s
of real time afterwards; readings briefly stale right after the step, fresh within a minute; history dropped 29
pre-sync samples (correctly, older than stored data) and kept writing. alpine.7 committed into slot B in 95 s;
nibe-wifi-fallback service installed and started (live fallback test is HP-40 step 6, bench only).

## HP-40 — Fresh Alpine card and the Wi-Fi fallback (added 2026-10-25)
**Status 2026-10-25: STILL OPEN (needs a bench Pi or a spare card)**: steps 1–5 (fresh card) and step 6 (Wi-Fi fallback trigger). Deferred by the owner until a spare Pi/card is at hand.

Verified without the Pi: `scripts/inspect-alpine-image.sh` (layout = the owner's card, random disk id, FAT hidden
sectors 16384 like the card, pinned firmware with the card's start.elf, config.txt selecting slot-a/, slot A cmdline
with the image's PARTUUID, empty slot-b/ and p4, seeded p3) and `scripts/qemu-boot-alpine-image.sh` (the image's own
slot-a kernel/initramfs boot it as an SD card: PARTUUID root, label mounts, nibe-persist seeding, default runlevel,
boot trace, no p1 writes, the fallback finding no profile, PL011 = ttyAMA0). Not verifiable in QEMU: the Pi
firmware reading this p1 (config.txt, os_prefix, overlays), Wi-Fi, the setup AP, the UART.
Use a **spare** Pi Zero W and card (never the production card, which is not reflashed).
1. Build and verify: `scripts/build-alpine-image.sh target/alpine/nibe-os-alpine-VERSION.tar`,
   `(cd target/alpine && sha256sum -c nibe-gateway-alpine-VERSION.img.xz.sha256)`,
   `scripts/inspect-alpine-image.sh target/alpine/nibe-gateway-alpine-VERSION.img.xz`.
2. Flash: `xzcat target/alpine/nibe-gateway-alpine-VERSION.img.xz | sudo dd of=/dev/sdX bs=4M conv=fsync status=progress`.
3. Unprovisioned first boot (HDMI if available): within ~2 min the **open** setup AP `NIBE-Gateway-XXXX` appears
   (amended 2026-10-31; `nibe-gateway-setup` is only the NM connection id); join it without a password,
   the web setup at 192.168.4.1:8080 asks for no device token, creates the admin and joins the bench Wi-Fi.
4. Provisioned first boot (re-flash): copy `nibe-provision.json.template` → `nibe-provision.json` on bootfs with
   Wi-Fi, admin password, an SSH key (no Tailscale key on the bench unless intended). Boot → on the LAN as
   `nibe-gateway.local`, `ssh nibe-admin@…` works; `ls /boot/firmware` has no `nibe-provision.json`;
   `cat /proc/cmdline` shows `root=PARTUUID=<the image's disk id>-02 nibe.slot=a`; `rc-status -a` clean;
   `sudo nibe-updater os-status` idle; `scripts/pi-diagnostics.sh debug`.
5. First OS update into slot B: `nibe_os_install` of the same or a newer bundle → tryboots slot B, commits.
6. Wi-Fi fallback (bench Pi only), live, without touching p3: over SSH run (detached, because the SSH link drops)
   `sudo setsid sh -c 'rc-service nibe-gateway stop; rc-service nibe-wifi-fallback restart; sleep 5;
   nmcli con down id "$(nmcli -t -f NAME con show --active | grep ^nibe-gateway-station- | head -n 1)"' &`.
   Expected: the Pi drops off the LAN, and ~2–3 min later `nibe-wifi-fallback` activates the station profile as
   root and it is reachable again: `grep nibe-wifi-fallback /var/log/messages` shows "activated
   nibe-gateway-station-…", `scripts/alpine-trial-report.sh` lists `wifi-fallback-activated` under events. Then
   `sudo rc-service nibe-gateway start` (or reboot). A normal boot records `wifi-fallback-not-needed`.
Pass: 2–6 hold. Record the flash time and first-boot time.

## HP-41 — Apply the owner's register notes (added 2026-10-26)
- [ ] After deploying a release with `PUT /api/v1/registers/notes`: on the dev machine run
  `python3 scripts/register-notes-f730-owner.py --out /tmp/register-notes.json` (576 notes: relevant 169, absent 396,
  suspicious 6, unmaintained 5; 42 display-verified). Log in with the admin password (curl cookie jar), then
  `curl -fsS -b /tmp/nibe.cookies -X PUT -H 'Content-Type: application/json' -H 'X-Requested-With: nibe-gateway'
  --data-binary @/tmp/register-notes.json http://<pi>:8080/api/v1/registers/notes | jq '.register_notes | length'`
  (exact commands in the script header) · Expected: 576; the Register explorer opens on "Relevant on this pump";
  43108 shows "active fan speed step" and "Checked against the display"; 41256 shows "Unmaintained";
  `/var/lib/nibe-gateway/state/register-notes.json` is 0600 and survives a restart; an API key gets 403 on the PUT ·
  Safety: no pump traffic; notes never change write permission.
Result 2026-10-26 18:44 (HP-41, applied): 0.1.23 deployed; seed written to /var/lib/nibe-gateway/state/
register-notes.json (nibe-gateway, 0600) over SSH and loaded after a gateway restart: 576 notes (169 relevant, 396
absent, 6 suspicious, 5 unmaintained), 42 display-verified; explorer returns them (43108 "active fan speed step",
41256 unmaintained). Owner to glance at the Register explorer filter and badges in the UI.

## HP-42 — Heating-offset control lease on the F730 (added 2026-10-29)
- [ ] Needs a release with the lease (first one after 0.1.24) deployed, the owner watching the pump display or
  myUplink, and 47011 still allowlisted (`allow_hardware_writes`, `writable_registers` contains 47011). Verified so
  far only on the simulator (contract and unit tests, curl run on 2026-10-29).
  Setup: in Access, create a **control** API key named `lease-test`; `KEY=nibe_…`, `P=http://<pi>:8080`,
  `H=(-H "Authorization: Bearer $KEY" -H 'Content-Type: application/json')`.
  1. **Baseline.** `curl -s "${H[@]}" -X POST -d '{"register_ids":[47011]}' $P/api/v1/discovery/probe`, wait ~5 s,
     then `curl -s "${H[@]}" $P/api/v1/measurements | jq '.[]|select(.id==47011)|{value,stale,age_ms}'`. Note the
     value as B (owner's normal; it was −2 on 2026-10-24) and that the display shows the same. `stale` must be false.
     Test value T = −2, or −3 if B is −2, so the change is visible.
  2. **Leased write.** `curl -s "${H[@]}" -X POST -d '{"offset":T,"lease_seconds":60}' $P/api/v1/controls/heating-offset | jq .`
     Expected: 200, `detail` ends with `lease 60 s, offset B is restored unless renewed`.
     `curl -s "${H[@]}" $P/api/v1/status | jq .heating_offset_lease` shows holder `lease-test`, offset T, baseline B,
     `remaining_s` ≤ 60. `curl -s "${H[@]}" $P/api/v1/discovery/probe/status | jq .writes.last` shows 47011=T accepted;
     the display shows T within a few seconds.
  3. **Lapse.** Do not renew. After 60–62 s: `heating_offset_lease.heating_offset` is null, `last_restore` has
     `ok: true`, baseline B, holder `lease-test`; `writes.last` shows 47011=B accepted; a probe of 47011 (step 1)
     reads B; **the display shows B**. `grep -i lease /var/log/messages` has "heating offset lease granted" and
     "lease lapsed; baseline restored". Record the seconds between expiry and the display change.
  4. **Renewal (short).** Write T with `lease_seconds` 60, write T again with 60 after ~40 s, and confirm no restore
     at the 60 s mark (`last_restore.at_ms` unchanged, display still T); then let it lapse as in step 3.
  5. **Restart with an expired lease.** Write T with `lease_seconds` 60. Over SSH at once:
     `sudo cat /var/lib/nibe-gateway/state/control-lease.json` (holder, baseline B, `expires_at_ms`), then
     `sudo rc-service nibe-gateway stop`. Wait 120 s; the display still shows T (nobody restores while stopped).
     `sudo rc-service nibe-gateway start`. Expected: within about a minute of the wire coming up (a restore that
     fails while the wire is starting is retried every 30 s) `last_restore.ok` is true with baseline B, the
     control-lease file has no `heating_offset`, 47011 reads B and the display shows B. Also check that the
     reconnect's alarm 251 is auto-reset as usual.
  6. **Unleased write ends the lease.** Write T with `lease_seconds` 60, then set B from the admin Controls page
     (no lease). The Controls page shows "No lease"; no restore happens at the 60 s mark.
  Safety: offsets stay inside −10..10 on an allowlisted named control; each write is sent once (a restore that fails
  transiently is re-sent every 30 s, which is harmless for an absolute value). To abort at any point, set B from the
  Controls page (this ends any lease). Delete the `lease-test` key afterwards.

  - **Status 2026-10-29: steps 1–6 VERIFIED on the owner's F730** (0.1.25, control API key, from the LAN; read-back
    via `/api/v1/measurements`, the owner did not watch the display). B = −2, T = −3.
    1. Baseline 47011 = −2, not stale. 2. Leased −3/60 s at 17:34:39: accepted, read-back −3 within 8 s; lease shows
    baseline −2. 3. Expiry 17:35:39: `last_restore.ok` with "baseline confirmed by post-write read-back" at +3.8 s;
    reads −2. 4. Write 17:38:27 + renewal 17:39:07: no restore at 17:39:27, restore after 17:40:07, reads −2.
    5. Leased −3 at 17:41:29, file showed holder/baseline −2/expiry; `rc-service nibe-gateway stop` 17:41:42, start
    17:43:43 (lease expired meanwhile): "lease loaded from state" then "lapsed; baseline restored" at +5 s, file
    `heating_offset: null`, reads −2; reconnect alarm 251 at 17:44:35 auto-reset in 4 s. 6. Leased −3, unleased −2
    after 16 s: detail "lease … ended without a restore", no restore at the 60 s mark, reads −2.
    Not done here: display observation, Busy/queue and wire-interruption cases, power cuts (spare card).
    MQTT owner-write cases: not applicable to this install (no Home Assistant/MQTT; owner 2026-10-29); the
    unleased-owner path is the admin Controls page or HTTP, covered by step 6.

### HP-42 amendment — 2026-10-29 implementation review
Still pending hardware; simulator evidence does not establish pump application or SD power-loss durability.
- During expiry, monitor `/api/v1/discovery/probe/status` and `/api/v1/measurements`: queued/sent restore must
  retain `heating_offset`; only accepted 47011=B followed by a reading with `observed_at_ms` strictly greater
  than outcome `updated_at_ms` may produce `last_restore.ok=true`. Check the display shows B.
- With owner supervision, interrupt the wire during restore, reconnect, and confirm bounded retries and eventual
  post-completion read-back. Send an unleased HTTP or MQTT owner offset while pending: it must cancel restoration.
- Immediately renew after the MQTT owner write: expect 503 until its post-completion reading, then a new lease
  with the owner's value as baseline. Also renew during pending restoration: original baseline must remain B.
- Bench/spare-card power-cut tests: cut after durable leased intent but before dispatch, and after durable owner
  cancellation but before dispatch. Restart must retain baseline protection in the first case and never restore
  the superseded lease in the second. Restore the owner's normal value after each supervised test.
- The receipt detail now reports `lease 60 s`; confirmation must be checked via outcome and reading, not detail.

### HP-42 addendum — 2026-10-29 rejected writes and independent completion
Verified locally: definite Busy/Validation/ControlsDisabled dispatch rejection retains the previous durable
lease and does not create a freshness watermark. Offset completion survives Temporary Lux and automatic alarm
reset; a new offset attempt invalidates its preceding completion. These are test results, not pump verification.
On the supervised F730 with the existing control gates: lease −10 from the owner's recorded baseline; occupy the
write queue with Temporary Lux, then try an unleased HTTP offset and separately an MQTT offset. Confirm Busy,
unchanged lease status and persisted baseline; stop renewals and confirm baseline restoration after expiry.
Repeat with no lease: Busy owner attempt, then a leased attempt after the queue drains and a fresh poll; it must
succeed without another unleased write or restart. After offset restore completion, issue Temporary Lux (and
observe an automatic 251 reset only if naturally present); confirm the subsequent offset reading completes
restoration without repeated baseline writes. Also power-cycle after a rejected owner request and check the
lease survives. Physical outcomes remain unverified; do not induce alarms solely for this test.

### HP-42 addendum — 2026-10-29: owner supersession crash boundaries
Verified locally only: persisted pending supersession survives pre-dispatch/crash images; startup requires a
new offset reading, matching the owner request cancels, otherwise normal expiry restores the retained baseline.
Completion-before-caller-resume regression is covered in simulation. On the supervised Pi/F730 with existing
the control gates: lease −10 from a known baseline; interrupt before owner dispatch and after a definite Busy
before rollback persistence; restart, confirm lease retained after fresh −10 read-back and baseline restored
at expiry. Repeat with owner offset 2 accepted before cancellation persistence; restart and confirm fresh 2
ends the lease and expiry leaves 2 unchanged. Verify no restore dispatch before the fresh startup reading.
Physical timing and power interruption remain unverified; no hardware writes performed here.

- [x] **HP-43** GitHub Releases update source · Needs the public repo with a published release newer
  than the Pi's version, and the Pi online · Steps: (1) on the Updates page press **Check for updates**; expect the
  latest version, its date and size, and "x.y.z available". (2) **Download and stage**; expect phase `staged` within a
  few minutes, gateway RSS stays well under the 512 MB budget during the download (`top` over SSH). (3) Activate;
  expect probation → `complete` as with a manual upload. (4) Check again: "Up to date", and the stage button disabled.
  (5) Optional: publish a release whose tar was signed with a different key and check that staging fails with
  "update rejected" and the active slot is unchanged · Expected: same outcome as a manual upload; no traffic to GitHub
  without a button press · Safety: the updater's signature check is unchanged; no pump writes involved.
  (2026-10-31 note: the owner's LAN addresses were replaced by `nibe-gateway.local` or generic wording in this file
  before the repository went public.)
  **Verified 2026-10-31 (steps 1–4):** the Pi on 0.1.27 found v0.1.28 under Online updates, downloaded and staged it,
  activated, probation `complete` on 0.1.28 with the pump link healthy. Step 5 (foreign key) not run. Owner feedback:
  no progress indication while downloading → progress bar added in 0.1.30.


- [ ] **HP-44** Open register writes on the live bus · The endpoint is tested in unit and
  contract tests only; the pump's answer to writes of registers never written before is unverified · Prerequisites:
  a release with open writes, the production config (`--device hardware --allow-hardware-reads`, open writes on by
  default), the owner watching the display. Steps: 1) log in, read the current value of 47049 (HW stop temp economy)
  with `POST /api/v1/discovery/probe` `{"register_ids":[47049]}` and note it; 2) `POST /api/v1/controls/register-write`
  `{"register":47049,"value":<current + 1.0>}` (header `X-Requested-With: nibe-gateway`, or a control-scoped API key);
  3) poll `/api/v1/registers/explorer` until `probe.writes.last.state` is `accepted`, `denied` or `timeout` and check
  the read-back of 47049 and menu 5.1.1 on the display; 4) write the original value back the same way; 5) repeat once
  with `{"register":47049,"value":<original>}` while another write is pending to confirm `429` busy · Expected:
  `accepted`, read-back and display both show the new value, then the original; `naks_received` and
  `ack_deadline_misses` unchanged · Safety: owner present; one register; restore the original value at once. Never
  test 45171, 48852 or other protocol/alarm registers this way.
- 2026-10-31 HP-44 addendum: also verify reads and MQTT. 6) `POST /api/v1/controls/register-read`
  `{"register":47049}`: expect the current value within a few seconds, matching menu 5.1.1. 7) With MQTT enabled
  and `allow_commands` on, publish to `nibe/<device>/register/47049/get` (empty payload): expect the value on
  `nibe/<device>/register/47049` and `{"ok":true}` on `nibe/<device>/result`. 8) Publish `<current + 1>` to
  `.../register/47049/set`, confirm on the display and the read-back, then restore the original value the same
  way. 9) Read a register the pump does not have (e.g. `{"register":1}`): expect 504 after ~20 s and no new alarm.
- 2026-10-31 HP-44 addendum 2 (poll choices): 10) On the Register explorer, set 47049 to Slow: expect it in
  Readings within ~10 min, in `GET /api/v1/registers/polls` under `slow` and `overrides`, and (MQTT discovery on)
  as a new Home Assistant entity without restarting HA. 11) Restart the gateway and confirm the choice is still
  applied; after the next OS/app update confirm again. 12) Set it back to Off: the override disappears.


- [ ] **HP-45** Setpoint leases on the live bus · Unit- and contract-tested only (fake bus and
  replay); the F730's answers for 47045/47049/47375 and the restore timing are unverified. Hypotheses: the pump
  accepts writes inside the catalog range and reads the written value back at once; the raw read of each is an
  integer that the lease can restore verbatim · Prerequisites: a release with this change, production config (open
  writes on), owner at the display, HP-44 steps 1–4 done for 47049. Steps: 1) note 47049 on the display (menu
  5.1.1) and in `POST /api/v1/controls/register-read` `{"register":47049}`; 2) `POST /api/v1/controls/register-write`
  `{"register":47049,"value":<current − 2.0>,"lease_seconds":120}`: expect `lease 120 s, baseline <raw>` in the
  detail and a 47049 entry in `register_leases.leases` in `GET /api/v1/status`; 3) after ~60 s renew with
  `<current − 3.0>` and `lease_seconds` 120: the baseline must not change; 4) wait ~2.5 min without writing: expect the
  original value on the display, the 47049 entry in `register_leases.restores` with `ok: true`, a `lease` event on
  the live stream and no 47049 entry in `leases`; 5) start a lease again, then write 47049 without a lease (the original value): expect
  "ended without a restore" and no restore after the lease time; 6) MQTT (`allow_commands` on): publish
  `{"value": <current − 2>, "lease_seconds": 120}` to `nibe/<device>/register/47049/set`, check `result`, let it lapse
  and confirm the restore; 7) restart the gateway (`rc-service nibe-gateway restart`) during an active 120 s lease:
  the lease must be listed again after start and restore on time; 8) repeat steps 2 and 4 once for 47045 and 47375
  (heating stop temperature, find it in the display menu first), the latter outside the heating season only or with the owner's consent ·
  Expected: every restore confirmed by read-back, `naks_received` and `ack_deadline_misses` unchanged, at most one
  restore write per lapse unless a write timed out · Safety: owner present; one register at a time; short leases;
  the original value is always written back by hand if a restore reports `ok: false`. 43005 (degree minutes) is not
  leasable by design.
  9) Regression for 47011 (the setpoint leases share the coordinator with the offset lease since 2026-10-31): repeat
  HP-42 steps 1–6 once on this release; the saved lease file keeps its earlier `heating_offset` fields, so also
  confirm that an active offset lease survives an update to this release.

- [ ] **HP-46** Audit log on the appliance · Verified in simulation, with a fake pump on a PTY
  (HTTP write → pump accepted → read-back) and with a local MQTT broker; not yet on the Pi or the live bus.
  Hypotheses: the F730's answer and read-back appear within ~10 s of a write; `audit.jsonl` on the data partition
  survives app and OS updates; the 0.5 s follower costs no measurable CPU on the Pi Zero · Prerequisites: a release
  with this change · Steps: 1) open the new **Audit log** page: expect earlier entries empty (the log starts with
  this release) and the Active leases panel listing any active lease; 2) change something harmless (e.g. save the
  MQTT settings unchanged): expect an "MQTT settings changed" entry by `admin` with the password shown as
  `(hidden)`; 3) during the next owner-approved register write (e.g. HP-45 step 2): expect an HTTP "Register write"
  entry and, within ~10 s, a "Register write on the pump" entry with `accepted; read back raw <value>`; the lease
  appears in Active leases with holder, leased value, restore value and time left; 4) let that lease lapse: expect
  a "Lease lapsed; earlier value restored" entry by `gateway` and a pump entry for the restore write; 5) if alarm
  251 occurs with auto-reset on, expect a pump entry for 45171 with no matching request; 6) after the next update,
  confirm the entries are still listed and `ls -l /var/lib/nibe-gateway/audit*.jsonl` shows mode 0600 and a size
  below ~512 KiB in total; 7) `top` for a minute: the gateway's CPU use is unchanged compared with before the
  release · Expected: no secrets in the page or the file (`grep -i -e password -e nibe_ /var/lib/nibe-gateway/audit.jsonl`
  shows only `"(hidden)"` values) · Safety: read-only apart from the writes already planned in HP-45.
## HP-47 — Fresh card whose first boot completes the layout (added 2026-10-31)
- [ ] Image `nibe-gateway-alpine-0.1.31-firstboot.1.img.xz` (OS bundle signed with the real release key, so
  the card accepts official releases afterwards). Verified only under QEMU raspi0 (`scripts/qemu-boot-alpine-image.sh`:
  short image on an 8 GiB virtual card with an old ext4 where p3 lands, two boots).
  Hypotheses to confirm on the Pi: the kernel re-reads the new table on the Pi's SD controller while the card is the
  boot device (if not, the init reboots once and resumes; the trace says "did not re-read it; rebooting"); the
  firmware boots from a p2 that is shorter than the layout; mke2fs and the seed take well under a minute; writing
  the 563 MiB image takes about 2–3 minutes on this card. Card: the owner's spare test card (it held Raspberry Pi
  OS before), never the production card. No `nibe-provision.json`: this run tests the setup AP.
  1. Laptop: `tailscale file get ~/Downloads/` then
     `cd ~/Downloads && sha256sum -c nibe-gateway-alpine-0.1.31-firstboot.1.img.xz.sha256`.
  2. Insert the card, find it with `lsblk -o NAME,SIZE,MODEL,MOUNTPOINTS` (sdX below), unmount any mounted partition
     (`sudo umount /dev/sdX?*`), then
     `time (xzcat nibe-gateway-alpine-0.1.31-firstboot.1.img.xz | sudo dd of=/dev/sdX bs=4M conv=fsync status=progress)`.
     Record the time (was ~25 min for the 4.9 GiB image).
  3. Before the first boot: `sudo sfdisk -d /dev/sdX` shows two partitions (start 16384 size 1048576 type c; start
     1064960 size 88064 type 83); after re-plugging, `bootfs` holds `nibe-first-boot` and no `nibe-provision.json`.
  4. First boot in the Pi (F730 bus not needed). Record the seconds from power-on until the Wi-Fi network
     `nibe-gateway-setup` appears. Expected: within ~3 min. Join it, open http://192.168.4.1:8080, complete the setup
     and join the home Wi-Fi (this also covers HP-40 step 3 and the start of HP-13).
  5. Power off, card into the laptop: `sudo sfdisk -d /dev/sdX` shows the full layout (starts 16384, 1064960,
     5111808, 6160384; sizes 1048576, 4046848, 1048576, 4046848; same label-id as in step 3); `lsblk -f /dev/sdX`
     shows ext4 `nibe-data` on sdX3; `sudo debugfs -R 'ls -l /' /dev/sdX3` lists `current`, `releases`, `state`,
     `system`, `update-state`; `bootfs` has no `nibe-first-boot` and has `nibe-trial/a-1.log` with
     "first boot: layout short, marker 1, plan layout", "partition table complete" and "complete, nibe-first-boot
     removed". Keep a copy of that log.
  6. Second boot: card back in the Pi. Expected: the gateway joins the home Wi-Fi saved in step 4 (p3 kept), no
     setup AP, `http://nibe-gateway.local:8080/` answers. Power off and check on the laptop that `sfdisk -d` is
     unchanged and `bootfs/nibe-trial/` still holds only `a-1.log` (normal boots do not write p1).
  7. Later (optional, with SSH set up through the Access page): an online update to the next release installs into
     slot B (sdX4) as in HP-40 step 5.
  Failure: if no setup AP appears within ~10 min, power off, read `bootfs/nibe-trial/*.log` on the laptop and send
  it; then write the card again (step 2). A card whose first boot failed is never repaired in place.
## 2026-10-31 — Open setup AP and tokenless setup over it (affects HP-47 step 4, HP-40 step 3, HP-13)
- [ ] Image `nibe-gateway-alpine-0.1.31-firstboot.2.img.xz` = `firstboot.1` (the first-boot layout work, HP-47)
  plus. Corrections to the earlier texts, **verified by reading the code, not on hardware**:
  - The SSID is `NIBE-Gateway-XXXX` (XXXX = last four hex digits of the Pi's serial number, upper case), **not**
    `nibe-gateway-setup`; that is only NetworkManager's connection id. HP-47 step 4 and the earlier HP-40 step 3
    text named the wrong network.
  - With `firstboot.1` the AP was WPA2 with a random 32-hex password stored only in root-only
    `state/setup-secrets.json` on p3, and the admin form needed the setup token from the same file: step 4 could
    not be completed. From `firstboot.2` the AP is open and, while no admin exists, a client on it needs no token.
  Hypotheses to confirm on the Pi: NetworkManager starts an AP without a security section on the Zero W's
  brcmfmac driver; the phone sees the socket peer as 192.168.4.x (NM shared mode, dnsmasq).
  HP-47 with `firstboot.2` (steps 1–3, 5–7 as written there, with the new file name and the step 3 values below):
  3. `sudo sfdisk -d /dev/sdX`: label-id `0x1e3ebec0`; start 16384 size 1048576 type c; start 1064960 size 88064
     type 83 (image sha256 `fdf42affb0447c9bdaabd9eff01a2f123740c62d6f6b740c63ead436e58b9bfe`; QEMU 18/18 PASS).
  4. First boot. Record the seconds from power-on until the Wi-Fi network `NIBE-Gateway-XXXX` appears (open, no
     lock icon). Join it **without a password**, open http://192.168.4.1:8080/. Expected: "Set up your gateway"
     with **no** "Device setup token" field; `curl -s http://192.168.4.1:8080/api/v1/setup/status` from the
     phone/laptop on the AP shows `"setup_token_required":false` and `"setup_ssid":"NIBE-Gateway-XXXX"`. Create
     the admin (12+ characters) right away, then Wi-Fi setup → join the home Wi-Fi.
  4a. After the admin exists, a second client on the AP (if it is still up) gets 401 from
     `curl -s -o /dev/null -w '%{http_code}' http://192.168.4.1:8080/api/v1/network/scan`.
  4b. Optional (re-flash): a `nibe-provision.json` with only `wifi` (no `admin_password`) puts an unclaimed Pi on
     the home LAN. From a LAN client `GET /api/v1/setup/status` reports `setup_token_required: true` and `POST
     /api/v1/setup/admin` without `X-Setup-Token` returns 401 (send `X-Requested-With: nibe-gateway`).
  Failure: no AP, or a WPA2 AP → `bootfs/nibe-trial/*.log` and, with SSH, `grep nibe-gateway /var/log/messages`
  and `nmcli -f 802-11-wireless-security.key-mgmt con show nibe-gateway-setup`.
## 2026-10-31 — HP-47 with `firstboot.3`: polkit allows the open AP; a refused AP no longer stops the gateway
- [ ] `firstboot.2` on the Pi (verified from its p3 `boot-logs/0001-a-normal/messages.txt`): steps 1–3 and
  the first-boot layout passed (table rewritten at 4.7 s, p3 seeded at 11.3 s, no re-read reboot; table after boot
  16384/1064960/5111808/6160384, label-id unchanged), but step 4 failed: NetworkManager refused every AP activation
  from uid 995 ("Not authorized to share connections via wifi.", polkit `wifi.share.open` was denied), the gateway
  exited and supervise-daemon stopped after 6 restarts, so neither AP nor HTTP came up. `firstboot.3` = `firstboot.2`
  + the polkit rule allowing `wifi.share.open` + the gateway staying up (`offline`, `last_error`) and retrying a
  refused AP every 30 s doubling to 5 min. Image `nibe-gateway-alpine-0.1.31-firstboot.3.img.xz`, sha256
  `ea66ef60d47b62280cfb05d699147d1937f5f848301bc21f19c5669ea2a7e94c`, label-id `0xdab89ba6`; QEMU 18/18 PASS
  (simulated network: polkit is still not exercised there).
  Rerun HP-47 from step 2 with the `firstboot.3` file name; step 3 expects label-id `0xdab89ba6` (partition
  starts/sizes as before); step 4 as in the `firstboot.2` entry above (open `NIBE-Gateway-XXXX`, no token field).
  Failure: no AP within ~5 min → with the card on the laptop, `grep -E 'nibe-gateway|NetworkManager.*(share|polkit|Not authorized)'
  p3/system/boot-logs/*/messages.txt`; the gateway should now still answer on its other addresses and log
  "NetworkManager refused AddAndActivateConnection2" with the reason, and retry instead of exiting.
- Note for HP-47: a `firstboot.2` or `firstboot.3` card updates to 0.1.33 or later, never to 0.1.32. Its `setup-secrets.json` has no `ap_password`, which 0.1.32 requires.

## HP-48 — Holiday-mode lease on the F730 (added 2026-11-01)
- [ ] **HP-48** Lease on 48043 that always restores 0 · Unit-tested with a fake channel only. Reported by the controller's
  author, not checked by us (2026-11-01): 0 = off and 10 = on, a write reads back, hot water does not start while it
  is on and starts within 15 s after it goes off. Hypotheses: a restore of 0 confirms by read-back like the setpoints
  in HP-45; the pump does not change 48043 by itself. If it does, for example through the dates in menu 4.7, the
  gateway re-sends the restore every 30 s until it reads 0 · Prerequisites: a release with this change, production
  config (open writes on), owner at the display, HP-45 steps 2–4 passed for 47049. Steps: 1) note 48043 and the
  display's holiday setting (menu 4.7); 2) with holiday off, `POST /api/v1/controls/register-write`
  `{"register":48043,"raw":10,"lease_seconds":120}`: expect a 48043 entry in `register_leases.leases` in
  `GET /api/v1/status` with `baseline` 0, and holiday on at the display; 3) let it lapse (~2.5 min): expect 48043 = 0,
  a restore with `ok: true` and holiday off at the display; 4) set holiday on **at the display**, then write
  `{"register":48043,"raw":0,"lease_seconds":120}` and let it lapse: expect `baseline` 0 and holiday still off; 5) set
  holiday on at the display, write `{"register":48043,"raw":10,"lease_seconds":120}` and let it lapse: expect holiday
  off; 6) watch 48043 for an hour with holiday on by a plain write, and through the end of a date range in menu 4.7
  if one can be set, to see whether the pump changes it by itself · Expected: at most one restore write per lapse
  unless a write timed out; `naks_received` and `ack_deadline_misses` unchanged · Safety: owner present; short
  leases; turn holiday off at the display if a restore reports `ok: false`.
