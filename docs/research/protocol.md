# NIBE F-series accessory protocol (MODBUS40 emulation) — research notes

Date: 2026-10-22. Short spec for implementers: [protocol-summary-for-implementers.md](protocol-summary-for-implementers.md).

## Evidence labels
| Label | Meaning |
|---|---|
| **VERIFIED-SOURCE** | Stated/implemented identically in source code we read (file cited). Not yet seen on our F730. |
| **VERIFIED-3P-CAPTURE** | Confirmed by bytes that third parties published as real bus data (other NIBE F-models, not our F730), re-checked by `tools/nibe_ref.py selftest`. |
| **CONFLICT** | Sources disagree; a policy is chosen and a hardware test is listed. |
| **HYPOTHESIS** | Plausible inference, nothing authoritative. Needs hardware. |

Nothing in this file is **VERIFIED-HW** yet: the owner is remote and nothing was run against the F730.

## Sources inspected (cloned to /tmp, not into the repo)
| Id | Repository / file | Commit | License |
|---|---|---|---|
| NP | https://github.com/anerdins/nibepi (NibePi 1.1.x, what the owner runs) | `1aa36b02986d88778d765be738c68fb2bcabdbea` (2023-10-15) | MIT |
| NPold | https://github.com/bebben88/NibePi (older NibePi, Node-RED flows, update scripts) | `00e584cded0abf2bec6b31c0776f6426a363602b` (2020-10-20) | MIT |
| PY | https://github.com/yozik04/nibe (Python lib used by Home Assistant's NIBE integration) | `00f4ae88142edd4bc43da38b92b0b47b7c519930` | GPL-3.0 |
| OH | https://github.com/openhab/openhab-addons `bundles/org.openhab.binding.nibeheatpump` (Java binding + `contrib/NibeGW` C/Arduino gateway by Pauli Anttila) | `d0a0a95e485c77402ea472d3e14ddfe1caeadae9` | EPL-2.0 |
| MM | NIBE ModbusManager 1.0.9 CSV export for F730 (DB 8310, 2020-06-24), as redistributed in PY `nibe/data/f730.csv` | (PY commit) | manufacturer data |

Below, `NP:backend.js:118` means file `backend.js` line 118 in NP at the commit above.

## 1. Physical layer
| # | Fact | Label | Evidence |
|---|---|---|---|
| 1.1 | RS-485 half-duplex bus between pump and accessories; NibePi uses a Pi Zero + RS485 HAT on terminals A/B (+12 V, GND). | VERIFIED-SOURCE | NP:README.en.md "RS485 HAT", "Connect … (12v,A,B,GND)" |
| 1.2 | 9600 baud. | VERIFIED-SOURCE | NP:backend.js:54 `new serialport(portName, 9600)`; OH:contrib/NibeGW/RasPi/nibegw.c `initSerialPort()` `B9600`; OH Arduino NibeGw.cpp `begin(9600, SERIAL_8N1)` |
| 1.3 | 8N1, no parity. | VERIFIED-SOURCE | nibegw.c header comment "Serial settings: 9600 baud, 8 bits, Parity: none, Stop bits 1"; node-serialport default 8N1 (NP) |
| 1.4 | No hardware flow control on Pi RS485 HATs. nibegw.c *defaults* to CRTSCTS (option `-f` disables) and v1.31 notes "Fixed serial settings (for RPi Zero 2 W + Waveshare RS485 CAN hat)". NibePi uses node-serialport defaults (rtscts=false). | VERIFIED-SOURCE | nibegw.c `initSerialPort(fd, hwflowctrl)`, changelog; NP:backend.js:54 |
| 1.5 | NibePi's serial device is `/dev/ttyAMA0`. | VERIFIED-SOURCE | NP:default.json:9; NPold:nibepi/heatpump_1.0.js:28 |
| 1.6 | The pump is the only master; accessories transmit only in reply to frames addressed to them. | VERIFIED-SOURCE | NP `makeResponse()` only writes in reply to a received frame; OH NibeGw.cpp `loop()` same |

