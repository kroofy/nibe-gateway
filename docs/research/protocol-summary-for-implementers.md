# NIBE F-series accessory bus — implementer summary

Status: **derived from source code of three independent open implementations; not yet observed on the owner's F730.**
Labels: **[S]** = verified in source code / third-party published bus bytes, **[H]** = hypothesis, **[C]** = sources conflict.
Full evidence and citations: [protocol.md](protocol.md). Executable oracle: [`tools/nibe_ref.py`](tools/nibe_ref.py)
(`python3 docs/research/tools/nibe_ref.py selftest`). Test vectors: [`fixtures/serial-captures/`](../../fixtures/serial-captures/).

## 1. Physical / link layer
| Item | Value | |
|---|---|---|
| Medium | RS-485 half duplex, 2-wire A/B (+GND), pump is bus master | [S] |
| Serial | **9600 baud, 8 data bits, no parity, 1 stop bit**, no flow control (no RTS/CTS, no XON/XOFF), raw mode | [S] |
| Byte time | 10 bits / 9600 = **1.042 ms per byte** | arithmetic |
| Linux device | `/dev/ttyAMA0` on NibePi (PL011 moved off Bluetooth); see [hardware.md](hardware.md) | [S]/[H] |
| Direction control | automatic in the RS485 HAT hardware → software just writes bytes | [H] (schematic) |

## 2. Our role
We emulate the **MODBUS40 accessory, bus address `0x0020`** (wire bytes `00 20`). The pump polls us; we only ever
transmit **immediately in reply** to a frame addressed to `00 20`. We never transmit unsolicited. [S]

Other addresses on the same bus (we must **stay silent** for them): `00 16` SMS40, `00 19`..`00 1C` RMU40 S1..S4,
`00 02` ECS, `41 C9`..`48 C9` heat-pump internal (16-bit). [S]

## 3. Frame formats
### 3.1 Pump → accessory ("response frame", start `0x5C`)
```
+------+---------+---------+-----+-----+------------------+-----+
| 5C   | ADDR_HI | ADDR_LO | CMD | LEN | DATA (LEN bytes) | CHK |
+------+---------+---------+-----+-----+------------------+-----+
  0       1         2        3     4     5 .. 4+LEN         5+LEN      total = LEN + 6 bytes
```
* ADDR is 16-bit big-endian (MODBUS40 = `00 20`). [S]
* **Escaping**: every `0x5C` byte inside DATA is transmitted twice (`5C 5C`). [S]
* **LEN counts the bytes on the wire, i.e. after escaping** (a 20-slot data message with one escaped byte has LEN `0x51`, not `0x50`). [S]
* **CHK = XOR of bytes 1 .. 4+LEN as on the wire** (ADDR_HI, ADDR_LO, CMD, LEN, escaped DATA). The `0x5C` start byte is **not** included. [S]
* **If that XOR equals `0x5C`, the pump sends `0xC5` instead.** Accept a frame iff `chk == calc || (calc == 0x5C && chk == 0xC5)`. [S]
  Do **not** copy nibegw.c's `if (calc != 0x5C && chk != 0xC5)` — it accepts any frame whose checksum byte is `C5`.
* Unescape (`5C 5C` → `5C`) **after** verifying the checksum. Header bytes are never escaped in any observed frame; LEN ≥ `0x5C` has never been observed (max seen `0x54`). [H]

### 3.2 Accessory → pump ("request frame", start `0xC0`)
```
+------+-----+-----+------------------+-----+
| C0   | CMD | LEN | DATA (LEN bytes) | CHK |      total = LEN + 4 bytes
+------+-----+-----+------------------+-----+
```
* **CHK = XOR of all preceding bytes INCLUDING the `C0` start byte.** [S]
* No address field. No escaping is done by any studied implementation. [S] Whether the pump would need escaping of `5C` inside request DATA is unknown. [H]
* **If CHK would be `0x5C`: [C]** yozik04/nibe sends `0xC5`, NibePi sends `0x5C`. → **Policy: never send such a frame until verified on hardware** (affects read requests for F730 registers **40043** and **48968**; request DATA containing `5C` affects **48732**). Catalog flags: `read_request_checksum_0x5c`, `address_contains_0x5c`.

### 3.3 Single-byte replies
| Byte | Meaning | |
|---|---|---|
| `06` | ACK | [S] |
| `15` | NAK (checksum error) | [S] |

The pump also sends a bare `06` after receiving an accessory's request frame; it may arrive glued in front of the next `5C` frame. [S] (NibePi `backend.js` strips a leading `06`; yozik04/nibe test vector `c0ee03ee0101c3 06`.)

## 4. Commands (CMD byte)
| CMD | Direction | Name | DATA | Our reply |
|---|---|---|---|---|
| `69` LEN 0 | pump→us | **read token** — "you may send one read request now" | – | read request frame or `06` |
| `6B` LEN 0 | pump→us | **write token** | – | **always `06`** (read-only gateway) |
| `68` | pump→us | **data message** (periodic register dump) | N × 4 bytes: `reg u16 LE`, `value 2 bytes LE`; reg `FFFF` = empty slot; typically 20 slots (LEN `0x50` before escaping) | `06` |
| `6A` | pump→us | **read response** | `reg u16 LE`, `value 4 bytes` (see §6) | `06` |
| `6C` | pump→us | write response | 1 byte: `01` accepted, `00` denied | `06` |
| `6D` | pump→us | **product info** (~every 15 s [S, nibe-py comment]) | `u8 ?`, `firmware u16 **BE**`, ASCII model string e.g. `F1255-12 R` | `06` |
| `6E` | pump→us | "modbus address msg" | `u8` | `06` [H] |
| other | pump→us (to `00 20`) | unknown | – | `06` (what NibePi does) [S] |
| `69` | us→pump | **read request** | `reg u16 LE` → `C0 69 02 lo hi CHK` | – |
| `6B` | us→pump | write request | `reg u16 LE`, `value 4 bytes LE` → `C0 6B 06 lo hi v0 v1 v2 v3 CHK` | **never sent by default** |

