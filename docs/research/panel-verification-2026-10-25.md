# F730 display photos vs gateway registers (2026-10-25)

Inputs (second set added below in §2 "Follow-up photos"): 16 photos of the pump's own display (`IMG_0023`–`IMG_0039`, no 0033), all of menu **3.1 SERVICEINFO**,
pages 1/16–16/16. EXIF `DateTimeOriginal` 16:01:15–16:03:29 with `OffsetTimeOriginal` **+01:00, i.e. CET**
(DST ended at 03:00 the same night; the survey times are CET too, so photo and survey use the same clock).
Gateway data: `docs/research/register-survey-2026-10-25.json` (15:29–15:53, re-reads 15:45/15:48/15:51), the live
poll list read from `GET /api/v1/registers/explorer` at 16:05:2x, and one read-only probe session of 32 IDs at
16:07:32–16:08:10 (`POST /api/v1/discovery/probe`). No `/controls/` call, no writes.

Labels: **VERIFIED** = the display and the register show the same value (or the same value within the drift of a
moving quantity, stated). **HYPOTHESIS** = interpretation not proven by these photos.

Pump state during the photos: hot-water charge in progress (priority "varmvatten", compressor 35 Hz, hot gas
91.9 °C). The charge had started before 15:45 and ended between 16:03:29 and 16:05:25 (43136 = 0 Hz, 43086 = 10 at
16:07). There is no history API on the gateway, so the poll values from exactly 16:01–16:03 are not retrievable;
temperatures are compared with the nearest readings (15:51 survey re-read and/or 16:05 poll) and judged by trend.

Display formatting observed: energy counters are **truncated** to whole kWh (35486.9 → "35486", 42242.6 → "42242"),
degree minutes are shown as an integer, sentinel −32768 is shown as "--".

## 1. Per photo: what the display shows

| Photo (CET) | Page | Heading | Label → value as shown |
|---|---|---|---|
| IMG_0023 16:01:15 | 1/16 | status, EB100 | driftprioritering → varmvatten; varmvatten laddning (BT6) → 50.8 °C; varmvatten topp (BT7) → 51.9 °C; beräknad framledning 1 → 24.0 °C; gradminuter → 1; utetemperatur (BT1) → 10.1 °C; värmebärarpumpshastighet (GP1) → 100 % |
| IMG_0024 16:01:51 | 2/16 | klimatsystem 1, EB100 | framledningstemp. (BT2) → 53.3 °C; kondensor retur (BT3) → 53.2 °C; beräknad framledning 1 → 24.0 °C; rumstemperatur 1 (BT50) → 21.1 °C; extern framledning (BT25) → -- °C; gradminuter → 1; extern justering → nej; golvtorksfunktion → från |
| IMG_0025 16:01:55 | 3/16 | EB100 (refrigerant/air temps) | hetgas (BT14) → 91.9 °C; vätskeledning (BT15) → 54.3 °C; förångare (BT16) → 1.8 °C; suggas (BT17) → 15.1 °C; kondensor fram (BT12) → 55.9 °C; kondensor retur (BT3) → 53.2 °C; frånluft (BT20) → 23.0 °C; avluft (BT21) → 4.4 °C |
| IMG_0026 16:01:58 | 4/16 | EB100 (ventilation) | värde, lufthastighetsgivare (BS1) → 180.3 (no unit); referens, lufthastighetsgivare → 180.7 (no unit); minskning från referensvärde → 0.2 %; fläkthastighet → 55 %; ventilation → normal |
| IMG_0027 16:02:37 | 5/16 | kompressor | aktuell kompressorfrekvens → 35 Hz; minsta tillåtna hastighet → 20 Hz; Inverter → 39.0 °C; dT Inverter - frånluft → 15.9 °C; 216 state → 1 |
| IMG_0028 16:02:40 | 6/16 | schemaläggning, EB100 | varmvatten → från; värme → från; ventilation → från; schema blockering → från; semesterinställning → från; SG Ready → inte använd |
| IMG_0029 16:02:43 | 7/16 | EB100 (electrical) | status intern tillsats → blockerad; max inställd eleffekt → 6.5 kW; säkringsstorlek → 16 A; ström L1 → 5.5 A; ström L2 → 3.8 A; ström L3 → 2.2 A; gradminuter → 1 |
| IMG_0030 16:02:45 | 8/16 | mjuka in-/utgångar, EB100-AA3 | AUX1–AUX5 → inte använd; X7 → larmutgång (1); SG Ready → inte använd |
| IMG_0031 16:02:52 | 9/16 | värmemängdsmätare BF1, EB100 | värme, endast kompr. → 35486 kWh; vv, endast kompr. → 32461 kWh; värme, inkl. int. tillsats → 42242 kWh; vv, inkl. int. tillsats → 33832 kWh; BF1 → 5.0 l/min |
| IMG_0032 16:02:55 | 10/16 | smart price adaption | status → 30; värme → 0.1; kyla → 0.0; pool → 0.0; varmvatten → 0 |
| IMG_0034 16:03:00 | 11/16 | modbus | adress → 1; version → 0 |
| IMG_0035 16:03:04 | 12/16 | internet | status → PROD 9:15:12; nätverkssladd → ja; ip inställt → ja; http-anslutning → ja; uppkopplad → ja; dhcp → till; proxy → från |
| IMG_0036 16:03:06 | 13/16 | internet, tcp/ip | ip-adress, nätmask, gateway, dns → the LAN's DHCP values (not recorded); mac → (shown, not recorded here) |
| IMG_0037 16:03:14 | 14/16 | QR service code, EB100 | QR code only |
| IMG_0038 16:03:26 | 15/16 | config EB100 | compressor → Mitsubishi 5118; inc. power → 3x400 V; int. add → 6.5 kW; heat med. pump → Wilo Yonos Para 7.5; fan → EBM G3G160-AC50; flow sensor → Huba DN20; inverter → NI1AB2.9; display type → 0 |
| IMG_0039 16:03:29 | 16/16 | EB100 | product → F730 Cu 3x400V; serial number → (shown, 14 digits); config changed → ja; display version → 9721R3; base version → 35; input version → 34; country code → SE; gitr → 235b9f4 |