## 2. Addresses
| # | Fact | Label | Evidence |
|---|---|---|---|
| 2.1 | Bytes 1–2 of a pump frame are a 16-bit big-endian address. `0x0020` MODBUS40, `0x0016` SMS40, `0x0019`–`0x001C` RMU40 S1–S4, `0x0002` ECS S2, `0x41C9`…`0x48C9` heat pump 1–8 (internal). | VERIFIED-SOURCE (+3P capture for 0x41C9) | PY nibegw.py `Address = Enum(Int16ub, …)`; PY test `test_parse_token_response_16bit_address` (`5c41c9f7007f`); OH nibegw.c header "0x0016 SMS40, 0x0019 RMU40, 0x0020 MODBUS40" |
| 2.2 | NibePi only processes frames whose byte 2 is 0x19–0x1C or 0x20 (ignores byte 1). | VERIFIED-SOURCE | NP:backend.js:138 |
| 2.3 | A gateway must only ACK/NAK frames addressed to its own emulated address (`0x20`); acking other addresses is an opt-in in OH (`sendAckToRMU40`, `sendAckToSMS40` default false). | VERIFIED-SOURCE | OH README "sendAckToMODBUS40 … true, sendAckToRMU40 … false"; NibeGw.cpp `shouldAckNakSend()` |

## 3. Frame layout and checksum
| # | Fact | Label | Evidence |
|---|---|---|---|
| 3.1 | Pump frame: `5C ADDR_HI ADDR_LO CMD LEN DATA[LEN] CHK`, total LEN+6. | VERIFIED-SOURCE | nibegw.c header diagram; NP:backend.js:143 `msgLength = msgOut[4]+6`; OH `NibeHeatPumpProtocol.RES_OFFS_*` |
| 3.2 | Pump CHK = XOR of bytes 1..LEN+4 (start byte excluded). NibePi starts at byte 2 (equivalent because byte 1 is 0 for accessory addresses). | VERIFIED-SOURCE + 3P | nibegw.c `checkMessage()` loop `i = 1; i < datalen + 5`; NP:backend.js:217-219; OH `checkMessageChecksumAndRemoveDoubles` `startIndex = 2`; selftest |
| 3.3 | If the XOR is `0x5C` the pump transmits `0xC5`. | VERIFIED-3P-CAPTURE | PY test `test_parse_multiple_with_5c_checksum` (real frame ends `…c5`, XOR = 0x5C); NP:backend.js:221; OH both implementations; PY `xor8()` |
| 3.4 | Inside DATA, `0x5C` is doubled on the wire. | VERIFIED-3P-CAPTURE | PY tests `test_parse_escaped_read_response`, `test_parse_multiple_escaped_value`, `test_parse_multiple_heavily_escaped_value`; NP:backend.js:170-178 (dedup); OH `removeEscapedDuplicates()` |
| 3.5 | **LEN and CHK refer to the escaped (wire) bytes.** Proof: `5c00206a074f9c5c5c002c00b2` has LEN 7 (6 payload bytes + 1 escape) and XOR over wire bytes = `b2` = CHK; XOR over unescaped bytes would be `ee`. | VERIFIED-3P-CAPTURE | computed by `tools/nibe_ref.py selftest`; PY `Prefixed(Int8ub, Dedupe5C(...))`; OH `newlen--` when removing duplicates |
| 3.6 | Escaping applies to DATA. No source escapes header bytes; no observed frame has a header byte `5C` (max LEN seen `0x54`). | HYPOTHESIS | PY `Dedupe5C` wraps only the data; OH dedups from index 1 (would also cover header) |
| 3.7 | Accessory frame: `C0 CMD LEN DATA[LEN] CHK`, CHK = XOR of all previous bytes incl. `C0`. | VERIFIED-SOURCE + 3P | NP:index.js:756 `Calc_CRC` (i from 0 to LEN+5); PY `Request` `RawCopy(RequestData)` incl. `Const(C0)`; OH `checkMessageChecksumAndRemoveDoubles` `startIndex = 0` for requests; PY tests `c06902a0a9a2`, `c06b06ec9f9cffffffbd` |
| 3.8 | Accessory frames are not escaped by anyone. Whether the pump expects escaping of `5C` in request DATA is unknown. | HYPOTHESIS | PY `RequestData` has no `Dedupe5C`; NP builds raw arrays |
| 3.9 | **CONFLICT**: request CHK equal to `0x5C`. PY substitutes `0xC5` (`xor8` used for `Request`); NP `Calc_CRC` does not. Policy: refuse to send (affects F730 read requests for 40043, 48968). | CONFLICT | PY nibegw.py `xor8`, `Request`; NP:index.js:756 |
| 3.10 | ACK = single byte `0x06`, NAK = single byte `0x15`. NAK is sent on checksum failure (only for own address). | VERIFIED-SOURCE | NP:backend.js:34-35,199; nibegw.c `sendAck()/sendNak()`; NibeGw.cpp `STATE_CRC_FAILURE` |
| 3.11 | The pump acknowledges an accessory's request frame with `06`; NibePi has to strip a leading `06` glued to the next `5C` frame. | VERIFIED-SOURCE + 3P | NP:backend.js:117-121 comment "Sometimes and ACK [0x06] is the first byte"; PY test `test_read_product_info_with_extras` ("06 # ack from pump") |

