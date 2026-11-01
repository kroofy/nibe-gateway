#!/usr/bin/env python3
"""Run the real replay gateway and exercise production-authenticated API via curl.
No hardware, network-manager, external Python packages, or preexisting state needed.
"""
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
os.chdir(ROOT)

with tempfile.TemporaryDirectory(prefix="nibe-replay-") as temporary:
    work = Path(temporary)
    log = (work / "gateway.log").open("w+")
    capture = work / "recorded.capture.jsonl"
    process = subprocess.Popen([
        "cargo", "run", "-p", "nibe-gateway", "--", "--device", "replay",
        "--bind", "127.0.0.1:0", "--data-dir", str(work / "data"),
        "--record-capture", str(capture),
    ], stdout=log, stderr=log, start_new_session=True)
    try:
        for _ in range(1200):
            log.flush()
            text = (work / "gateway.log").read_text()
            match = re.search(r"127\.0\.0\.1:[1-9]\d*", text) if "gateway listening" in text else None
            if match:
                base = "http://" + match[0]
                break
            if process.poll() is not None:
                raise AssertionError("gateway exited: " + text)
            time.sleep(0.05)
        else:
            raise AssertionError("gateway startup timeout")

        def request(path, data=None, *, method=None, authenticated=True,
                    csrf=True, extra=(), expected=200, raw_file=None):
            output = work / "response.json"
            command = ["curl", "-sS", "--max-time", "10", "-o", str(output),
                       "-w", "%{http_code}", "-c", str(work / "cookies")]
            if authenticated:
                command += ["-b", str(work / "cookies")]
            if csrf:
                command += ["-H", "X-Requested-With: nibe-gateway"]
            if data is not None or raw_file is not None:
                command += ["-H", "Content-Type: application/json", "--data-binary",
                            "@" + str(raw_file) if raw_file else json.dumps(data)]
            if method:
                command += ["-X", method]
            command += list(extra) + [base + path]
            response = subprocess.run(command, capture_output=True, text=True)
            assert response.returncode == 0, "curl transport failed"
            assert int(response.stdout) == expected, (path, response.stdout, expected)
            return json.loads(output.read_text())

        request("/api/v1/registers/explorer", authenticated=False, expected=401)
        request("/api/v1/discovery/probe", {"register_ids": [40004]},
                authenticated=False, expected=401)
        token = json.loads((work / "data/setup-secrets.json").read_text())["setup_token"]
        request("/api/v1/setup/admin", {"password": "local-replay-test-password"},
                authenticated=False, extra=["-H", "X-Setup-Token: " + token])
        for _ in range(60):
            device = request("/api/v1/device")
            if len(device["registers"]) == 17:
                break
            time.sleep(0.05)
        assert device["mode"] == "replay"
        assert device["identity"]["model"] == "F730"
        assert device["identity"]["firmware"] == "1234"
        assert not device["controls_enabled"]
        expected = json.loads((ROOT / "fixtures/serial-captures/synthetic-f730-session.expected.json").read_text())
        actual = {str(r["id"]): r["decoded"]["value"] for r in device["registers"]}
        assert actual == expected["final_values"]
        explorer = request("/api/v1/registers/explorer")
        assert len(explorer["items"]) == 581
        assert sum(r["state"] == "observed" for r in explorer["items"]) == 17
        assert all(not r["writable"] for r in explorer["items"])
        assert explorer["diagnostics"]["frames_for_us"] == 8
        assert explorer["diagnostics"]["acks_sent"] == 8
        assert explorer["diagnostics"]["naks_sent"] == 1
        assert explorer["diagnostics"]["ack_deadline_misses"] == 0
        capabilities = request("/api/v1/device/capabilities")
        assert not any(capabilities[k] for k in ("writes", "raw_writes", "rmu40_emulation", "hardware_verified"))
        request("/api/v1/discovery/scan", method="POST")
        request("/api/v1/discovery/probe", {"register_ids": [40004]}, csrf=False, expected=403)
        for ids in ([1], [40043], [48968], [48732], [45171], [40004, 40004]):
            request("/api/v1/discovery/probe", {"register_ids": ids}, expected=422)
        request("/api/v1/discovery/probe", {"register_ids": [48852, 40004]})
        request("/api/v1/discovery/probe", {"register_ids": [40008]}, expected=429)
        status = request("/api/v1/discovery/probe/cancel", method="POST")
        assert status["cancelled"] and not status["running"]
        request("/api/v1/controls/heating-offset", {"offset": 2}, expected=403)
        request("/api/v1/registers/40004", {"value": 123}, expected=404)
        request("/api/v1/registers/catalog/import", {}, expected=422)
        result = request("/api/v1/registers/catalog/import",
                         raw_file=ROOT / "fixtures/register-catalogs/f730.json")
        assert result["count"] == 581 and not result["writes_enabled"]
        # Import is process-local, versioned, validated, and clears observations.
        for _ in range(60):
            if all(r["reading"] is None for r in request("/api/v1/registers/explorer")["items"]):
                break
            time.sleep(0.01)
        else:
            raise AssertionError("catalog import did not clear prior observations")
        request("/api/v1/auth/logout", method="POST")
        request("/api/v1/registers/explorer", expected=401)
        os.killpg(process.pid, signal.SIGTERM)
        assert process.wait(timeout=10) == 0
        records = [json.loads(line) for line in capture.read_text().splitlines()]
        assert records[0]["format"] == "nibe-gateway/capture-jsonl"
        assert all(a["t_us"] <= b["t_us"] for a, b in zip(records[1:], records[2:]))
        assert len([r for r in records[1:] if r["dir"] == "rx"]) == 12
        assert all(r["hex"] in ("06", "15") for r in records[1:] if r["dir"] == "tx")
        summary = json.loads(Path(str(capture) + ".summary.json").read_text())
        assert summary["complete"] and summary["dropped"] == 0
        print("Replay curl: 17 expected values, 581 catalog entries, auth/CSRF, probe safety/cancel, offline import, read-only policy and drained capture passed")
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
        log.close()