Pages 12–14 (NIBE Uplink network status, QR) have no MODBUS40 counterpart and are not mapped (by design).

## 2. Display label → register → gateway value → verdict

Times are CET. "Survey" = 15:29–15:51 values; "poll" = 16:05 explorer; "probe" = 16:07–16:08.

### Counters and settings (slow-changing: compared directly)

| Display label (page) | Register | Gateway value (time) | Verdict |
|---|---|---|---|
| värme, endast kompr. 35486 kWh (9) | 44308 Heat Meter - Heat Cpr | 35486.9 kWh (15:34, 15:45–51, probe 16:07:36) | **MATCH** u32, kWh ×0.1 — VERIFIED |
| vv, endast kompr. 32461 kWh (9) | 44306 Heat Meter - HW Cpr | 32461.3 (15:51) → 32461.5 (16:07:34) | **MATCH** u32, kWh ×0.1 — VERIFIED |
| värme, inkl. int. tillsats 42242 kWh (9) | 44300 Heat Meter - Heat Cpr and Add | 42242.6 (survey, poll 16:05:54, 16:07:31) | **MATCH** u32, kWh ×0.1 — VERIFIED |
| vv, inkl. int. tillsats 33832 kWh (9) | 44298 Heat Meter - HW Cpr and Add | 33832.5 (15:51) → 33832.7 (16:05:53) | **MATCH** u32, kWh ×0.1 — VERIFIED |
| BF1 5.0 l/min (9) | 40072 BF1 EP14 Flow | 5.0 l/min (every read; probe 16:07:52) | **MATCH** s16, l/min ×0.1 — VERIFIED that the gateway reads what the pump uses. Whether 5.0 is a measured flow is still open (constant across modes and pump speeds; HYPOTHESIS: fixed/default value) |
| gradminuter 1 (1, 2, 7) | 43005 Degree Minutes (16 bit) | raw 14 → 1.4, static 15:41–15:51 (DM is frozen during the HW charge that was still running at 16:02:43); after the charge: 14.0 (16:05:44), 19.3 (16:07:37) | **MATCH** s16, ×0.1 (display shows the integer part) — VERIFIED with the caveat that no read was taken in the same minute as the photo |
| gradminuter 1 (1, 2, 7) | 40940 Degree Minutes (32 bit) | −0.1 / 0.0 / 0.0 / 0.0 (survey), 0.0 at 16:07:38 while 43005 = 19.3 | **DIFFERENT MEANING / not maintained** on 9721 — VERIFIED: the display follows 43005, 40940 stays ~0 |
| max inställd eleffekt 6.5 kW (7) | 47212 Max int add. power | raw 650 → 6.5 kW (probe 16:08:04) | **MATCH** s16, kW ×0.01 — VERIFIED |
| säkringsstorlek 16 A (7) | 47214 Fuse | 16 A (16:08:05) | **MATCH** u16, A ×1 — VERIFIED |
| minsta tillåtna hastighet 20 Hz (5) | 43122 Compr. current min.freq. | 20 Hz (16:07:41) | **MATCH** ×1 — VERIFIED (47103 "Min comp freq" setting also 20; 43122 is the live limit) |
| 216 state 1 (5) | 32260 NIBE Inverter 216-state | 1 (15:29, 16:07:45) | **MATCH** u8 ×1, no label on the display — VERIFIED |
| fläkthastighet 55 % (4) | 47265 Exhaust Fan speed normal | 55 % (poll 16:05:26) | **MATCH** — VERIFIED for mode normal |
| ventilation normal (4) | 47260 Fan Mode | 0 (survey, probe 16:08:02) | **MATCH**, value label 0 = "normal" — VERIFIED |
| fläkthastighet 55 % (4) | 41256/41257/41258/43108 Fan speed current | 0 % always | **DIFFERENT MEANING** (not the live fan speed on this unit) — VERIFIED |
| smart price adaption status 30 (10) | 44908 State smart price adaption | 30 (15:34, 16:07:51) | **MATCH** u8 ×1, shown as a bare number — VERIFIED |
| smart price adaption värme 0.1 (10) | 44896 SPA Heating Offset | raw 1 → 0.1 (15:34, 16:07:46) | **MATCH** s8, ×0.1 — VERIFIED. **The survey's suggested correction to scale 1 is wrong**; keep the catalog's scale 10 |
| smart price adaption varmvatten 0 (10) | 44897 SPA HW Comfort Mode | 0 (16:07:47) | **MATCH** ×1 — VERIFIED (value only 0) |
| smart price adaption kyla 0.0 (10) | 44899 SPA Cool Offset | 0 (catalog scale 1) | value **MATCH**; the display's one decimal suggests ×0.1 like 44896 — scale HYPOTHESIS (0 cannot prove it) |
| smart price adaption pool 0.0 (10) | 44898 SPA Pool Offset | 0 (catalog scale 1) | as 44899: value MATCH, scale ×0.1 HYPOTHESIS |
| driftprioritering varmvatten (1) | 43086 Prio | 20 (15:45/15:48/15:51); 10 at 16:07:39 after the charge ended | **MATCH**, value label 20 = "varmvatten" (Hot water) — VERIFIED by mode (no same-minute read) |
| extern justering nej (2) | 43161 External adjustment activated via input S1 | 0 (16:08:06) | **MATCH**, 0 = "nej" — value consistent; HYPOTHESIS that this is the source register (only value 0 seen) |
| golvtorksfunktion från (2) | 47276 Floor drying | 0 (16:08:07) | **MATCH**, 0 = "från" — as above, HYPOTHESIS on source |
| semesterinställning från (6) | 48043 Holiday - Activated | 0 (16:08:08) | **MATCH**, 0 = "från" — HYPOTHESIS on source |
| SG Ready inte använd (6, 8) | 44874 State SG Ready | 50 (15:34, 16:08:09) | **HYPOTHESIS**: 50 may be the "not used / normal" state; the display line more likely reflects the AUX input configuration (no register found) |
| status intern tillsats blockerad (7) | 43091 Int. el.add. State | 0 (16:07:40); 47370 Allow Additive Heating = 0 (survey) | **HYPOTHESIS**: "blockerad" follows from 47370 = 0; whether 43091 = 0 means "blocked" or "off" is not provable from one value |
| display version 9721R3 (16) | 43001 Software version | 9721 | **MATCH** (numeric part) — VERIFIED |
| product F730 Cu 3x400V (16) | 0x6D product info | "F730 Cu 3x400V" | **MATCH** — VERIFIED |

