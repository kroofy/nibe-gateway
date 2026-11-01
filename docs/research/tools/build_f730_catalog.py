#!/usr/bin/env python3
"""Build fixtures/register-catalogs/f730.json from upstream register lists (stdlib only).

Inputs (clone them to /tmp, never into this repo):
    git clone --depth 1 https://github.com/anerdins/nibepi /tmp/nibepi        # MIT
    git clone --depth 1 https://github.com/yozik04/nibe /tmp/nibe-py          # GPL-3.0 (facts only)

Usage:
    python3 docs/research/tools/build_f730_catalog.py \
        --nibepi /tmp/nibepi/models/F730.json \
        --mm-csv /tmp/nibe-py/nibe/data/f730.csv \
        --ext /tmp/nibe-py/nibe/data/extensions.json \
        --out fixtures/register-catalogs/f730.json

Base record = NibePi models/F730.json (itself a copy of NIBE ModbusManager export DB 8310).
Each record is cross-checked field by field against the raw ModbusManager CSV; disagreements and
community corrections (yozik04/nibe extensions.json) are recorded under "conflicts", never silently
applied. Everything is verification="unverified" and write_allowed=false.
See docs/research/register-catalog.md for the schema.
"""
from __future__ import annotations

import argparse
import csv
import json
import re
from functools import reduce
from pathlib import Path

NIBEPI_COMMIT = "1aa36b02986d88778d765be738c68fb2bcabdbea"
NIBEPY_COMMIT = "00f4ae88142edd4bc43da38b92b0b47b7c519930"

SOURCES = {
    "nibepi-f730": {
        "description": "anerdins/nibepi models/F730.json",
        "url": f"https://github.com/anerdins/nibepi/blob/{NIBEPI_COMMIT}/models/F730.json",
        "license": "MIT, Copyright (c) 2019 Fredrik Anerdin",
        "note": "Used by the owner's running NibePi installation (same file family). Field 'titel' = title.",
    },
    "nibe-modbusmanager-f730-db8310": {
        "description": "NIBE ModbusManager 1.0.9 CSV export, 'Product: F730', 'Database: 8310', dated 20200624",
        "url": f"https://github.com/yozik04/nibe/blob/{NIBEPY_COMMIT}/nibe/data/f730.csv",
        "license": "Manufacturer export redistributed in yozik04/nibe; redistribution terms unclear",
        "note": "Only used to cross-check fields; titles/descriptions taken from nibepi-f730.",
    },
    "yozik04-nibe-extensions": {
        "description": "yozik04/nibe community corrections nibe/data/extensions.json",
        "url": f"https://github.com/yozik04/nibe/blob/{NIBEPY_COMMIT}/nibe/data/extensions.json",
        "license": "GPL-3.0 (only individual facts referenced, no file copied)",
        "note": "Community corrections; recorded as conflicts / unverified additions.",
    },
}

SV_EN = {
    "Inga larm": "No alarm",
    "Låg frånluftstemperatur": "Low exhaust air temperature",
    "Misslyckad avfrostning": "Defrosting failed",
    "Frysskydd": "Frost protection",
    "Misslyckad periodisk höjning": "Periodic increase failed",
    "Effektvakt aktiverad": "Load monitor activated",
    "Avfrostning": "Defrosting",
    "Filterlarm": "Filter alarm",
    "Kom.avb Modbus": "Communication interrupted (Modbus)",
    "Kyla": "Cooling",
    "Överföring": "Transfer",
    "Pool 2": "Pool 2",
    "Pool": "Pool",
    "Varmvatten": "Hot water",
    "Värme": "Heat",
    "Från": "Off",
    "Normal": "Normal",
    "Hastighet 1": "Speed 1",
    "Hastighet 2": "Speed 2",
    "Hastighet 3": "Speed 3",
    "Hastighet 4": "Speed 4",
}

# Registers with special safety meaning (see docs/research/nibepi-analysis.md and protocol.md)
SPECIAL = {
    45171: {"flags": ["alarm_reset", "nibepi_writes_on_startup", "never_write"],
            "note": "Writing 1 resets alarms. NibePi writes it on every start and whenever 45001==251."},
    48852: {"flags": ["protocol_control", "nibepi_writes_on_startup", "never_write"],
            "note": "MODBUS40 word swap; changes 32-bit word order in read responses. NibePi writes 0 on every start. Read it, never write it."},
    48889: {"flags": ["protocol_control", "never_write"],
            "note": "MODBUS40 disable LOG.SET (USB stick register selection for data messages). Never write."},
    45001: {"flags": ["alarm"], "note": "Most severe current alarm. 251 = communication interrupted (Modbus), i.e. no accessory answering."},
}

