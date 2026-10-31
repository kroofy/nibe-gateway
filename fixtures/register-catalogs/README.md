# Register catalogs

| File | Model | Registers | Verification |
|---|---|---|---|
| `f730.json` | NIBE F730 | 581 | **all `unverified`**, `write_allowed: false` everywhere, `write_policy: "disabled"` |

Schema, provenance, cross-check results and regeneration: [docs/research/register-catalog.md](../../docs/research/register-catalog.md).
Decision: [ADR 0010](../../docs/decisions/0010-ws01-register-catalog-sources.md).

A register being listed here does **not** mean this pump supports it, and `access: read_write` is only what
the source claims. Per-installation verification results belong in a separate overlay, not in this file.

## Attribution
Titles, descriptions and value maps derive from NibePi `models/F730.json`
(https://github.com/anerdins/nibepi, MIT License, Copyright (c) 2019 Fredrik Anerdin), which itself mirrors
NIBE ModbusManager export data (NIBE Energy Systems). Community corrections from yozik04/nibe are referenced
as individual facts in `conflicts`/`notes` with source links. Value-map labels were translated from Swedish by WS01.

NIBE Energy Systems owns the underlying ModbusManager data; this project is not affiliated with or endorsed by
NIBE. The data is included, as it is in NibePi and yozik04/nibe, only so the gateway can describe the registers.
If you hold rights to it and want it removed, open an issue.