### Moving quantities (photo 16:01–16:03 vs nearest gateway reads)

| Display label (page, time) | Register | Gateway value (time) | Verdict |
|---|---|---|---|
| utetemperatur (BT1) 10.1 °C (1, 16:01:15) | 40004 | 10.1 (poll 16:05:31) | **MATCH** ×0.1 °C — VERIFIED (4 min apart, equal) |
| beräknad framledning 1 24.0 °C (1, 2) | 43009 | 24.0 (survey 15:41, poll 16:05:45) | **MATCH** ×0.1 °C — VERIFIED |
| rumstemperatur 1 (BT50) 21.1 °C (2, 16:01:51) | 40033 | 21.1 (poll 16:05:39) | **MATCH** ×0.1 °C — VERIFIED |
| värmebärarpumpshastighet (GP1) 100 % (1) | 43437 | 100 % (survey 15:51, poll 16:05:52) | **MATCH** ×1 % — VERIFIED |
| varmvatten laddning (BT6) 50.8 °C (1, 16:01:15) | 40014 | 48.7 (15:51) → 51.4 (16:05:36) | **MATCH** ×0.1 °C — between the two reads, consistent |
| varmvatten topp (BT7) 51.9 °C (1, 16:01:15) | 40013 | 50.8 (15:51) → 52.2 (16:05:34) | **MATCH** ×0.1 °C — consistent |
| frånluft (BT20) 23.0 °C (3, 16:01:55) | 40025 | 23.0 (survey), 23.1 (16:05:37) | **MATCH** ×0.1 °C — VERIFIED |
| avluft (BT21) 4.4 °C (3) | 40026 | 4.7 (15:51, compressor running); 14.6 at 16:05 after stop | **MATCH** ×0.1 °C — consistent with trend |
| framledningstemp. (BT2) 53.3 °C (2, 16:01:51) | 40008 | 51.3 (15:51); 29.2 (16:05:32, charge over) | **MATCH** ×0.1 °C — consistent with trend (rising during charge) |
| kondensor retur (BT3) 53.2 °C (2, 3) | 40012 | 50.9 (15:51); 22.6 (16:05:33) | **MATCH** ×0.1 °C — consistent with trend |
| hetgas (BT14) 91.9 °C (3) | 40018 | 64.7 → 74.0 → 80.3 (15:45–15:51, rising) | **MATCH** ×0.1 °C — trend-consistent, 10 min gap |
| vätskeledning (BT15) 54.3 °C (3) | 40019 | 50.2 → 52.4 (15:45–15:51) | **MATCH** ×0.1 °C — trend-consistent |
| förångare (BT16) 1.8 °C (3) | 40020 | 2.1–2.7 (15:45–15:51) | **MATCH** ×0.1 °C — trend-consistent |
| suggas (BT17) 15.1 °C (3) | 40022 | 18.5 → 15.5 (15:45–15:51, falling) | **MATCH** ×0.1 °C — trend-consistent |
| kondensor fram (BT12) 55.9 °C (3) | 40017 | 51.3 → 53.7 (15:45–15:51) | **MATCH** ×0.1 °C — trend-consistent |
| extern framledning (BT25) "--" (2) | 40071 | −32768 sentinel (survey) | **MATCH**: sentinel = not connected ("--") — VERIFIED |
| aktuell kompressorfrekvens 35 Hz (5, 16:02:37) | 43136 | 35.0 (15:45–15:51); 0.0 at 16:05:48 after the charge | **MATCH** ×0.1 Hz — VERIFIED by mode |
| Inverter 39.0 °C (5, 16:02:37) | 43140 Inverter Temperature | 32.7 → 35.2 → 36.5 (15:45–15:51, rising); 30.3 at 16:07:44 (stopped) | **MATCH** s16 ×0.1 °C — trend-consistent |
| dT Inverter - frånluft 15.9 °C (5) | none | = 39.0 − 23.1 (43140 − 40025) | **NOT FOUND** — computed by the display; derivable |
| värde, lufthastighetsgivare (BS1) 180.3 (4, 16:01:58) | 40050 EB100-BS1 Air flow | 180.4–181.8 (15:29–15:51); 178.5 (16:07:43) | **MATCH** ×0.1 — VERIFIED. The display shows **no unit**; "m³/h" (community) stays HYPOTHESIS |
| referens, lufthastighetsgivare 180.7 (4) | 43124 Airflow ref. | 180.7 (15:48, 15:51); sentinel again at 16:07:42 | **MATCH** ×0.1, unitless — VERIFIED. Transient sentinel reproduced (treat as gap) |
| minskning från referensvärde 0.2 % (4) | none | (180.7 − 180.3) / 180.7 = 0.22 % | **NOT FOUND** — computed; derivable from 40050/43124 |
| ström L1 5.5 A (7, 16:02:43) | 40083 EB100-BE1 Current | 5.8/5.7/5.6 (15:45–15:51); 5.6 (16:07:55) | **MATCH** u32 ×0.1 A, L1 = BE1 — VERIFIED |
| ström L2 3.8 A (7) | 40081 EB100-BE2 Current | 4.0/4.0/4.4 (15:45–15:51); 0.7 (16:07:56, compressor stopped) | **MATCH** u32 ×0.1 A, L2 = BE2 — VERIFIED |
| ström L3 2.2 A (7) | 40079 EB100-BE3 Current | 2.2/2.1/2.2 (15:45–15:51); 2.2 (16:07:57) | **MATCH** u32 ×0.1 A, L3 = BE3 — VERIFIED |

