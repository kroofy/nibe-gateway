#!/usr/bin/env python3
"""Build the register notes (ADR 0119) for the owner's F730 from the two research files.

Output is the body for `PUT /api/v1/registers/notes` (mode "replace"). The script never talks to
the gateway; apply the output with an admin session (see the usage below). Stdlib only.

Mapping:
- survey class a -> relevant, b -> absent, d -> suspicious; "excluded" (and c) registers get no note.
- RESOLVED: class-d registers whose doubt later evidence cleared -> relevant.
- UNMAINTAINED: answers but firmware 9721 does not keep it up to date.
- DISPLAY_VERIFIED: registers the panel-verification doc marks VERIFIED against the pump's display
  (plus the fan registers 47260/47265/43108/40050 from the 2026-10-26 fan-speed test).

Usage:
    python3 scripts/register-notes-f730-owner.py --out /tmp/register-notes.json
    # on a machine that reaches the gateway (password prompted, never stored):
    read -rsp 'Admin password: ' PW; echo
    jq -n --arg password "$PW" '{password: $password}' |
      curl -fsS -c /tmp/nibe.cookies -H 'Content-Type: application/json' \
        -H 'X-Requested-With: nibe-gateway' --data-binary @- http://GATEWAY:8080/api/v1/auth/login
    unset PW
    curl -fsS -b /tmp/nibe.cookies -X PUT -H 'Content-Type: application/json' \
      -H 'X-Requested-With: nibe-gateway' --data-binary @/tmp/register-notes.json \
      http://GATEWAY:8080/api/v1/registers/notes | jq '.register_notes | length'
    rm -f /tmp/nibe.cookies
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SURVEY = ROOT / "docs/research/register-survey-2026-10-25.json"
PANEL = ROOT / "docs/research/panel-verification-2026-10-25.md"
SURVEY_SOURCE = "docs/research/register-survey-2026-10-25.json class {}"
PANEL_SOURCE = "docs/research/panel-verification-2026-10-25.md"
MAX_TEXT = 256

CLASS_RELEVANCE = {"a": "relevant", "b": "absent", "d": "suspicious"}

# Doubt cleared later (all in the panel-verification doc).
RESOLVED = {
    43108: "Active fan speed step: 0 = normal, 1-4 = temporary speed; went 0 -> 3 -> 0 with a temporary speed 3 "
           "(fan-speed test 2026-10-26). Not a percentage.",
    43124: "Sentinel while the compressor is off, a value only while it runs (fan watch 2026-10-26); "
           "not a fault. Matches the display reference value.",
    40051: "Catalog scale corrected to /10 on 2026-10-26; follows 40050 and the fan.",
    44896: "Display shows 0.1 for raw 1, so the catalog scale x0.1 is right; the survey's scale-1 doubt is withdrawn.",
}

UNMAINTAINED = {
    41256: "Always 0, even at fan speed 3 (fan-speed test 2026-10-26): not maintained on firmware 9721. "
           "Use 43108 for the speed step and 40050 for the fan output.",
    41257: "Always 0, even at fan speed 3 (fan-speed test 2026-10-26): not maintained on firmware 9721. See 41256.",
    41258: "Always 0, even at fan speed 3 (fan-speed test 2026-10-26): not maintained on firmware 9721. See 41256.",
    40940: "32-bit degree minutes are not maintained on firmware 9721 (stay ~0 while the display follows 43005). "
           "Use 43005.",
    40072: "BF1 flow stuck at 5.0 l/min in every mode and pump speed. The display shows the same 5.0, so it is "
           "the pump's own (default) value, not a decode error.",
}

# Marked VERIFIED in docs/research/panel-verification-2026-10-25.md. Left out on purpose: the
# trend-consistent temperatures (40008, 40012-40022, 40026, 43140), HYPOTHESIS rows, 40940/41256-41258
# ("different meaning"), 44898/44899 (scale unproven) and the ambiguous 47044/47049 pair.
DISPLAY_VERIFIED = frozenset({
    # heat meters, BF1 (display page 9)
    44298, 44300, 44306, 44308, 40072,
    # degree minutes, electrical, compressor, inverter state
    43005, 47212, 47214, 43122, 32260, 43136, 43001, 43086,
    # ventilation and fan
    47260, 47265, 43108, 40050, 43124,
    # smart price adaption
    44908, 44896, 44897,
    # temperatures and pump speed compared within minutes
    40004, 43009, 40033, 43437, 40025, 40071,
    # phase currents L3/L2/L1
    40079, 40081, 40083,
    # compressor starts/hours, add-heat time factor
    43416, 43420, 43424, 43081, 43239,
    # heat curve and offset, hot-water start/stop temperatures
    47007, 47011, 47043, 47045, 47046, 47047, 47048,
})

LABELS = {
    43108: "active fan speed step",
    40050: "airflow (tracks fan output and frost)",
    43124: "airflow reference (only while the compressor runs)",
    40026: "air after the evaporator",
}


def clip(text: str) -> str | None:
    text = " ".join((text or "").split())
    if not text:
        return None
    return text if len(text.encode()) <= MAX_TEXT else text.encode()[: MAX_TEXT - 3].decode(errors="ignore") + "..."


def build(survey: dict, panel_text: str) -> dict:
    missing = sorted(i for i in DISPLAY_VERIFIED if str(i) not in panel_text)
    if missing:
        raise SystemExit(f"display-verified registers not in the panel doc: {missing}")
    notes = []
    for reg in sorted(survey["registers"], key=lambda r: r["id"]):
        rid, cls = reg["id"], reg["class"]
        if cls not in CLASS_RELEVANCE:
            continue
        relevance = CLASS_RELEVANCE[cls]
        text = reg.get("note")
        sources = [SURVEY_SOURCE.format(cls)]
        if rid in RESOLVED:
            relevance, text = "relevant", RESOLVED[rid]
        if rid in UNMAINTAINED:
            relevance, text = "unmaintained", UNMAINTAINED[rid]
        if rid in RESOLVED or rid in UNMAINTAINED or rid in DISPLAY_VERIFIED or rid in LABELS:
            sources.append(PANEL_SOURCE)
        entry = {"register": rid, "relevance": relevance, "source": "; ".join(sources)}
        if rid in DISPLAY_VERIFIED:
            entry["display_verified"] = True
        if rid in LABELS:
            entry["label"] = LABELS[rid]
        if clip(text):
            entry["note"] = clip(text)
        notes.append(entry)
    seen = {n["register"] for n in notes}
    absent = sorted((DISPLAY_VERIFIED | set(UNMAINTAINED) | set(LABELS) | set(RESOLVED)) - seen)
    if absent:
        raise SystemExit(f"overrides for registers without a survey note: {absent}")
    return {"mode": "replace", "notes": notes}


def summary(body: dict) -> str:
    counts = Counter(n["relevance"] for n in body["notes"])
    verified = sorted(n["register"] for n in body["notes"] if n.get("display_verified"))
    return (f"{len(body['notes'])} notes: " + ", ".join(f"{k} {counts[k]}" for k in
            ("relevant", "absent", "suspicious", "unmaintained"))
            + f"; display-verified {len(verified)}: {' '.join(map(str, verified))}")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--survey", type=Path, default=SURVEY)
    parser.add_argument("--panel", type=Path, default=PANEL)
    parser.add_argument("--out", type=Path, help="write here instead of stdout")
    args = parser.parse_args(argv)
    body = build(json.loads(args.survey.read_text(encoding="utf-8")), args.panel.read_text(encoding="utf-8"))
    text = json.dumps(body, indent=1, ensure_ascii=False) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    print(summary(body), file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