# Unit/scale corrections from owner hardware checks (docs/research/panel-verification-2026-10-25.md).
# scale is the catalog divisor; None keeps the source value.
UNIT_SCALE_OVERRIDES = {
    43141: ("kW", 100, "Owner verification 2026-10-25: W ×1 gave 76 W at 35 Hz; kW ×0.01 (0.76–0.79 kW) matches the energy counter and L2 current rise (HYPOTHESIS, strongly supported; docs/research/panel-verification-2026-10-25.md)."),
    43375: ("kW", 100, "Same raw value as 43141; kW ×0.01 (see 43141)."),
    43081: (None, None, "Display 3.3 'tidfaktor' 1438.7 = raw ×0.1 (VERIFIED 2026-10-25); NIBE's add-heat time factor, not hours, so no unit."),
    43239: (None, None, "Display 3.3 'av vilken varmvatten' 236.8 = raw ×0.1 (VERIFIED 2026-10-25); time factor for hot water, no unit."),
    40051: (None, 10, "Owner verification 2026-10-25/26: scale ÷100 gave 17.5–18.9 beside 40050 at 180; ÷10 (community value) matches 40050 and follows the fan (docs/research/panel-verification-2026-10-25.md, fan sections)."),
    43108: (None, None, "active fan speed step: 0 = normal, 1–4 = temporary speed (owner test 2026-10-26); not a percentage, so no unit (docs/research/panel-verification-2026-10-25.md, fan-speed test)."),
}

# Raw ranges the sources leave out, taken from the register's own enum description.
RANGE_OVERRIDES = {
    48132: (0, 4, "Range 0..=4 from the description (0=Off, 1=3h, 2=6h, 3=12h, 4=One time increase); the sources give none."),
}


def xor8(b):
    return reduce(lambda a, c: a ^ c, b, 0)


def slug(title: str, rid: int) -> str:
    s = title.lower()
    s = s.replace("°", "").replace("+", "plus-").replace("%", "pct")
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")
    return f"{s}-{rid}" if s else f"register-{rid}"


def num(v):
    if v is None or v == "":
        return None
    f = float(v)
    return int(f) if f.is_integer() else f


