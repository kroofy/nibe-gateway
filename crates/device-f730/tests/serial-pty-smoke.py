#!/usr/bin/env python3
"""Exercise the real feature-gated serial adapter using a local Linux PTY.
This is synthetic plumbing verification, never a physical UART/F730 claim.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
os.chdir(ROOT)


def pump(command, payload=b"", address=0x20):
    wire = payload.replace(b"\x5c", b"\x5c\x5c")
    body = address.to_bytes(2, "big") + bytes([command, len(wire)]) + wire
    checksum = 0
    for byte in body:
        checksum ^= byte
    return b"\x5c" + body + bytes([0xc5 if checksum == 0x5c else checksum])


with tempfile.TemporaryDirectory(prefix="nibe-ws04-pty-") as temporary:
    work = Path(temporary)
    master, slave = pty.openpty()
    serial = os.ttyname(slave)
    log = (work / "gateway.log").open("w+")
    process = subprocess.Popen([
        "cargo", "run", "-p", "nibe-gateway", "--features", "serial", "--",
        "--device", "hardware", "--serial", serial, "--network", "simulated",
        "--bind", "127.0.0.1:0", "--data-dir", str(work / "data"),
    ], stdout=log, stderr=log, start_new_session=True)
    try:
        for _ in range(1200):
            text = (work / "gateway.log").read_text()
            match = re.search(r"127\.0\.0\.1:[1-9]\d*", text) if "gateway listening" in text else None
            if match:
                base = "http://" + match[0]
                break
            if process.poll() is not None:
                raise AssertionError("gateway exited: " + text)
            time.sleep(0.05)
        else:
            raise AssertionError("serial gateway startup timeout")
        os.close(slave)
        slave = None

        def exchange(frame, expected):
            # Split every frame at a non-frame boundary to exercise actual UART reads.
            os.write(master, frame[:3])
            time.sleep(0.002)
            os.write(master, frame[3:])
            readable, _, _ = select.select([master], [], [], 0.5)
            response = os.read(master, 256) if readable else b""
            assert response == expected, (frame.hex(), response.hex(), expected.hex())

        # Reading 48852 changes only our decoder; we never send a write or read request.
        exchange(pump(0x6d, b"\x01\x04\xd2F730 SYNTHETIC PTY"), b"\x06")
        exchange(pump(0x6a, (48852).to_bytes(2, "little") + b"\x00\x00\x00\x00"), b"\x06")
        exchange(pump(0x6a, (43424).to_bytes(2, "little") + bytes.fromhex("0000f512")), b"\x06")
        exchange(pump(0x6a, (40004).to_bytes(2, "little") + bytes.fromhex("8300ffff")), b"\x06")
        exchange(pump(0x69), b"\x06")
        exchange(pump(0x6b), b"\x06")
        exchange(pump(0xee, address=0x19), b"")
        bad = bytearray(pump(0x69))
        bad[-1] ^= 1
        exchange(bad, b"\x15")

        def request(path, data=None, expected=200, setup=False):
            command = ["curl", "-sS", "--max-time", "10", "-c", str(work / "cookies"),
                       "-b", str(work / "cookies"), "-H", "X-Requested-With: nibe-gateway",
                       "-o", str(work / "response.json"), "-w", "%{http_code}"]
            if setup:
                token = json.loads((work / "data/setup-secrets.json").read_text())["setup_token"]
                command += ["-H", "X-Setup-Token: " + token]
            if data is not None:
                command += ["-H", "Content-Type: application/json", "-d", json.dumps(data)]
            response = subprocess.run(command + [base + path], capture_output=True, text=True)
            assert response.returncode == 0, "curl transport failed"
            assert int(response.stdout) == expected, (path, response.stdout, expected)
            return json.loads((work / "response.json").read_text())

        request("/api/v1/setup/admin", {"password": "ws04-local-pty-test-password"}, setup=True)
        explorer = request("/api/v1/registers/explorer")
        values = {r["id"]: r["reading"] for r in explorer["items"] if r["reading"]}
        assert values[48852]["decoded"]["value"] == 0
        assert values[43424]["decoded"]["value"] == 4853
        assert values[40004]["decoded"]["value"] == 13.1
        assert explorer["word_order"] == "high_word_first"
        assert explorer["diagnostics"]["acks_sent"] == 6
        assert explorer["diagnostics"]["naks_sent"] == 1
        assert explorer["diagnostics"]["frames_for_other_addresses"] == 1
        assert explorer["diagnostics"]["read_requests_sent"] == 0
        assert explorer["diagnostics"]["ack_deadline_misses"] == 0
        assert not explorer["capabilities"]["active_probing"]
        request("/api/v1/discovery/probe", {"register_ids": [48852]}, expected=403)
        request("/api/v1/controls/heating-offset", {"offset": 1}, expected=403)
        os.killpg(process.pid, signal.SIGTERM)
        assert process.wait(timeout=10) == 0
        # WS08: a port already held (flock) by another master, as NibePi's
        # node-serialport does, must make the gateway refuse to start
        # (serialport-rs takes TIOCEXCL plus a non-blocking exclusive flock).
        held_master, held_slave = pty.openpty()
        try:
            fcntl.flock(held_slave, fcntl.LOCK_EX | fcntl.LOCK_NB)
            refused = subprocess.run([
                "cargo", "run", "-q", "-p", "nibe-gateway", "--features", "serial", "--",
                "--device", "hardware", "--serial", os.ttyname(held_slave), "--network", "simulated",
                "--bind", "127.0.0.1:0", "--data-dir", str(work / "data-locked"),
            ], capture_output=True, text=True, timeout=120)
            assert refused.returncode != 0 and "exclusive lock" in refused.stderr, refused.stderr
        finally:
            os.close(held_slave)
            os.close(held_master)
        print("WS04 serial PTY: fragmented frames, 48852=0/read-response word order, scaling, ACK/NAK, RMU silence, hardware probe/write refusal and second-master lock refusal passed (synthetic, no physical hardware)")
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
        if slave is not None:
            os.close(slave)
        os.close(master)
        log.close()