### Shown on the display, no register in the catalog (NOT FOUND)

- Schedules (6/16): varmvatten / värme / ventilation schemaläggning, schema blockering — all "från".
- Soft inputs/outputs (8/16): AUX1–AUX5 "inte använd", X7 "larmutgång (1)", SG Ready "inte använd".
- modbus adress 1, version 0 (11/16).
- Internet/tcp-ip status (12–13/16) — the pump's own NIBE Uplink link (pump on the LAN by DHCP).
- Config (15/16): compressor Mitsubishi 5118, supply 3x400 V, int. add 6.5 kW (same number as 47212), pump Wilo
  Yonos Para 7.5, fan EBM G3G160-AC50, flow sensor Huba DN20, inverter NI1AB2.9, display type 0.
- Identity (16/16): serial number (the gateway reports `serial: null`), config changed ja, base version 35, input
  version 34, country SE, gitr 235b9f4. None of the inverter version registers 40327–40332 (46/18/2049/1304/9/289)
  equals 35 or 34.
- "dT Inverter - frånluft" and "minskning från referensvärde": computed values (see above).

### Follow-up photos 16:21:49–16:23:09 CET (second set, `IMG_0040`–`IMG_0044`)

Compressor stopped throughout (3.2 "status: från"; 43136 = 0 Hz, 43141 = 0 at 16:26). Gateway 0.1.13 values from
`GET /api/v1/device` (10-min group read at 16:17:12–16:17:53, fast group at 16:26) and one read-only probe of 14 IDs
at 16:26:40–16:27:08 (43147 43375 47043–47049 48743 43091 43427 43435 47370).

