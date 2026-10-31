# F730 register survey on the owner's pump (2026-10-25)

Labels: **VERIFIED** = read from this pump in this survey (or cross-checked between registers). **SOURCE** = catalog
claim. **HYPOTHESIS** = our interpretation, to be confirmed against the display/myUplink.
Raw per-register results: `docs/research/register-survey-2026-10-25.json` (one object per catalog entry: state,
raw, decoded, class, tier, note, and the three re-reads where taken).

## Summary

- **All 576 readable catalog registers answered** (546 probed + 30 polled). **0 timeouts, 0 "unsupported".**
  The 5 others were not read by design: 45171 alarm reset, 48889 LOG.SET disable (never-write flags), and 40043,
  48732, 48968, which the gateway refuses because the read request would contain the 0x5C escape byte
  (`AmbiguousChecksum`).
- "Answers" is not the same as "useful": firmware 9721 answers for every climate system S1–S8 and every accessory,
  returning sentinels or factory defaults.

| Class | Count | Meaning |
|---|---:|---|
| (a) works, plausible, relevant | **165** | 55 core telemetry, 55 settings worth showing, 28 rare/installer settings, 27 service diagnostics |
| (b) works but irrelevant here | **396** | 43 return the "unavailable" sentinel; 353 return defaults/zeros for S2–S8 or accessories not fitted |
| (c) unsupported (timeouts) | **0** | |
| (d) suspicious decode or meaning | **15** | 40051, 40072, 40940, 41256, 41257, 41258, 43064, 43108, 43124, 43141, 43147, 43375, 44896, 49287, 49288 |
| not read (excluded) | 5 | 40043, 45171, 48732, 48889, 48968 |

**Recommended poll list (46 registers, ≈ 52 s per round** at the measured 1.13 s/register; today's 30 take ≈ 34 s):
today's 30 (40004 40008 40012 40013 40014 40025 40026 40033 40067 41846 41848 41850 43005 43009 43084 43136 43416
43420 43424 43437 44298 44300 45001 47007 47011 47041 47265 47398 48132 48852) **plus** the 12 myUplink graph
registers 40017 40018 40019 40020 40022 40050 40072 40079 40081 40083 44306 44308 **plus** 43086 (priority:
heat/hot water/off), 43141 (compressor input power, needs scale fix), 43081 (add-heat operating time) and
47260 (fan mode). See §4.

## 1. Method

- Gateway 0.1.10 on the Pi (release `d875b775…`), hardware reads enabled, control-scope API key.
- `POST /api/v1/discovery/probe` with 32 catalog IDs at a time, 18 sessions run one after another
  (15:29–15:44 CET). After each session `GET /api/v1/registers/explorer` was saved. The gateway restarted at
  15:37:20 for a supervised write test (coordinator); session 12 was interrupted and re-run in full, sessions
  12–17 ran on the new process.
- Each session of 32 took 34–41 s (one read per ~1.1 s token). **No probe timeouts, no poll timeouts during any
  session, 0 NAK/checksum errors.**
- 32 dynamic registers were then re-read three times, ~3 min apart (15:45, 15:48, 15:51) while the pump was
  charging hot water, to check that they move and agree with each other.
- Only read requests (0x69 → read request → 0x6A). No `/controls/` call, no writes.
- Decoding is the gateway's own (catalog type/scale, 32-bit high word first, 48852 = 0). Limitation: the
  explorer stores the decoded integer, not the four raw bytes, so a u16/u8 register returning 0x8000 would show
  as 32768/0. No u16 read 32768.

Observation (VERIFIED): **probe sessions starve the poll loop.** During the survey the poll counter advanced
only ~30 reads in 7.5 min (3520 → 3550), i.e. poll values went stale for minutes. Not a fault (ADR 0101 gives
probes priority), but UI/MQTT staleness during long probes should be expected, and a large probe run is visible
on graphs.

## 2. Sanity checks (VERIFIED by cross-reading)

Snapshot during hot-water charging, 15:45–15:51 (three reads each):

- **Mode**: 43086 Prio = 20 (hot water; 30 = heat earlier in the survey); 43427 compressor state 60, 43435
  Cpr status 1, 43431 supply pump state 20; 44282/44283 "used compressors HW/heat" = 0/1 during heating.
- **Compressor**: 43136 actual 35.0 Hz = 43182 target 35 = 40321 request 35. Limits 43122/43123 = 20/120 Hz,
  40322 max 120.00 Hz (scale 100 correct), 47103/47104 = 20/120.
