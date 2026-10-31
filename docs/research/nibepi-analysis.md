# NibePi — what the existing installation actually does

Source: https://github.com/anerdins/nibepi @ `1aa36b02986d88778d765be738c68fb2bcabdbea` (MIT), plus older
https://github.com/bebben88/NibePi @ `00e584cd` for Node-RED flows and update scripts. Read 2026-10-22.
Everything here is **VERIFIED-SOURCE** (read in code) unless marked HYPOTHESIS; the owner's exact version may differ.

## 1. Process structure
* `index.js` (library, loaded by Node-RED nodes `nibe-config`, `nibe-input`, `nibe-output`, `nibe-request`, …)
  forks `backend.js` (serial core) via `lib/startCore.js`. Communication over Node IPC (`process.send`).
* `backend.js` owns `/dev/ttyAMA0` at 9600 (backend.js:54) and implements the ACK/token logic (`makeResponse`).
* Alternative cores: `nibegw-client.js` (UDP to an external NibeGW) and `modbus-client.js` (S-series TCP).
* Node-RED flows (NPold `node-red/flows_1.1B.json`) implement automations: weather-forecast curve adjust,
  indoor control, Tibber price control, hot-water automation, RMU emulation, fan control.
* Read-only root FS; `config.json` and graphs are saved by remounting rw (index.js `updateConfig`, `saveGraph`).

## 2. Bus behaviour (backend.js)
| Behaviour | Where |
|---|---|
| Only frames with byte 2 ∈ {0x19,0x1A,0x1B,0x1C,0x20} are processed, others dropped. | backend.js:138 |
| Frame length `LEN+6`; checksum XOR from byte 2; accepts `C5` for calc `5C`; NAK `15` on mismatch. | :143, :215-226, :199 |
| Strips a leading `06` (pump ACK) glued to the next frame. | :117-121 |
| Dedups `5C 5C` and recomputes LEN/checksum before handing data to index.js. | :168-185 |
| Read token (`69`,LEN 0): send queued on-demand read, else next register of the round-robin list, else ACK. | :306-336 |
| Write token (`6B`,LEN 0): pop one queued write and send it, else ACK. | :338-353 |
| Any other frame for an accepted address: ACK. | :354-365 |
| RMU addresses: detects whether a real RMU40 answered the first frame (next byte `06` or `C0`); if not, **emulates RMU40** (answers 0x60/0x62/0x63/0xEE). | :145-161, :231-305 |

## 3. Writes NibePi performs on its own (important for migration)
| Write | When | Where |
|---|---|---|
| **45171 = 1** (alarm reset) | on every core start (initial `sendQueue`) and again after the first frame (`startReset`) | backend.js:37, :133-136 |
| **48852 = 0** (MODBUS40 word swap off) | on every core start (initial `sendQueue`) | backend.js:37; also nibegw-client.js:6 |
| **45171 = 1** | whenever product info arrives and 45001 (alarm) reads **251** "Kom.avb Modbus" | index.js:416-425 |
| Curve offset / setpoints / fan / hot-water registers | Node-RED automations if enabled by the user (e.g. `nibe-weather`, `nibe-indoor`, `nibe-price`, `nibe-hotwater`, `nibe-rmu` nodes) | NPold flows_1.1B.json node types |
| Any register via MQTT `<topic>/<reg>/set` | if MQTT enabled | index.js `handleMQTT` |

Consequences:
* The owner's pump almost certainly has **48852 = 0** → 32-bit read responses are word-swapped (high word first).
* NibePi **silently clears communication alarms** (251) — our read-only gateway will not, so a 251 alarm raised
  during switch-over may stay visible until reset on the display. Whether 251 self-clears is unknown.
* When NibePi stops, values it last wrote (e.g. heat offset 47011, room setpoint 47398, curve) **stay in the
  pump**. Record them before migrating (feasibility.md §5).

## 4. Register handling
* Model detection from product info (0x6D): `model = string.split(" ")[0].split("-")[0]`, `firmware = data[6]*256+data[7]`
  (index.js:382-396); then loads `models/<model>.json`.
* `models/F730.json`: 578 registers, fields `register, factor, size, mode (R | R/W), titel, info, unit, min, max[, map]`.
  Cross-check: identical in type/factor/mode/min/max to NIBE ModbusManager export DB 8310 for all 575 common
  registers (see register-catalog.md).
* Read decoding (index.js `decodeMessage`, :1010-1148): data message 32-bit = low word at N, high at N+1;
  read response 32-bit = high word first (consistent with 48852 = 0). Range check flags values outside min/max
  as "corrupt" and drops them.
* Write path (`setDataValue`, index.js:661-755): clamps to min/max (but still sends the clamped value only if not
  "corrupt"); 32-bit writes little-endian `v0..v3`.

## 5. Privacy / network behaviour
* Every 30 min (and at start) posts `{id: md5(last 8 chars of MAC)[-10:], model, fw, version}` to
  `http://dev.anerdins.se:18081/` (index.js `updateID`/`sendID`, :1445-1500, cadence :454). Our gateway does not phone home.

## 6. Logging hook usable for capturing real traffic without replacing NibePi
`/etc/nibepi/config.json` → `"log": {"enable": true, "core": true, "debug": true, "error": true}` makes
`backend.js:115` log **every raw serial chunk** to `/tmp/nibepi.log` (log.js) as
```
2026-10-22 21:03:04.123 RAW: [92,0,32,105,0,73]
```
(timestamp local time with ms; decimal byte list; chunks as delivered by node-serialport, not frames; our own
transmissions are **not** logged as RAW, only as "SENT"/"OK" debug lines). This is the safest first capture
method — NibePi keeps acknowledging the pump. WS04 should provide an importer `nibepi-log → capture-jsonl`
(HARDWARE_PENDING has the steps). Note `/tmp` is likely tmpfs: copy the file off before reboot.

## 7. Things to copy / not copy
| Copy | Do not copy |
|---|---|
| ACK everything addressed to us, NAK checksum errors | Start-up writes (45171, 48852) |
| One read per read token, on-demand reads before round-robin | Blind `testmode` reads of arbitrary registers |
| Leading-`06` tolerance, `C5` checksum rule | XOR from byte 2 (breaks for 16-bit addresses — irrelevant but sloppy) |
| Use 0x6D product info for model detection | Swallowing out-of-range values silently (we report them with a quality flag) |
| | RMU40 emulation (out of scope unless the owner relies on it — check pump menu) |
| | Cloud ping |