| Photo (CET) | Menu | Label → value as shown |
|---|---|---|
| IMG_0040 16:21:49 | 3.2 KOMPRESSORINFO | status → från; antal starter → 32325; total drifttid → 46953 h; - av vilken varmvatten → 14054 h |
| IMG_0041 16:22:02 | 3.3 TILLSATSINFO | status → blockerad; tidfaktor → 1438.7 (no unit); - av vilken varmvatten → 236.8 (no unit) |
| IMG_0042 16:22:22 | 1.9.1.1 VÄRMEKURVA | curve → 5; offset (thermometer icon) → −2; cursor at utetemp 0 °C → framledningstemp 29 °C (curve read-out, not a live value) |
| IMG_0043 16:23:04 | 5.1.1 VARMVATTENINST. | starttemp. ekonomi 43 °C; stopptemp. ekonomi 47 °C; starttemp. normal 47 °C; stopptemp. normal 51 °C; starttemp. lyx 50 °C; stopptemp. lyx 54 °C |
| IMG_0044 16:23:09 | 5.1.1 (scrolled) | starttemp. normal 47; stopptemp. normal 51; starttemp. lyx 50; stopptemp. lyx 54; stopptemp. per. höjning 55 °C; hög effekt → radio circle (state not readable from the photo) |