- **Refrigerant circuit** (all moving, plausible for a hot-water charge): hot gas BT14 40018 64.7 → 74.0 → 80.3 °C;
  condenser out BT12 40017 51.3 → 53.7 °C; liquid BT15 40019 50.2 → 52.4 °C; evaporator BT16 40020 2.1–2.7 °C;
  suction BT17 40022 18.5 → 15.5 °C. Supply BT2 40008 47.7 → 51.3, return BT3 40012 49.1 → 50.9, BT6 40014
  47.8 → 48.7, BT7 40013 50.5 → 50.8 °C.
- **Flow dT**: 43065 "dT actual" −1.3 / 0.4 / 0.8 vs BT2−BT3 −1.4 / 0.2 / 0.4 → same quantity, scale 10 correct.
- **Air**: 40050 BS1 air flow 181.8 / 180.9 / 180.4, 41026 adjusted 180.1–181.0, 43124 airflow ref 180.7
  (unit m³/h per the community list; F730 nominal range fits). Exhaust air BT20 40025 23.0 °C, extract after
  evaporator BT21 40026 7.2 → 4.7 °C while the compressor runs (15.3 °C when idle).
- **Phase currents** 40083/40081/40079 (BE1/BE2/BE3): 5.6–5.8 / 4.0–4.4 / 2.1–2.2 A at 35 Hz; 7.0/2.4/2.2 A
  earlier. Plausible for 3×400 V with a 16 A fuse (47214 = 16). Current sensors are evidently fitted.
- **Counters monotonic**: 44306 HW-compressor heat 32461.2 → 32461.3, 44298 HW incl. add 33832.4 → 33832.5,
  41848 consumed for HW 10188.8 → 10188.9 kWh during the charge; heating counters static. 43416 starts 32324 →
  32325 between the morning and the survey.
- **Energy ratios** (lifetime): HW heat 33832 / consumed 10189 ≈ 3.3; heating heat 42243 / consumed 19969 ≈ 2.1
  (includes add heat: 44300 − 44308 = 6756 kWh from the immersion heater; HW 1371 kWh). Plausible.
- **Identity**: 43001 software version = **9721**, identical to the 0x6D product-info version → confirms u16
  decode and register numbering.
- **Add heat**: 43084 power 0.0 kW, 43091 state 0, 43081 total add time 1438.7 h, 43239 HW add time 236.8 h,
  static. **47370 "Allow Additive Heating" = 0** — the immersion heater is currently blocked by setting.

## 3. Classes in detail

### (b) Works but irrelevant on this installation (396)

- **Sentinel −32768 / 255 (43)**: supply/return/room sensors of S2–S8 (40005–40007, 40030–40032, 40127–40129,
  40159–40170, 40188–40194), solar 40044, BT25 40071, external water heater 40077/40078, BT74/BT64 averages
  40212/40889, +Adjust humidity/temps/need 40873–40875/40878, humidity 41027, EME20 42075, **24 h averages 42100
  "BT1 average 24h" and 42101 "used heating power average 24h"** (the 1 h average 40185 works), real air flow 48915.
- **Defaults for climate systems S2–S8 (151)**: heat curve 5, offset −1, min/max supply 20/40 °C, room setpoint
  20 °C, mixing valve state 10, "extra heating system pump" 1, calculated supply 20.0 °C, etc. These look like
  real values but are firmware defaults; an integration must hide them.
- **Accessories not fitted / features unused (202)**: EME20 AA23-BE5 block (41930–42032, all 0), +Adjust,
  PV panel, OEK/outdoor-air mixing (48890 "outdoor air fan speed" 100 % is a setting, no OEK), Smart Home,
  SCA, SMS40, solar, pool, SG Ready inputs, cooling and night-cooling settings, floor drying program, humidity,
  external energy meters, system-wide heat meters 42437–42447 (all 0: single-unit installation), language/date
  format. Full list in the JSON (`class: "b"`).

### (d) Suspicious (15)

