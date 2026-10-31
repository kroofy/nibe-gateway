#!/usr/bin/env python3
"""Audit log over the real serial adapter (ADR 0128), using a local Linux PTY as a fake F730: an HTTP register
write, the pump's acceptance and the read-back all appear in GET /api/v1/audit and in audit.jsonl.
With NIBE_SMOKE_MQTT_PORT set to a local broker's port (for example
`docker run --rm -p 127.0.0.1:18883:1883 eclipse-mosquitto:2 mosquitto -c /mosquitto-no-auth.conf`), MQTT register
commands are checked too: a set_raw is recorded, a get is not.
Synthetic plumbing verification, never a physical UART/F730 claim. Run: python3 crates/api/tests/audit-pty-smoke.py"""
import json, os, pty, re, select, signal, socket, subprocess, sys, tempfile, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
os.chdir(ROOT)
def pump(command, payload=b"", address=0x20):
    wire = payload.replace(b"\x5c", b"\x5c\x5c")
    body = address.to_bytes(2, "big") + bytes([command, len(wire)]) + wire
    c = 0
    for b in body: c ^= b
    return b"\x5c" + body + bytes([0xc5 if c == 0x5c else c])
def mqtt_publish(port, topic, payload):
    """Minimal MQTT 3.1.1 CONNECT + QoS 0 PUBLISH + DISCONNECT."""
    def field(b): return len(b).to_bytes(2, "big") + b
    def packet(kind, body):
        n, length = len(body), b""
        while True:
            byte, n = n % 128, n // 128
            length += bytes([byte | (0x80 if n else 0)])
            if not n: break
        return bytes([kind]) + length + body
    with socket.create_connection(("127.0.0.1", port), timeout=5) as s:
        s.sendall(packet(0x10, field(b"MQTT") + b"\x04\x02\x00\x3c" + field(b"audit-smoke")))
        assert s.recv(4)[:2] == b"\x20\x02"
        s.sendall(packet(0x30, field(topic.encode()) + payload.encode()))
        s.sendall(b"\xe0\x00")
work = Path(tempfile.mkdtemp(prefix="nibe-audit-pty-"))
master, slave = pty.openpty()
log = (work / "gateway.log").open("w+")
p = subprocess.Popen(["cargo", "run", "-q", "-p", "nibe-gateway", "--features", "serial", "--", "--device", "hardware",
    "--serial", os.ttyname(slave), "--network", "simulated", "--allow-hardware-reads", "--bind", "127.0.0.1:0",
    "--data-dir", str(work / "data")], stdout=log, stderr=log, start_new_session=True)