| Display label | Register | Gateway value (time) | Verdict |
|---|---|---|---|
| antal starter 32325 | 43416 | 32325 (16:17:27) | **MATCH** s32, count ×1 — VERIFIED |
| total drifttid 46953 h | 43420 | 46953 h (16:17:30) | **MATCH** s32, h ×1 — VERIFIED |
| - av vilken varmvatten 14054 h | 43424 | 14054 h (16:17:32) | **MATCH** s32, h ×1 — VERIFIED |
| tidfaktor 1438.7 | 43081 "Tot. op.time add." | raw 14387 → 1438.7 (16:17:23) | **MATCH** s32, ×0.1 — VERIFIED. Meaning is NIBE's *time factor* (hours weighted by electric step), not plain hours: the display shows no unit; catalog unit "h" should read "time factor" (unitless / weighted h) |
| - av vilken varmvatten 236.8 | 43239 "Tot. HW op.time add." | raw 2368 → 236.8 (16:17:25) | **MATCH** s32, ×0.1 — VERIFIED; same unit remark as 43081 |
| tillsats status blockerad | 43091 = 0, 47370 = 0 (16:27) | | consistent with page 7/16; which register drives "blockerad" — HYPOTHESIS (47370 = 0 most likely) |
| kompressor status från | 43427 = 20, 43435 = 0 (16:27); 60 / 1 while running at 15:45–15:51 | | value labels **HYPOTHESIS**: 43435 0 = off / 1 = running; 43427 20 = stopped, 60 = running |
| värmekurva 5 | 47007 Heat Curve S1 | 5 (16:17:43) | **MATCH** s8 ×1 — VERIFIED |
| förskjutning −2 | 47011 Heat Offset S1 | −2 (16:17:45) | **MATCH** s8 ×1 — VERIFIED |
| starttemp. ekonomi 43 °C | 47045 | raw 430 → 43.0 (16:26:50) | **MATCH** s16, °C ×0.1 — VERIFIED |
| stopptemp. ekonomi 47 °C | 47049 | raw 470 → 47.0 (16:26:58) | **MATCH** s16, °C ×0.1 — VERIFIED |
| starttemp. normal 47 °C | 47044 | raw 470 → 47.0 (16:26:47) | **MATCH** s16, °C ×0.1 — VERIFIED |
| stopptemp. normal 51 °C | 47048 | raw 510 → 51.0 (16:26:56) | **MATCH** s16, °C ×0.1 — VERIFIED |
| starttemp. lyx 50 °C | 47043 | raw 500 → 50.0 (16:26:45) | **MATCH** s16, °C ×0.1 — VERIFIED |
| stopptemp. lyx 54 °C | 47047 | raw 540 → 54.0 (16:26:54) | **MATCH** s16, °C ×0.1 — VERIFIED |
| stopptemp. per. höjning 55 °C | 47046 | raw 550 → 55.0 (16:26:52) | **MATCH** s16, °C ×0.1 — VERIFIED |
| hög effekt (radio circle) | 48743 Hot water high power mode | 0 (16:27:00) | **HYPOTHESIS**: circle state not distinguishable in the photo |

Order note: the register order is 47043 lux start, 47044 normal start, 47045 eco start, 47046 periodic stop,
47047 lux stop, 47048 normal stop, 47049 eco stop. All values are distinct (starts 43/47/50, stops 47/51/54/55) except
47044 (start normal) and 47049 (stop eco), which both read 47.0; for that pair the photo confirms the scale but the
assignment rests on the catalog titles (HYPOTHESIS for that pair only).

### Still not on any photographed screen (UNRESOLVED by display)

