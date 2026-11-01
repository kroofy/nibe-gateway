# F730 register catalog — provenance and schema

File: [`fixtures/register-catalogs/f730.json`](../../fixtures/register-catalogs/f730.json) (581 registers).
Generator: [`tools/build_f730_catalog.py`](tools/build_f730_catalog.py) (inputs and commands in its header).

## 1. Sources and cross-check
| Source | Registers | Role |
|---|---|---|
| NibePi `models/F730.json` @ 1aa36b0 (MIT) | 578 | **Base record** (title, description, type, scale, unit, access, min/max, value maps) — this is what the owner's NibePi uses today |
| NIBE ModbusManager 1.0.9 CSV, "Product: F730", "Database: 8310", 2020-06-24 (via yozik04/nibe `nibe/data/f730.csv`) | 576 | Field-by-field cross-check; `raw_default`; fills registers missing in NibePi |
| yozik04/nibe `nibe/data/extensions.json` (GPL-3.0) | 23 touching F730 | Community corrections, recorded as `conflicts` (not applied unless an owner check confirms them, `UNIT_SCALE_OVERRIDES`); adds 48852 and 48889 (protocol-control registers absent from both lists) |

Cross-check result (computed 2026-10-22):
* 575 registers in both NibePi and ModbusManager: **data type, scale, access and raw min/max identical for all 575**.
  Only differences: 20 NibePi titles carry a stray leading `=` (`=+Adjust …`, 40868–40888) → stripped.
* Only in NibePi: 43110 "Heating allowed", 43111 "Cooling allowed", 47260 "Fan Mode".
* Only in ModbusManager: 47340 "Cooling with room sensor" (CSV says scale 10 but the description is an enum —
  community says scale 1; recorded as conflict).
* Community corrections recorded as conflicts on 13 registers in the shipped file (the current builder gives 10, because owner overrides settle some; the shipped `chosen` values predate them), e.g. 40051 scale 100→10, 43141/43375 W→kW with
  scale 100, 40050/41026 unit m³/h, 47377 type u8→s16, 41256–41258/43108 unit % → enum.

So the catalog is, in effect, the manufacturer's export; the open question is not the numbers but **whether this
F730's firmware supports each register** — hence `verification: "unverified"` everywhere.

## 2. Schema (schema_version 1)
Top level: `schema`, `schema_version`, `catalog_id`, `catalog_version`, `model`, `model_match`, `firmware`,
`semantics`, `write_policy` (always `"disabled"`), `generated_by`, `sources{}`, `counts`, `registers[]`.

| Field | Type | Meaning |
|---|---|---|
| `id` | int | Register number as used on the bus (40004 = `44 9C` LE) |
| `name` | string | Stable slug `<title>-<id>` (same convention as yozik04/nibe, unique) |
| `title`, `description` | string / null | English text from the source |
| `data_type` | `u8 s8 u16 s16 u32 s32` | Raw integer type (LE) |
| `scale` | int | **engineering value = raw / scale** |
| `unit` | string / null | Display unit as given by the source, or an owner override (null if none) |
| `access` | `read` \| `read_write` | What the source **claims**. Not a permission. |
| `write_allowed` | bool | Always `false` in shipped catalogs. Only a hardware-verified, audited allow-list may enable a write. |
| `raw_min`, `raw_max` | number / null | Raw (unscaled) limits; `null` = source gives 0/0 (= unspecified) |
| `raw_default` | number / null | ModbusManager default (raw), only when min ≠ max |
| `value_map` | object / null | Raw value → label (from NibePi maps, translated to English) |
| `flags` | string[] | See below |
| `sources` | string[] | Keys into top-level `sources` |
| `conflicts` | object[] | `{field, source, value, chosen}` disagreements, not applied unless an owner check confirms them |
| `notes` | string[] | Free text provenance notes |
| `verification` | string | Catalog-level state, always `unverified` here |

Flags: `two_slot_32bit` (u32/s32 occupy N and N+1 in data messages), `address_contains_0x5c` (48732),
`read_request_checksum_0x5c` (40043, 48968 — see protocol.md 3.9), `enum_in_description`, `alarm` (45001),
`alarm_reset` (45171), `protocol_control` (48852, 48889), `nibepi_writes_on_startup` (45171, 48852),
`never_write` (45171, 48852, 48889).

## 3. Register states (brief §7) — mapping
The catalog only carries static knowledge. Runtime discovery state is separate and keyed by `id`:

| State | Set by |
|---|---|
| `catalog-known` | present in the loaded catalog |
| `observed` | seen in a 0x68 data message or a 0x6A response on this pump |
| `confirmed-readable` | a read request returned a plausible, non-sentinel value |
| `unsupported` | read request timed out repeatedly / pump never answers (behaviour HYPOTHESIS) |
| `unverified` | catalog-known, never observed |
| `unknown/undecoded` | seen on the bus but not in the catalog → show raw only, never writable |

Verification on hardware (owner): compare a handful of values against the F730 display (HARDWARE_PENDING),
then mark those `verified` in a *separate* per-installation overlay, not in this shared fixture.

## 4. Suggested first read set (read-only monitoring, M2)
40004 BT1 outdoor, 40008 BT2 supply S1, 40012 BT3 return, 40013 BT7 HW top, 40014 BT6 HW load,
40025 BT20 exhaust air, 40026 BT21 vented air, 40033 BT50 room S1, 43005 degree minutes, 43009 calc. supply S1,
43084 int. el. add. power, 43086 prio, 43136 compressor frequency, 43437 supply pump speed, 45001 alarm,
43416/43420/43424 compressor counters (32-bit, word-order test), 48852 word swap (read only).
These are all `access: read` except 43005 and 48852 (never written by us).

## 5. Regenerating
```
git clone --depth 1 https://github.com/anerdins/nibepi /tmp/nibepi
git clone --depth 1 https://github.com/yozik04/nibe /tmp/nibe-py
python3 docs/research/tools/build_f730_catalog.py --nibepi /tmp/nibepi/models/F730.json \
  --mm-csv /tmp/nibe-py/nibe/data/f730.csv --ext /tmp/nibe-py/nibe/data/extensions.json \
  --out fixtures/register-catalogs/f730.json
```
Upstream HEADs may have moved; the builder records the commits it was written against. Bump
`catalog_version` when regenerating; the builder hard-codes it.