try:
    for _ in range(2400):
        t = (work / "gateway.log").read_text()
        m = re.search(r"127\.0\.0\.1:[1-9]\d*", t) if "gateway listening" in t else None
        if m: base = "http://" + m[0]; break
        if p.poll() is not None: raise SystemExit("exited: " + t)
        time.sleep(0.05)
    os.close(slave)
    def send(frame):
        os.write(master, frame)
        r, _, _ = select.select([master], [], [], 0.5)
        return os.read(master, 256) if r else b""
    def req(path, data=None, setup=False, method=None):
        cmd = ["curl", "-sS", "--max-time", "30", "-c", str(work/"c"), "-b", str(work/"c"), "-H", "X-Requested-With: nibe-gateway"]
        if setup: cmd += ["-H", "X-Setup-Token: " + json.loads((work/"data/setup-secrets.json").read_text())["setup_token"]]
        if data is not None: cmd += ["-H", "Content-Type: application/json", "-d", json.dumps(data)]
        return json.loads(subprocess.run(cmd + [base + path], capture_output=True, text=True).stdout or "null")
    send(pump(0x6d, b"\x01\x04\xd2F730 SYNTHETIC PTY"))
    req("/api/v1/setup/admin", {"password": "audit-pty-test-password"}, setup=True)
    print("write:", req("/api/v1/controls/register-write", {"register": 47049, "raw": 470}))
    # Write token: the gateway sends the write request; the pump accepts it.
    out = send(pump(0x6b)); assert out[:2] == b"\xc0\x6b", out.hex()
    assert send(pump(0x6c, b"\x01")) == b"\x06"
    # Read token: the gateway reads 47049 back; the pump answers 470.
    out = send(pump(0x69)); assert out[:2] == b"\xc0\x69", out.hex()
    assert send(pump(0x6a, (47049).to_bytes(2, "little") + (470).to_bytes(4, "little"))) == b"\x06"
    for _ in range(40):
        page = req("/api/v1/audit")
        if any(e["source"] == "pump" for e in page["entries"]): break
        time.sleep(0.25)
    for e in page["entries"]: print(json.dumps(e))
    pumps = [e for e in page["entries"] if e["source"] == "pump"]
    assert len(pumps) == 1 and pumps[0]["ok"] and pumps[0]["target"] == "47049" and "read back raw 470" in pumps[0]["outcome"], page
    # A denied write is not read back by the adapter: recorded after the 10 s read-back wait.
    req("/api/v1/controls/register-write", {"register": 47049, "raw": 300})
    out = send(pump(0x6b)); assert out[:2] == b"\xc0\x6b", out.hex()
    assert send(pump(0x6c, b"\x00")) == b"\x06"
    for _ in range(80):
        page = req("/api/v1/audit")
        if sum(e["source"] == "pump" for e in page["entries"]) == 2: break
        time.sleep(0.25)
    denied = [e for e in page["entries"] if e["source"] == "pump"][0]
    assert not denied["ok"] and denied["detail"] == "raw 300" and denied["outcome"] == "denied; no read-back", denied
    http = [e for e in page["entries"] if e["action"] == "Register write"]
    assert http and http[0]["actor"] == "admin" and http[0]["target"] == "47049", page
    broker = os.environ.get("NIBE_SMOKE_MQTT_PORT")
    if broker:
        view = req("/api/v1/integrations/mqtt", {"enabled": True, "host": "127.0.0.1", "port": int(broker),
            "username": None, "password": None, "topic_prefix": "nibe", "discovery": False,
            "discovery_prefix": "homeassistant", "allow_commands": True})
        for _ in range(40):
            if req("/api/v1/integrations/mqtt")["status"]["state"] == "connected": break
            time.sleep(0.25)
        time.sleep(0.5)  # the subscription follows the connection
        mqtt_publish(int(broker), view["base_topic"] + "/register/40004/get", "")
        mqtt_publish(int(broker), view["base_topic"] + "/register/47104/set_raw", "90")
        for _ in range(40):
            page = req("/api/v1/audit")
            if any(e["source"] == "mqtt" for e in page["entries"]): break
            time.sleep(0.25)
        mqtt = [e for e in page["entries"] if e["source"] == "mqtt"]
        assert len(mqtt) == 1 and mqtt[0]["action"] == "MQTT register/47104/set_raw", page
        assert mqtt[0]["target"] == "47104" and mqtt[0]["detail"] == "90" and mqtt[0]["ok"], mqtt
        print("mqtt:", json.dumps(mqtt[0]))
        # Complete that write on the pump, so the next write is not queued behind it.
        out = send(pump(0x6b)); assert out[:2] == b"\xc0\x6b", out.hex()
        assert send(pump(0x6c, b"\x01")) == b"\x06"
        out = send(pump(0x69)); assert out[:2] == b"\xc0\x69", out.hex()
        assert send(pump(0x6a, (47104).to_bytes(2, "little") + (90).to_bytes(4, "little"))) == b"\x06"
        for _ in range(40):
            page = req("/api/v1/audit")
            if any(e["source"] == "pump" and e["target"] == "47104" for e in page["entries"]): break
            time.sleep(0.25)
        assert page["entries"][0]["outcome"] == "accepted; read back raw 90", page["entries"][0]
    # Stopped before the read-back: the outcome still reaches the file on shutdown.
    req("/api/v1/controls/register-write", {"register": 47049, "raw": 480})
    out = send(pump(0x6b)); assert out[:2] == b"\xc0\x6b", out.hex()
    assert send(pump(0x6c, b"\x01")) == b"\x06"
    time.sleep(0.7)
    os.killpg(p.pid, signal.SIGTERM); p.wait(timeout=10)
    lines = (work / "data/audit.jsonl").read_text().splitlines()
    last = json.loads(lines[-1])
    assert (last["source"], last["detail"], last["outcome"]) == ("pump", "raw 480", "accepted; no read-back"), last
    assert len(lines) == len(page["entries"]) + 2, lines
    print(f"audit PTY smoke: HTTP writes, pump acceptance/denial and read-backs recorded; {len(lines)} lines in audit.jsonl (synthetic, no physical hardware)")
finally:
    if p.poll() is None: os.killpg(p.pid, signal.SIGKILL)
    subprocess.run(["rm", "-rf", str(work)])