| Item | Registers | Status |
|---|---|---|
| Compressor input power | 43141 / 43375 | No F730 screen photographed so far shows it (3.1 pages 1–16, 3.2, 3.3). Cross-register evidence below |
| Compressor current | 43147 | Not shown anywhere photographed (3.1 page 7 shows only L1–L3 phase currents) |
| Consumed energy | 41846 (1972.9), 41848 (10189.1), 41850 (19969.1) kWh | Not on 3.1/3.2/3.3; myUplink energy log is the remaining place to compare |
| Fan modes 1–4 | 47260, 47261–47264 | needs a temporary fan speed set in menu 1.2 |

Cross-register evidence for 43141 (HYPOTHESIS, strong):
- **Energy counter**: 41848 "consumed energy due to hot water" went 10188.8 (15:40) → 10188.9 (15:51) → 10189.1
  (16:05) over a hot-water charge of ≤ 25 min with add heat blocked, i.e. 0.3 ± 0.1 kWh → 0.5–1.0 kW average.
  43141 raw 76–79 during that charge → **0.76–0.79 kW at ×0.01 kW** fits; ×1 W (0.08 kW) and ×0.1 kW (7.8 kW) do not.
- **Phase current**: L2 (40081) is 0.7 A with the compressor stopped and 4.0–4.4 A at 35 Hz, a delta of ~3.4 A.
  0.78 kW / 230 V = 3.4 A, so the compressor inverter appears to sit on L2 and its input power is ~0.78 kW.
  (L1 stays ~5.6–6.4 A whether the compressor runs or not.)
- 43375 "mean" read the same raw values as 43141 every time → same unit and scale.

43147 (HYPOTHESIS, unresolved): raw 3104 at 20 Hz and 5125–5355 at 35 Hz; as mA that is 3.1 / 5.1–5.4 A, which
is **not** the L2 input-current delta (~3.4 A). It may be the inverter's output (motor phase) current, which can
exceed the input current at part load; a different unit (e.g. 0.1 W or rpm-like) cannot be excluded. Nothing on
the display shows it.

41846/41848/41850 (HYPOTHESIS, consistent): kWh ×0.1 makes the HW charge increment above physically consistent
(≈ 0.3 kWh) and gives lifetime ratios heat/consumed of 3.3 (HW) and 2.1 (heating incl. add); not shown on the
display.

## 3. Value labels revealed by the display

| Register | Raw | Display text |
|---|---|---|
| 43086 Prio | 20 | "varmvatten" (hot water) — VERIFIED (catalog label "Hot water" correct) |
| 47260 Fan Mode | 0 | "normal" — VERIFIED (catalog label "Normal" correct) |
| 43161 Ext. adjustment via input S1 | 0 | "nej" — HYPOTHESIS on source |
| 47276 Floor drying | 0 | "från" — HYPOTHESIS on source |
| 48043 Holiday - Activated | 0 | "från" — HYPOTHESIS on source |
| 44908 State smart price adaption | 30 | shown as "30" (no text label) |
| 32260 Inverter 216-state | 1 | shown as "1" (no text label) |
| 40071 BT25 (any temperature) | −32768 | "--" — VERIFIED |
| 43435 Cpr Status EP14 | 0 / 1 | 3.2 "status: från" while 0; 1 seen while running — HYPOTHESIS 0 = off, 1 = on |
| 43427 Compressor State EP14 | 20 / 60 | 20 while 3.2 shows "från"; 60 while running at 35 Hz — HYPOTHESIS |
| 43091 Int. el.add. State (with 47370 = 0) | 0 | 3.3 "status: blockerad" — HYPOTHESIS on which register drives it |

## 4. Corrections and confirmations for the catalog owner (no catalog edits made here)

- **44896 Smart Price Adaption Heating Offset: s8, ×0.1 (catalog scale 10) — keep. VERIFIED** (display 0.1 for
  raw 1). The survey's "scale 1 more likely" is withdrawn.
- 44898 / 44899 SPA pool/cool offset: catalog scale 1; display formats them with one decimal → probably ×0.1 like
  44896. HYPOTHESIS.
- 40940 Degree Minutes (32 bit): not the value the display shows; use 43005. VERIFIED.
- 43005: s16, ×0.1, unit DM. VERIFIED (display shows the integer part).
- 40050 / 43124: ×0.1 VERIFIED; the display shows no unit (m³/h is still only a community claim).
- 40072: the display shows the same 5.0 l/min, so it is not a gateway decode error; meaning (measured or default)
  remains open. The heat-meter page is titled "värmemängdsmätare BF1", so the pump's kWh counters are tied to BF1.
