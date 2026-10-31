# myUplink's user-facing parameters mapped to F730 registers (2026-10-25)

Source: the owner's screenshots of myUplink's "Select parameters" (graph) list for the F730 Cu 3x400V,
2026-10-25 15:40–15:42. myUplink's list is NIBE's own pick of what a household cares about, so it is a good
priority list for polling, history, MQTT and the UI. Mapping by catalog title/sensor designation (SOURCE);
`*` = in the current poll list; values not yet compared side by side with the app (HYPOTHESIS until done).

| myUplink parameter | Register | Unit/scale | Notes |
|---|---|---|---|
| Calculated supply climate system 1 | 43009* | °C ×0.1 | |
| Condenser (BT12) | 40017 | °C ×0.1 | "EB100-EP14-BT12 Condensor Out" |
| Current (BE1/BE2/BE3) | 40083 / 40081 / 40079 | A ×0.1 | Phase currents; only meaningful with current sensors fitted |
| Current compressor frequency | 43136* | Hz ×0.1 | Actual; 43182 target, 40321 request |
| Degree minutes | 43005* | ×0.1 | 16-bit; 40940 is the 32-bit variant |
| Discharge (BT14) | 40018 | °C ×0.1 | Hot gas |
| Evaporator (BT16) | 40020 | °C ×0.1 | |
| Exhaust air (AZ30-BT20) | 40025* | °C ×0.1 | |
| Extract air (AZ30-BT21) | 40026* | °C ×0.1 | |
| Fan speed exhaust air | unclear | % | 41256–41258/43108 read 0 % earlier; to be found by the survey |
| Heating medium pump speed (GP1) | 43437* | % | "Supply Pump Speed EP14" |
| Hot water charging (BT6) | 40014* | °C ×0.1 | |
| Hot water top (BT7) | 40013* | °C ×0.1 | |
| Liquid line (BT15) | 40019 | °C ×0.1 | |
| Number of compressor starts | 43416* | count | Counter |
| Oper. time | 43420* | h | Compressor total; counter |
| Oper. time hot water | 43424* | h | Counter |
| Outdoor temperature | 40004* | °C ×0.1 | 40067* average |
| Power internal add. heat | 43084* | kW ×0.01 | |
| Return line (BT3) | 40012* | °C ×0.1 | |
| Room temperature (BT50) | 40033* | °C ×0.1 | S1 |
| Suction gas (BT17) | 40022 | °C ×0.1 | |
| Supply line (BT2) | 40008* | °C ×0.1 | S1 |
| Time factor add heat | unclear | | Possibly 43081 "Tot. op.time add." (h ×0.1) — HYPOTHESIS |
| Value, air velocity sensor (BS1) | 40050 | ×0.1 | 41026 adjusted, 40051 unfiltered |
| Heat meter: Flow sensor (BF1) | 40072 | l/min ×0.1 | Only with a flow sensor fitted |
| Heat meter: Heating, compressor only | 44308 | kWh ×0.1, u32 | Counter |
| Heat meter: Heating, including int. add. heat | 44300* | kWh ×0.1, u32 | Counter |
| Heat meter: Hot water, compressor only | 44306 | kWh ×0.1, u32 | Counter |
| Heat meter: Hot water, including int. add. heat | 44298* | kWh ×0.1, u32 | Counter |
| Other: Current price (SE4) (excl. fees) | none local | | Cloud data from myUplink; 41928 "Smart Price Adaption Price" may mirror it — HYPOTHESIS |

Also user-facing in the app (owner, 2026-10-25): "Hot water boost" = 48132 Temporary Lux (write verified,
HP-32); "Hot water demand" economy/normal/luxury = 47041* (hypothesis by title); "hot water amount" (minutes)
and "hot water temperature" — registers to be identified (the survey should look for them).

Not yet polled but on myUplink's list: 40017, 40018, 40019, 40020, 40022, 40050, 40072, 40079, 40081, 40083,
44306, 44308 (12 registers, ~13 s more per poll round).
