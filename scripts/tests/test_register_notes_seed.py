"""Seed for the owner's register notes (ADR 0041): survey mapping, overrides and API limits."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("register_notes_seed", ROOT / "scripts/register-notes-f730-owner.py")
seed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seed)


def survey(*regs):
    return {"registers": [{"id": i, "class": c, "note": n} for i, c, n in regs]}


class MappingTest(unittest.TestCase):
    def test_classes_map_and_excluded_get_no_note(self):
        body = seed.build(
            survey((40004, "a", ""), (40005, "b", "sentinel"), (43141, "d", "W too low"),
                   (45171, "excluded", "never"), (40999, "c", "timeout"),
                   # registers the override tables name must exist in the input
                   *[(i, "d", "") for i in sorted(set(seed.UNMAINTAINED) | set(seed.RESOLVED))],
                   *[(i, "a", "") for i in sorted((seed.DISPLAY_VERIFIED | set(seed.LABELS))
                                                  - set(seed.UNMAINTAINED) - set(seed.RESOLVED) - {40004})]),
            " ".join(str(i) for i in seed.DISPLAY_VERIFIED))
        notes = {n["register"]: n for n in body["notes"]}
        self.assertEqual(body["mode"], "replace")
        self.assertEqual(notes[40004]["relevance"], "relevant")
        self.assertEqual(notes[40005], {"register": 40005, "relevance": "absent", "note": "sentinel",
                                        "source": "docs/research/register-survey-2026-10-25.json class b"})
        self.assertEqual(notes[43141]["relevance"], "suspicious")
        self.assertNotIn(45171, notes)
        self.assertNotIn(40999, notes)
        self.assertNotIn("label", notes[40004])
        self.assertTrue(notes[40004]["display_verified"])

    def test_real_files_overrides_and_api_limits(self):
        body = seed.build(json.loads(seed.SURVEY.read_text()), seed.PANEL.read_text())
        notes = {n["register"]: n for n in body["notes"]}
        for rid in (41256, 41257, 41258, 40940, 40072):
            self.assertEqual(notes[rid]["relevance"], "unmaintained", rid)
            self.assertTrue(notes[rid]["note"])
        self.assertEqual(notes[43108]["label"], "active fan speed step")
        self.assertEqual(notes[43108]["relevance"], "relevant")
        self.assertEqual(notes[40050]["label"], "airflow (tracks fan output and frost)")
        self.assertEqual(notes[43124]["label"], "airflow reference (only while the compressor runs)")
        self.assertEqual(notes[40026]["label"], "air after the evaporator")
        verified = {r for r, n in notes.items() if n.get("display_verified")}
        self.assertEqual(verified, set(seed.DISPLAY_VERIFIED))
        # Ambiguous pair, trend-only rows and "different meaning" rows stay unmarked.
        for rid in (47044, 47049, 40026, 40940, 41256, 44898, 44899):
            self.assertNotIn(rid, verified, rid)
        for rid in (44308, 40083, 43005, 47260, 47265, 43108, 40050, 43081, 47007, 47011, 44896):
            self.assertIn(rid, verified, rid)
        self.assertNotIn(45171, notes, "excluded registers get no note")
        # Server limits: <= 2048 notes, text <= 256 bytes, label <= 80 chars, body <= 256 KiB.
        self.assertLessEqual(len(body["notes"]), 2048)
        self.assertLess(len(json.dumps(body, indent=1, ensure_ascii=False).encode()), 256 * 1024)
        for n in body["notes"]:
            self.assertLessEqual(set(n), {"register", "relevance", "display_verified", "label", "note", "source"})
            self.assertIn(n["relevance"], {"relevant", "absent", "suspicious", "unmaintained"})
            for field in ("note", "source"):
                if field in n:
                    self.assertTrue(0 < len(n[field].encode()) <= 256, (n["register"], field))
                    self.assertFalse(any(ord(c) < 32 for c in n[field]))
            self.assertLessEqual(len(n.get("label", "")), 80)


if __name__ == "__main__":
    unittest.main()