- 43081 / 43239: s32, ×0.1 VERIFIED; the value is NIBE's add-heat **time factor** (display "tidfaktor", no
  unit), so the catalog unit "h" is misleading — suggest unit none (or "h-equivalent"), title "time factor".
- 43416 (count ×1), 43420 / 43424 (h ×1), 47007 / 47011 (×1), 47043–47049 (°C ×0.1): VERIFIED, no change.
- 43141 / 43375: kW ×0.01 (catalog has W ×1) — HYPOTHESIS, strong (energy-counter and L2 cross-checks); not on
  any display screen.
- 43147: unresolved; ×0.001 A (mA) remains a HYPOTHESIS and does not match the L2 input-current delta.
- 41846 / 41848 / 41850: kWh ×0.1 — HYPOTHESIS (consistent cross-check), not on the display.
- No other scale corrections: every compared register already matches the catalog scale.

## Fan and airflow watch, 2026-10-26 08:44–09:23 (read-only probes every ~45 s, no fan-speed change made)

VERIFIED by observation on the owner's F730 (no setting changed during the window):
- Fan settings: 47260 mode 0 (normal); speeds 47265 normal 55 %, 47264 speed 1 **0 %** (stops ventilation),
  47263 speed 2 30 %, 47262 speed 3 80 %, 47261 speed 4 100 %; return times 47271–47274 4 h each; 48890 100 %.
- 41256, 41257, 41258, 43108 ("fan speed current") stayed 0 the whole time → not maintained on firmware 9721.
- One frost/defrost cycle at constant fan setting: airflow 40050 fell 181 → 167 while BT21 40026 (air after the
  evaporator) cooled −9.0 → −13.1 °C (frost restricting airflow); at 09:10 40026 jumped to −7.2, −1.9, −0.3 °C
  and airflow recovered to 184 (defrost); then compressor off (40026 up to +10.5 °C), restart at 09:18 (+9.0 → 0.4).
  40050/41026 therefore also indicate frost and filter state, not only fan speed.
- 43124 "Airflow ref." returns the integer-limit sentinel while the compressor is off and a value (186–188) while
  it runs — explains the one sentinel seen in the survey. Not a fault.
- 40051 (unfiltered airflow) reads ~17.5–18.8 with the catalog scale: confirms the survey's ×10 scale error.
Still open (HP-34 a): the response to modes 1–4 — needs a temporary fan-speed change in menu 1.2 during a watch.

## Fan-speed test, 2026-10-26 19:24–19:28 (owner set a temporary speed 3 in myUplink, then back to normal)

VERIFIED (read-only probes while the owner changed the setting; compressor off throughout):
| Register | Normal (55 %) | Speed 3 (80 %) | Back to normal |
|---|---:|---:|---:|
| 47260 fan mode | 0 | 3 | 0 (within one pass) |
| 43108 "fan speed current" | 0 | **3** | 0 |
| 40050 BS1 airflow | 210.7 | 229.5 → 298.1 → 293 (settled) | 240 and falling |
| 40051 unfiltered airflow (catalog scale ÷100, should be ÷10) | 19.3 | 30.4 / 29.0 | 20.4 |
| 41026 adjusted airflow (slow filter) | 193.8 | 204 → 214 (still rising) | 221 (lagging) |
| 41256 / 41257 / 41258 | 0 | 0 | 0 |

Conclusions:
- 43108 is the **active fan speed step** (0 = normal, 1–4 = temporary speeds), not a percentage and not dead —
  this corrects the morning watch, which saw it only at 0. The catalog's "%" unit is wrong for it.
- 47260 reports the selected mode immediately; 40050 follows the fan proportionally (~3.7–3.8 per % of fan speed:
  210.7 at 55 %, 298 at 80 %), i.e. it is the practical "actual fan output" reading.
- 41256–41258 stayed 0 at every speed: not maintained on firmware 9721.
HP-34 (a) closed for speed 3; speeds 1, 2 and 4 behave the same way per the catalog (not each tested).