Examples (all verified by `nibe_ref.py selftest`):
```
read token   5C 00 20 69 00 49            write token  5C 00 20 6B 00 4B
read req     C0 69 02 A0 A9 A2            (register 43424 = 0xA9A0)
read resp    5C 00 20 6A 06 A0 A9 F5 12 00 00 A2        (43424 = 4853 with word-swap=1)
write resp   5C 00 20 6C 01 01 4C (ok)    5C 00 20 6C 01 00 4D (denied)
escaped      5C 00 20 6A 07 4F 9C 5C 5C 00 2C 00 B2     (40015 raw 0x005C = 9.2 °C, LEN 7 on wire)
```

## 5. Reply state machine (read-only MODBUS40 emulation)
```
on complete 0x5C frame with plausible header:
    if addr != 0x0020:                       -> no transmission (stay silent)
    elif checksum bad:                       -> send 15 (NAK)
    elif CMD==69 && LEN==0 (read token):     -> if a read is queued: send ONE read request frame, else send 06
    elif CMD==6B && LEN==0 (write token):    -> send 06
    else:                                    -> send 06, then deliver the unescaped frame to the decoder
on 06 / 15 received:                         -> pump's ack/nak of our last request (log/count it)
on C0 frame received:                        -> another accessory's request (or echo); ignore
on anything else:                            -> garbage counter; keep hunting for 0x5C
```
* Reply **as fast as possible** after the last byte of a frame (CHK). The pump's deadline is **unknown [H]**;
  target < 10 ms. The serial task must never wait on HTTP, disk or locks held by other tasks.
* If the pump gets no reply it raises **alarm 251 "communication interrupted (Modbus)"** (register 45001). [S]
* Frame length is known after byte 4, so completion = `received >= LEN + 6`. Max frame seen: 90 bytes. Use a bound
  (e.g. LEN ≤ 0x80) and a header plausibility check (`00 {02,16,19..1C,20}` or `{41..48} C9`) for resync.
* After a checksum failure **resume scanning at the byte after the `5C`**, not after the claimed frame (a corrupted
  LEN must not swallow the next frames). (Reference: `parse_stream()` in `tools/nibe_ref.py`.)
* One outstanding read at a time; match the `6A` response by register number. Response may arrive one or more
  token cycles later. [H] Timeout ≈ 5 s, ≤ 3 retries (yozik04/nibe) / 7 s re-request, 30 s give-up (NibePi). [S]
* Do **not** answer `0xEE` (accessory version), `0x60/0x62/0x63` (RMU40) — those go to RMU addresses; we don't emulate RMU40.

## 6. Value decoding
* Data types from the catalog: `u8 s8 u16 s16 u32 s32`; engineering value = `raw / scale`. [S]
* **Data message (0x68)**: each slot value is 2 bytes LE. 8-bit types: use the **low byte only** (`FD 00` = s8 −3). [S]
  32-bit registers occupy **two slots**: register N = low word, N+1 = high word (e.g. `C5 A5 AD 98` `C6 A5 01 00` → 42437 = 0x000198AD). [S for word_swap=1 pumps; H in general]
* **Read response (0x6A)**: 4 value bytes. 8/16-bit: first 1/2 bytes LE (rest ignored). 32-bit: depends on pump setting
  **48852 "MODBUS40 word swap"** (menu 5.3.11): `1` → plain LE `v0 v1 v2 v3`; `0` → high word first `v2 v3 v0 v1`. [S]
  **NibePi writes 48852 = 0 on every start** [S], so the owner's pump is most likely at 0. Read 48852 (read request,
  harmless) and/or let the user confirm in menu 5.3.11; **never write it**. Whether 48852 also affects 0x68 data
  messages is **[C]** (NibePi: no; yozik04/nibe: yes) → verify on hardware with a 32-bit counter (43420).
* **Sentinels [H]** (yozik04/nibe): s16 `0x8000`, u16 `0xFFFF`, u8 `0xFF`, s8 `0x80`, s32 `0x80000000`, u32 `0xFFFFFFFF` → "no value / sensor missing". Report as unavailable, keep raw.
* Product info: model family = first token of the ASCII string, cut at `-` or space (`F730 …` → `F730`). Firmware = bytes 6–7 big-endian. [S]

## 7. Things the gateway must NOT do
* No writes at all by default (write token → `06`). In particular never send NibePi's start-up writes
  `C0 6B 06 73 B0 01 00 00 00 6F` (45171 alarm reset = 1) and `C0 6B 06 D4 BE 00 00 00 00 C7` (48852 word swap = 0).
* No ACK/NAK for frames not addressed to `00 20`. No RMU40 emulation.
* No blind register scanning; read requests only for catalog registers, rate-limited (≤ 1 per read token).

## 8. Timing budget (estimates)
* Token frame 6 B ≈ 6.3 ms on the wire; 20-slot data message 86 B ≈ 90 ms; our ACK 1 B ≈ 1 ms; read request 6 B ≈ 6.3 ms.
* Token period, data-message period and ACK deadline: **unknown [H]** — measure on hardware
  (see `docs/progress/HARDWARE_PENDING.md`). Capture with monotonic µs timestamps.