## 4. Commands
| CMD | Name (PY enum) | Direction | Payload | Accessory reaction | Label / evidence |
|---|---|---|---|---|---|
| 0x69, LEN 0 | MODBUS_READ_REQ ("read token") | pump→acc | — | send one read request or `06` | VERIFIED-SOURCE: NP:backend.js:306; nibegw.c "Read token received"; OH `isModbus40ReadTokenPdu` |
| 0x6B, LEN 0 | MODBUS_WRITE_REQ ("write token") | pump→acc | — | send one write request or `06` | VERIFIED-SOURCE: NP:backend.js:338; nibegw.c; OH |
| 0x69 | read request | acc→pump | reg u16 LE | — | VERIFIED-SOURCE+3P: NP:index.js:506-511; PY `ModbusReadReq`; `c06902a0a9a2` |
| 0x6A | MODBUS_READ_RESP | pump→acc | reg u16 LE + 4 value bytes | `06` | VERIFIED-3P: PY `ModbusReadResp`, tests; NP:backend.js else-branch writes ack |
| 0x6B | write request | acc→pump | reg u16 LE + 4 value bytes LE | — | VERIFIED-SOURCE+3P: NP:index.js:718-727; PY `ModbusWriteReq` |
| 0x6C | MODBUS_WRITE_RESP | pump→acc | 1 byte: 1 ok / 0 denied | `06` | VERIFIED-3P: PY `test_succesfull_write_response`, `test_failed_write_response` |
| 0x68 | MODBUS_DATA_MSG | pump→acc | up to 20 × (reg u16 LE, value 2 B LE); reg 0xFFFF = empty | `06` | VERIFIED-3P: PY `ModbusData`, tests; OH `isModbus40DataReadOut` (LEN ≥ 0x50); OH README "A telegram contains a maximum of 20 registers" |
| 0x6D | PRODUCT_INFO_MSG | pump→acc | u8 ?, version u16 BE, ASCII model | `06` | VERIFIED-3P: PY `ProductInfoData`, test `F1255-12 R`/`F1155-16`; NP:index.js:382-396 (`firmware = data[6]*256+data[7]`) |
| 0x6E | MODBUS_ADDRESS_MSG | pump→acc | u8 address | `06` (default branch) | HYPOTHESIS (PY enum only) |
| 0xEE | ACCESSORY_VERSION_REQ | pump→RMU | — | RMU answers `C0 EE 03 EE 03 01 C1` (NP emulates RMU v259) | VERIFIED-SOURCE: NP:backend.js:288-297; PY `AccessoryVersionReq` |
| 0xEE to 0x20 | ACCESSORY_VERSION_REQ to MODBUS40 | pump→acc | — | ACK (`06`), same as NibePi: its MODBUS40 branch treats 238 as an empty case and writes `ack` (anerdins/nibepi backend.js 0x20 else-branch; bebben 1.1 backend.js:334). The `C0 EE 03 EE 03 01 C1` reply is RMU40-only (addresses 0x19–0x1C) | VERIFIED-HW 2026-10-24: owner's F730 sends `5C 00 20 EE 00 CE` every ~1.5 s while no accessory answers (`real-f730-passive-2026-10-24`). With ACKs (HP-26 run 1) it slowed to ~4.7 s but did not stop |
| 0x60/0x62/0x63 | RMU write/data/data-req | RMU40 only | — | NP emulates RMU40 when no real RMU answers | VERIFIED-SOURCE: NP:backend.js:233-287 |
| 0xB1 | STRING_MSG | pump→RMU | strings (ISO-8859-1) | — | 3P test `test_parse_strings` |
| 0x73/0xB7/0xED/0xF1/0xF2/0xF7/0xFC/0xA0/0x55/0x90 | internal heat-pump/ECS traffic | – | – | ignore | PY enum + tests (VVM500 etc.) |

