"""Source-format synthetic imports and fake-systemctl ownership handoff."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


importer = load("nibepi_import", "scripts/import-nibepi-log.py")
fallback = load("nibepi_fallback", "packaging/nibepi/fallback.py")


class ImportTest(unittest.TestCase):
    def test_real_log_js_format_preserves_chunks_and_relative_midnight_timing(self):
        # backend.js Array.from(data), then log.js `${time} ${kind}: [${data}]`.
        source = io.StringIO("2026-10-22 23:59:59.999 RAW: [92,0,32]\r\n"
                             "2026-10-22 23:59:59.999 SENT: [SECRET-CANARY]\n"
                             "2026-10-23 00:00:00.001 RAW: [105,0,73,92,0,32,107,0,75]\n"
                             "2026-10-23 00:00:00.001 RAW: [6]\n")
        output = io.StringIO()
        self.assertEqual(importer.convert(source, output, True), 3)
        rows = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertTrue(rows[0]["synthetic"])
        self.assertEqual([r["t_us"] for r in rows[1:]], [0, 2000, 2000])
        self.assertEqual(bytes.fromhex("".join(r["hex"] for r in rows[1:])),
                         bytes([92, 0, 32, 105, 0, 73, 92, 0, 32, 107, 0, 75, 6]))
        self.assertNotIn("SECRET", output.getvalue())
        self.assertEqual({r["dir"] for r in rows[1:]}, {"rx"})

    def test_rejects_bad_input_and_clock_regression(self):
        for raw in ("[256]", "[-1]", "[]", "[1.5]", "[1,,2]", "garbage", "[True]"):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                importer.convert(io.StringIO(f"2026-10-22 00:00:00.000 RAW: {raw}\n"), io.StringIO())
        with self.assertRaisesRegex(ValueError, "backwards"):
            importer.convert(io.StringIO("2026-10-22 00:00:01.000 RAW: [6]\n2026-10-22 00:00:00.000 RAW: [6]\n"), io.StringIO())
        with self.assertRaises(ValueError):
            importer.convert(io.StringIO("no raw chunks\n"), io.StringIO())

    def test_cli_private_atomic_file_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, output = Path(tmp) / "log", Path(tmp) / "capture"
            source.write_text("2026-10-22 00:00:00.000 RAW: [92,0,32,105,0,73]\n")
            command = [sys.executable, str(ROOT / "scripts/import-nibepi-log.py"), str(source), str(output), "--synthetic"]
            subprocess.run(command, check=True, capture_output=True)
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            contents = output.read_bytes()
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
            self.assertEqual(output.read_bytes(), contents)


class FallbackTest(unittest.TestCase):
    def test_ownership_order_latch_and_stop_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = root / "systemctl"
            fake.write_text('#!/usr/bin/env python3\nimport os,sys\nwith open(os.environ["CALL_LOG"],"a") as f: f.write(" ".join(sys.argv[1:])+"\\n")\nif os.environ.get("FAIL_STOP") and "stop" in sys.argv: sys.exit(1)\n')
            fake.chmod(0o755)
            calls = root / "calls"
            with patch.dict(os.environ, {"NIBE_SYSTEMCTL": str(fake), "CALL_LOG": str(calls),
                                         "NIBE_FALLBACK_LATCH": str(root / "latched")}, clear=False):
                with patch.object(fallback, "healthy", return_value=True):
                    fallback.main()
                    self.assertFalse(calls.exists())
                with patch.object(fallback, "healthy", return_value=False):
                    with patch.dict(os.environ, {"FAIL_STOP": "1"}), self.assertRaises(subprocess.CalledProcessError):
                        fallback.main()
                    self.assertEqual(calls.read_text().splitlines(), ["stop nibe-gateway.service"])
                    calls.unlink()
                    fallback.main()
                    fallback.main()
                    self.assertEqual(calls.read_text().splitlines(), ["stop nibe-gateway.service", "start nodered.service"])


if __name__ == "__main__":
    unittest.main()