| Reg | Title | Read | Problem / likely fix |
|---|---|---|---|
| 40051 | Air flow unfiltered | raw 1761–1894 → 17.6–18.9 | **Scale 10, not 100** (community conflict is right): 176–189 matches 40050/41026. Unit m³/h. |
| 43141 | compr. in power | raw 28 (20 Hz) / 76–79 (35 Hz) → "W" | **Scale 100, unit kW** (community): 0.28 / 0.76–0.79 kW. 76 W for a running compressor is impossible. |
| 43375 | compr. in power mean | same raw as 43141 | Same fix as 43141. |
| 43147 | Compr. in current | raw 3104 / 5125–5355 → "A" | Impossible as A. Most likely **mA (scale 1000)** → 3.1 / 5.1–5.4 A. No source records this. HYPOTHESIS. |
| 40940 | Degree minutes 32-bit | −1, 0, 0 | Disagrees with 43005 (14 = 1.4 DM) read at the same moment. Use 43005; treat 40940 as unmaintained on 9721. |
| 40072 | BF1 flow | 5.0 l/min, every read | Constant across heating and HW and pump speed 71–100 %. Likely no BF1 fitted and 5.0 a default. myUplink lists it → compare (HP-34). |
| 43064 | Heat medium flow dT set point | 2.0 (heat) / 53.0 (HW) | Mode-dependent; 53.0 is a temperature, not a dT. Meaning unclear; do not expose as "dT". |
| 41256–41258, 43108 | Fan speed current | always 0 % | Not the live fan speed on this unit (fan clearly runs). See §5. |
| 43124 | Airflow ref. | −32768 once, then 180.7 | Transiently unavailable; value plausible. Treat sentinel as gap. |
| 44896 | Smart Price Adaption heating offset | raw 1 → 0.1 | Offsets are whole steps; scale 1 (+1) more likely. Smart price adaption appears active (44908 state 30, 41929 level 1). HYPOTHESIS. |
| 49287 / 49288 | Energy meter mode X23/X22 | 2 / 3 | Outside the catalog's 0..1 range; range in the source is wrong (accessory not used). |

The other catalog conflicts resolve as follows: 40050/41026 unit m³/h (community) fits; 43005 unit "DM" fine;
47340 scale 1 vs 10 cannot be decided (reads 0); 47377 u8 vs s16 indistinguishable at 24 h.

## 4. Value ranking for an integration user

**Core telemetry (poll, graph, MQTT)** — all VERIFIED readable and plausible:
temperatures 40004 outdoor, 40067 outdoor average, 40008 supply, 40012 return, 40013 HW top, 40014 HW charge,
40025 exhaust air, 40026 extract air, 40033 room; refrigerant 40017 condenser, 40018 hot gas, 40019 liquid,
40020 evaporator, 40022 suction; 43009 calculated supply; 43005 degree minutes; 43136 compressor Hz;
43437 heating medium pump %; 40050 air flow; 40079/40081/40083 phase currents; 43084 add-heat power;
43086 priority (10 off, 20 HW, 30 heat); 45001 alarm; counters 41846/41848/41850 consumed energy, 44298/44300
heat incl. add, 44306/44308 heat compressor only, 43416 starts, 43420/43424 compressor hours, 43081/43239
add-heat hours. With a scale fix: 43141 compressor input power (kW).

Useful but slower-moving (could be read on a long interval if the engine ever supports two rates): 40185 1 h
outdoor average, 40195 room average, 43140 inverter temperature, 43065 flow dT, 43427/43431/43435 state codes,
40813 compressor slow-down reason, 40323/40324 inverter alarm/fault code, 43182/40321 target/request Hz.

**Settings worth showing** (read_write per source; all read fine): 47007 heat curve S1 (5), 47011 offset S1 (−2),
47015/47019 min/max supply S1 (20/40 °C), 47041 HW comfort mode (1 = normal), 47043–47049 HW start/stop
temperatures (normal 47/51 °C), 47050/47051 periodic HW (on, 14 days), 47137 operational mode (0 = auto),
47260 fan mode (0 = normal), 47261–47265 fan speeds (100/80/30/0, normal 55 %), 47275 filter reminder (3 months),
47300/47301 DOT −20 °C / ΔT 7, 47370/47371 allow add heat (0) / allow heating (1), 47375 stop heating at
16.0 °C, 47376 stop add heat −5.0 °C, 47387 HW production (1), 47394 use room sensor S1 (**0**), 47398 room
setpoint S1 (20.0), 47402 room sensor factor, 47212 max add power 6.5 kW, 47214 fuse 16 A, 48132 temporary lux,
48743 HW high power mode, 47020–47028 own curve points (only used with curve 0), 43001 software version.

**Noise**: everything in class (b), installer tuning (`setting-low`: compressor regulation, DM steps, fan return
times, HWC times, charge pump), and the service diagnostics (relays, inverter versions, defrost timers — 0 so far,
worth a look in winter).

### Recommended poll list

46 registers, measured cost 1.13 s each → **≈ 52 s per round** (today 30 → ≈ 34 s).

| Group | Registers |
|---|---|
| Keep (30) | 40004 40008 40012 40013 40014 40025 40026 40033 40067 41846 41848 41850 43005 43009 43084 43136 43416 43420 43424 43437 44298 44300 45001 47007 47011 47041 47265 47398 48132 48852 |
| myUplink graph list (12) | 40017 40018 40019 40020 40022 40050 40072 40079 40081 40083 44306 44308 |
| Add (4) | 43086 priority, 43141 compressor power (fix scale to 100/kW first), 43081 add-heat hours ("time factor" candidate), 47260 fan mode |