## 5. Bus sequencing and timing
| # | Fact | Label | Evidence |
|---|---|---|---|
| 5.1 | Pump sends tokens to `0x20`; accessory may put **one** request per token. NibePi alternates between an on-demand queue and a round-robin list of registers (`regQueue`) on read tokens; one write per write token. | VERIFIED-SOURCE | NP:backend.js:306-353, index.js:905 |
| 5.2 | Without a reply the pump raises an alarm (alarm 251 "Kom.avb Modbus" on F730). | VERIFIED-SOURCE | OH README Prerequisites ("must be acknowledged, otherwise the heat pump will raise an alarm"); NP models/F730.json 45001 map `251: "Kom.avb Modbus"`; NP:index.js:416-425 auto-reset on 251 |
| 5.3 | Product info message roughly every 15 s. | VERIFIED-SOURCE (comment) | PY connection/__init__.py `READ_PRODUCT_INFO_TIMEOUT = 20  # Product info message is sent every 15 seconds` |
| 5.4 | Read-response timeout used by implementations: PY 5 s × 3 retries; NP re-queues after 7 s, gives up after 30 s. | VERIFIED-SOURCE | PY `DEFAULT_TIMEOUT = 5`, `read_retries=3`; NP:index.js:502,522 |
| 5.5 | Deadline for ACK after a frame; token period; data-message period; delay until read response. | **UNKNOWN** | No source states them. Implementations reply immediately in their receive path (Arduino adds 1 ms DE settle before and after: NibeGw.cpp `sendAck()`). |
| 5.6 | The set of registers in the 0x68 data message is chosen by a `LOG.SET` file on a USB stick created with NIBE ModbusManager; register 48889 can make the pump ignore it. Default content without LOG.SET on F730 is unknown. | VERIFIED-SOURCE / HYPOTHESIS | OH README ("Those 20 registers can be configured via the Nibe ModbusManager"); PY extensions.json 48889 "MODBUS40 Disable LOG.SET … USB stick" |

## 6. Accessory activation on the pump
| # | Fact | Label | Evidence |
|---|---|---|---|
| 6.1 | Activate "Modbus" as accessory: hold *Back* ~7 s → service menu → 5.2 System settings (some models need a sub-menu) → tick "Modbus". Alarm until the accessory answers. | VERIFIED-SOURCE | NP:README.en.md "Activate Modbus in the heatpump" |
| 6.2 | Word-swap setting visible in service menu **5.3.11** (MODBUS40 settings). | VERIFIED-SOURCE (doc) | PY README "word swap disabled in 5.3.11 service menu" |
| 6.3 | RMU40 accessories S1–S4 are separately activated; NibePi emulates an RMU40 for any RMU address the pump polls and no real RMU answers. | VERIFIED-SOURCE | NP:backend.js:145-161 (`checkACK`), index.js:459-490 |