def load_mm_csv(path: Path) -> dict[int, dict]:
    lines = path.read_text(encoding="latin-1").splitlines()
    assert lines[2].startswith("Product: F730") and lines[3].startswith("Database: 8310"), lines[:4]
    out = {}
    for r in csv.reader(lines[5:], delimiter=";"):
        if len(r) < 10:
            continue
        out[int(r[2])] = {"title": r[0], "info": r[1], "unit": r[3], "size": r[4], "factor": int(r[5]),
                          "min": num(r[6]), "max": num(r[7]), "default": num(r[8]), "mode": r[9]}
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--nibepi", required=True)
    ap.add_argument("--mm-csv", required=True)
    ap.add_argument("--ext", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    nibepi = {int(r["register"]): r for r in json.loads(Path(a.nibepi).read_text())}
    mm = load_mm_csv(Path(a.mm_csv))
    ext: dict[int, dict] = {}
    for block in json.loads(Path(a.ext).read_text()):
        if "f730.json" in block["files"]:
            for k, v in block["data"].items():
                ext.setdefault(int(k), {}).update(v)

    ids = sorted(set(nibepi) | set(mm) | {k for k, v in ext.items() if "size" in v})
    regs = []
    for rid in ids:
        np_ = nibepi.get(rid)
        m = mm.get(rid)
        e = ext.get(rid, {})
        sources, conflicts, flags, notes = [], [], [], []
        if np_:
            sources.append("nibepi-f730")
        if m:
            sources.append("nibe-modbusmanager-f730-db8310")
        if np_ is None and m is None:
            sources.append("yozik04-nibe-extensions")
            notes.append("Not in the F730 ModbusManager export nor in NibePi; known only from community data.")
        base = np_ or {}
        title = base.get("titel") or (m and m["title"]) or e.get("title")
        desc = base.get("info") if np_ else (m["info"] if m else e.get("info", ""))
        size = base.get("size") or (m and m["size"]) or e.get("size")
        factor = int(base.get("factor") or (m and m["factor"]) or e.get("factor") or 1)
        mode = base.get("mode") or (m and m["mode"]) or ("R/W" if e.get("write") else "R")
        unit = (base.get("unit") if np_ else (m["unit"] if m else e.get("unit"))) or ""
        unit = unit.strip()
        if unit in ("", "1"):
            if unit == "1":
                notes.append("Source unit '1' (dimensionless) normalised to null.")
            unit = None
        rmin = num(base.get("min")) if np_ else (m["min"] if m else num(e.get("min")))
        rmax = num(base.get("max")) if np_ else (m["max"] if m else num(e.get("max")))
        if rmin == 0 and rmax == 0:
            rmin = rmax = None  # 0/0 means "no range given" in the ModbusManager export
        if rid in UNIT_SCALE_OVERRIDES:
            unit, new_factor, note = UNIT_SCALE_OVERRIDES[rid]
            if new_factor is not None:
                factor = new_factor
            notes.append(note)
        if rid in RANGE_OVERRIDES:
            rmin, rmax, note = RANGE_OVERRIDES[rid]
            notes.append(note)
        rdef = None
        if m and m["min"] != m["max"]:
            rdef = m["default"]

        # cross-check NibePi vs ModbusManager CSV
        if np_ and m:
            for fld, a_v, b_v in (("data_type", np_["size"], m["size"]), ("scale", int(np_["factor"]), m["factor"]),
                                  ("access", np_["mode"], m["mode"]), ("title", np_["titel"], m["title"])):
                if a_v != b_v:
                    if fld == "title" and a_v.lstrip("=") == b_v:
                        notes.append("NibePi title has a stray leading '=' (spreadsheet artefact); removed.")
                        title = b_v
                        continue
                    conflicts.append({"field": fld, "source": "nibe-modbusmanager-f730-db8310", "value": b_v,
                                      "chosen": a_v})
        if np_ is None and m is not None:
            notes.append("Present in ModbusManager export but missing from NibePi F730.json.")
        if m is None and np_ is not None:
            notes.append("Present in NibePi F730.json but missing from ModbusManager export DB 8310.")
        # community corrections (recorded, not applied)
        for k_ext, fld in (("factor", "scale"), ("unit", "unit"), ("size", "data_type")):
            if k_ext in e and (np_ or m):
                cur = {"scale": factor, "unit": unit, "data_type": size}[fld]
                val = e[k_ext]
                if fld == "unit" and val is not None:
                    val = val.strip() or None
                if val != cur:
                    conflicts.append({"field": fld, "source": "yozik04-nibe-extensions", "value": val, "chosen": cur})
        if m and np_ is None and "factor" in e and e["factor"] != factor:
            pass  # already recorded above

        value_map = None
        if np_ and "map" in np_:
            value_map = {}
            for item in np_["map"]:
                (k, v), = item.items()
                value_map[str(k)] = SV_EN.get(v, v)
            if any(SV_EN.get(v) not in (None, v) for item in np_["map"] for v in item.values()):
                notes.append("value_map labels translated from Swedish (NibePi) to English by WS01.")
        elif e.get("mappings"):
            notes.append("yozik04/nibe lists value labels: " + json.dumps(e["mappings"], ensure_ascii=False))

        # protocol flags
        if size in ("u32", "s32"):
            flags.append("two_slot_32bit")
        if (rid & 0xFF) == 0x5C or (rid >> 8) == 0x5C:
            flags.append("address_contains_0x5c")
        if xor8([0xC0, 0x69, 0x02, rid & 0xFF, rid >> 8]) == 0x5C:
            flags.append("read_request_checksum_0x5c")
        if rid in SPECIAL:
            flags += SPECIAL[rid]["flags"]
            notes.append(SPECIAL[rid]["note"])
        if desc and re.search(r"\d+\s*=\s*\w", desc) and value_map is None:
            flags.append("enum_in_description")

        regs.append({
            "id": rid,
            "name": slug(title, rid),
            "title": title,
            "description": (desc or "").strip() or None,
            "data_type": size,
            "scale": factor,
            "unit": unit,
            "access": "read_write" if mode == "R/W" else "read",
            "write_allowed": False,
            "raw_min": rmin,
            "raw_max": rmax,
            "raw_default": rdef,
            "value_map": value_map,
            "flags": sorted(set(flags)),
            "sources": sources,
            "conflicts": conflicts,
            "notes": notes,
            "verification": "unverified",
        })

    doc = {
        "schema": "nibe-gateway/register-catalog",
        "schema_version": 1,
        "catalog_id": "f730",
        "catalog_version": "2026.10.22-1",
        "model": "F730",
        "model_match": {"product_info_prefix": "F730", "note": "Matched against the 0x6D product-info model string (first token, before '-' or space), as NibePi does."},
        "firmware": {"applies_to": "unknown", "note": "Sources do not state firmware ranges. ModbusManager DB 8310 (2020-06-24)."},
        "semantics": {
            "value": "engineering_value = raw / scale (raw interpreted per data_type, little-endian)",
            "raw_min_max": "raw (unscaled) limits from the source; null = not specified",
            "access": "what the SOURCE claims; the gateway never writes based on this (write_allowed is always false)",
            "verification": "unverified for every entry until confirmed on the owner's F730 (see docs/research/register-catalog.md)",
            "sentinels": "HYPOTHESIS: raw values at the integer limit (s16 -32768, u16 65535, u8 255, s8 -128, s32 -2^31, u32 2^32-1) mean 'no value' (yozik04/nibe encoders.py)",
        },
        "write_policy": "disabled",
        "generated_by": "python3 docs/research/tools/build_f730_catalog.py (see header for inputs)",
        "sources": SOURCES,
        "counts": {
            "total": len(regs),
            "read": sum(r["access"] == "read" for r in regs),
            "read_write_claimed": sum(r["access"] == "read_write" for r in regs),
            "with_conflicts": sum(bool(r["conflicts"]) for r in regs),
        },
        "registers": regs,
    }
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {a.out}: {doc['counts']}")


if __name__ == "__main__":
    main()