If ≈ 50 s is too slow: 40072 (constant, pending HP-34) and 43081 (changes only when the immersion heater runs) are
the first to drop (→ ≈ 50 s); 48852 could be read once at start-up if the decoder keeps the word order (needs a
code check; today it is learned from the poll). 47398 is only effective when 47394 = 1 (it is 0).

### Writable candidates for future supervised tests (do not test without the owner)

Already verified: 47011 heat offset, 45171 alarm reset (edge), 48132 temporary lux (HP-32 passed 15:37–15:39 today).
Shortlist, lowest risk first:
1. **47041 hot water comfort mode** (0 eco / 1 normal / 2 luxury) — myUplink "Hot water demand".
2. **47265 exhaust fan speed normal** (%) — directly visible as air flow 40050 within minutes.
3. **47260 fan mode** (0 normal, 1–4 speeds) — temporary fan boost, returns after 47271–47274 hours.
4. **47007 heat curve S1** (0–15) — slow effect via 43009.
5. **47375 stop temperature heating** (°C ×10) — summer/autumn switch-off.
6. 47050/47051 periodic HW (legionella) on/off/interval.

Do not expose: 47137 operational mode, 47370/47371 allow add heat/heating, 47212 max add power, 47214 fuse,
47206/48072 DM thresholds, compressor-frequency limits, 48852 word swap, anything for S2–S8 or accessories.

## 5. Open items from the myUplink list (coordinator request)

- **40017, 40018, 40019, 40020, 40022** (BT12/BT14/BT15/BT16/BT17): VERIFIED readable, moving, mutually
  consistent (§2). Poll them.
- **40050** BS1 air flow: VERIFIED, ~181, agrees with 41026/43124; unit m³/h (community). 40051 needs scale 10.
- **40072** BF1 flow: reads a constant 5.0 l/min → suspicious (class d), compare with the app.
- **40079/40081/40083** BE3/BE2/BE1: VERIFIED, move with compressor load (2.1–7.0 A).
- **44306/44308** heat meter compressor only: VERIFIED, 32461.2 / 35486.9 kWh, 44306 increments during HW,
  both below the "incl. add" counters 44298/44300 as expected.
- **Fan speed exhaust air**: no register in the catalog reports a live fan speed: 41256–41258 and 43108 are
  always 0 %, 48890 belongs to the OEK accessory. HYPOTHESIS: myUplink shows the effective setting, i.e.
  47265 (55 %) while 47260 fan mode = 0, else 47261–47264 for modes 1–4. The owner's display/app showed 55 %
  on 2026-10-24, which matches 47265. To confirm, set a temporary fan speed on the display and check that 47260
  and the app change together (HP-34).
- **Time factor add heat**: HYPOTHESIS 43081 "Tot. op.time add." = 1438.7 h (static; 43239 HW share 236.8 h).
  NIBE's "time factor" weights hours by electric step, so the app value may differ from 43081; compare (HP-34).
- **Hot water amount (~24 min)**: no catalog register matches (searched titles/descriptions for amount/minutes/
  hot water). HYPOTHESIS: computed by the firmware or myUplink from BT6/BT7 and comfort mode and not exported
  over MODBUS40. Not available to the gateway.
- **Hot water temperature (~50 °C)**: = **40013 BT7 HW top** (50.4–50.8 °C during the survey), already polled.
  40014 BT6 is the charging sensor (lower).
- **Current price (SE4)**: 41928 "Smart Price Adaption Price" reads 0.00, so the price itself is not mirrored;
  but smart price adaption looks active (41929 level 1, 44908 state 30, 44896 offset raw 1).

## 6. Surprises

1. Zero timeouts: firmware 9721 answers every catalog register, including systems S2–S8 and accessories that do
   not exist. "Readable" therefore says nothing about relevance; 396 of 576 are noise here.
2. 43141/43375 compressor power and 43147 compressor current are mis-scaled in the catalog (×100 and probably
   ×1000); 40051 too (×10 not ×100).
3. The immersion heater is disabled (47370 = 0) despite 1438.7 h of past add-heat operation.
4. The room sensor is not used for control (47394 = 0), so 47398 "room setpoint" has no effect today.
5. 24 h averages 42100/42101 return the sentinel; the 1 h average works.
6. Smart price adaption appears active (myUplink feature) and is visible in 41929/44896/44908.
7. Long probe sessions pause the poll loop almost completely.