## 7. Values
| # | Fact | Label | Evidence |
|---|---|---|---|
| 7.1 | Types `u8 s8 u16 s16 u32 s32`, scaling factor (divide), raw min/max, R or R/W per register come from the ModbusManager export. | VERIFIED-SOURCE | MM CSV header `Title;Info;ID;Unit;Size;Factor;Min;Max;Default;Mode`; NP models/*.json |
| 7.2 | Data-message slot value is 2 bytes LE; s8 is sign in the low byte (`FD 00` → −3). | VERIFIED-3P | PY test `test_parse_multiple_read_request` (47011 `fd00`); NP:index.js s8 branch |
| 7.3 | 32-bit registers appear as two consecutive slots N (low word) and N+1 (high word) in data messages — confirmed for a pump with word swap = 1; NibePi decodes data messages the same way while forcing word swap = 0. PY decodes data messages through the word-swap-aware decoder. | **CONFLICT** (for word swap 0) | PY test `test_read_multiple_with_u32` (42437 = `c5a5ad98 c6a50100` → 10462.1, `word_swap=True`); NP:index.js:1030-1033; PY `_on_raw_coil_set` |
| 7.4 | Read response 32-bit order: 48852 = 1 → `v0 v1 v2 v3` LE; 48852 = 0 → words swapped (high word first). | VERIFIED-SOURCE (+3P for =1) | PY encoders.py `CoilDataEncoderNibeGw._get_parser` ("yes, it is visa versa"); PY test `test_read_s32_coil`; NP:index.js:1039 (high word first, NP forces 0) |
| 7.5 | NibePi writes 45171 = 1 (alarm reset) and 48852 = 0 (word swap off) at **every start**, unconditionally, and writes 45171 = 1 again whenever 45001 reads 251. | VERIFIED-SOURCE | NP:backend.js:37 `sendQueue` initial content, :133-136; NP:index.js:416-425; same in NP:nibegw-client.js:6 |
| 7.6 | Integer-limit values mean "no value". | HYPOTHESIS | PY encoders.py `is_hitting_integer_limit`; PY test `test_invalid_values` (RMU 0x8000 → None) |
| 7.7 | RMU data (0x62) uses odd fixed-point offsets (±5, setpoint +50/−7). Irrelevant unless we emulate RMU40. | VERIFIED-SOURCE | PY `FixedPointStrange`, `RmuData`; NP:index.js `decodeRMU` |

## 8. Generic Modbus RTU is NOT this protocol
The "MODBUS40" name refers to NIBE's RS-485 Modbus RTU accessory box; NibePi/NibeGW **emulate the accessory on
NIBE's internal accessory bus** instead. There are no Modbus function codes, no CRC-16, no slave IDs: tokens
from the pump, XOR checksums, `5C`/`C0` start bytes and byte-stuffing. PY has a separate `modbus.py` connection for
pumps reached through a *real* MODBUS40 box or S-series TCP — not applicable here.
(Evidence: OH README "without using an expensive MODBUS40 adapter"; PY README "Connection methods".)

## 9. Open questions (each has a hardware step in `docs/progress/HARDWARE_PENDING.md`)
1. ACK deadline, token period, data-message period, read-response latency (5.5).
2. Current value of 48852 and its effect on 0x68 32-bit slots (7.3).
3. Request checksum `0x5C` rule and escaping of `5C` in request data (3.8, 3.9).
4. Whether alarm 251 self-clears when communication resumes, and how long a silent gap triggers it.
5. What the pump does with a read request for an unsupported register (no answer? `6A` with zeros?).
6. Which accessories (Modbus, RMU40 S1–S4) are activated on the owner's pump; LOG.SET present?
7. Real F730 product-info string and firmware number.
8. Whether we receive an echo of our own transmissions (HAT receiver enable during TX) — parser tolerates it either way.
