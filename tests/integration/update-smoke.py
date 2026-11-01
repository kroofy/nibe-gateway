#!/usr/bin/env python3
"""Real gateway/auth HTTP + independent updater, temp A/B slots and fake service.

Builds native debug executables, strips a copy, and never calls host systemd/SSH.
Exercises signed uploads, health/release identity, process restarts, rollback,
failed readiness rollback, leases, replay capture export and support sanitization.
"""
import http.cookiejar
import io
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tarfile
import tempfile
import time
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
subprocess.run(["cargo", "build", "-p", "nibe-gateway", "-p", "nibe-updater", "--bins"], cwd=REPO, check=True)
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target")) / "debug"
SIGNER = TARGET / "nibe-release"
UPDATER = TARGET / "nibe-updater"
PASSWORD = "private-admin-CANARY"
with tempfile.TemporaryDirectory(prefix="nibe-update-") as tmp:
    root = Path(tmp)
    daemon = None
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    subprocess.run(["strip", "-o", str(root / "gateway"), str(TARGET / "nibe-gateway")], check=True)
    subprocess.run([str(SIGNER), "keygen", str(root / "private"), str(root / "public")], check=True)
    admin = root / "admin"
    admin.mkdir()
    artifacts = []
    for number in range(4):
        (admin / "index.html").write_text(f"signed release {number}")
        artifact = root / f"release-{number}.tar"
        # Third artifact is signed and structurally valid, but its executable
        # exits immediately. It must fail readiness and restore release 0.
        binary = root / "gateway"
        if number == 2:
            binary = root / "failing-gateway"
            binary.write_text("#!/bin/sh\nexit 7\n")
        subprocess.run([str(SIGNER), "sign", str(root / "private"), "0.0.9" if number == 3 else "0.1.0", "local-test", str(binary), str(admin), str(artifact)], check=True)
        artifacts.append(artifact)
    slot = root / "releases/slot-a"
    slot.mkdir(parents=True)
    with tarfile.open(artifacts[0]) as archive:
        manifest = archive.extractfile("manifest.json").read()
        (slot / "manifest.json").write_bytes(manifest)
        with tarfile.open(fileobj=io.BytesIO(archive.extractfile("payload.tar").read())) as payload:
            payload.extractall(slot, filter="data")
    (slot / "nibe-gateway").chmod(0o755)
    (root / "current").symlink_to("releases/slot-a")
    state = root / "update-state"
    state.mkdir()
    (state / "state.json").write_text(json.dumps(dict(phase="idle", active="a", previous=None, candidate=None, version=None, outcome=None)))
    control = root / "control.sock"
    pidfile = root / "gateway.pid"
    helper = root / "restart"
    helper.write_text("#!/usr/bin/env python3\n" + f"root = {str(root)!r}\nport = {port}\n" + '''
import json, os, pathlib, signal, subprocess, time
root = pathlib.Path(root)
pidfile = root / "gateway.pid"
if pidfile.exists():
    try:
        old = int(pidfile.read_text())
        os.kill(old, signal.SIGTERM)
        # Wait for the socket/store to close before launching another gateway.
        for _ in range(100):
            stat = pathlib.Path(f"/proc/{old}/stat")
            if not stat.exists() or stat.read_text().split()[2] == "Z": break
            time.sleep(.02)
    except ProcessLookupError: pass
slot = (root / "current").resolve()
manifest = json.loads((slot / "manifest.json").read_text())
# Test-only additive schema migration seam: simulate a v3 release after the old
# process releases the store, before the candidate starts. No production flag.
if "release 1" in (slot / "admin/index.html").read_text() or "release 2" in (slot / "admin/index.html").read_text():
    config_path = root / "state/config.json"
    config = json.loads(config_path.read_text())
    config.update(schema_version=3, min_reader_version=2, test_v3_additive={"preserve": True})
    temporary = config_path.with_suffix(".migration")
    with open(temporary, "w") as migration:
        os.chmod(temporary, 0o600)
        json.dump(config, migration)
        migration.flush()
        os.fsync(migration.fileno())
    os.replace(temporary, config_path)
log = open(root / "gateway.log", "ab")
process = subprocess.Popen([str(slot / "nibe-gateway"), "--device", "replay", "--data-dir", str(root / "state"), "--bind", f"127.0.0.1:{port}", "--admin-dir", str(slot / "admin"), "--record-capture", str(root / f"capture-{time.time_ns()}.jsonl")], env={**os.environ, "RUST_LOG": "info", "NIBE_UPDATER_SOCKET": str(root / "control.sock"), "NIBE_RELEASE_ID": manifest["sha256"]}, stdout=log, stderr=log, start_new_session=True)
pidfile.write_text(str(process.pid))
''')
    helper.chmod(0o755)
    jar = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(jar))
    def request(path, data=None, expected=200, headers=None, authenticated=True):
        raw = data if isinstance(data, bytes) else json.dumps(data).encode() if data is not None else None
        hdr = {"X-Requested-With": "nibe-gateway", "Content-Type": "application/octet-stream" if isinstance(data, bytes) else "application/json"}
        hdr.update(headers or {})
        client = opener if authenticated else urllib.request.build_opener(urllib.request.ProxyHandler({}))
        req = urllib.request.Request(base + path, data=raw, headers=hdr)
        try:
            response = client.open(req, timeout=120)
        except urllib.error.HTTPError as error:
            response = error
        body = response.read()
        assert "frame-ancestors 'none'" in response.headers.get("Content-Security-Policy", ""), path
        assert response.headers.get("X-Content-Type-Options") == "nosniff", path
        assert response.headers.get("Referrer-Policy") == "no-referrer", path
        assert response.status == expected, (path, response.status, body)
        return json.loads(body) if response.headers.get("Content-Type", "").startswith("application/json") else body
    def wait(predicate, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                result = predicate()
                if result: return result
            except (OSError, AssertionError): pass
            time.sleep(.1)
        raise AssertionError("Timed out; gateway log:\n" + (root / "gateway.log").read_text())
    def login():
        jar.clear()
        return request("/api/v1/auth/login", {"password": PASSWORD})
    def phase():
        return json.loads((state / "state.json").read_text())["phase"]
    try:
        subprocess.run([str(helper)], check=True)
        wait(lambda: request("/health")["ready"])
        daemon_log = open(root / "updater.log", "wb")
        daemon = subprocess.Popen([str(UPDATER), "serve", "--root", str(root), "--key", str(root / "public"), "--target", "local-test", "--socket", str(control), "--service-helper", str(helper), "--health-address", f"127.0.0.1:{port}", "--probation-seconds", "4"], stdout=daemon_log, stderr=daemon_log)
        wait(control.exists)
        token = json.loads((root / "state/setup-secrets.json").read_text())["setup_token"]
        for path in ["/api/v1/updates/status", "/api/v1/diagnostics/support", "/api/v1/diagnostics/developer"]:
            request(path, expected=401, authenticated=False)
        request("/api/v1/updates/upload", b"untrusted", expected=401, headers={"X-Setup-Token": token}, authenticated=False)
        request("/api/v1/setup/admin", {"password": PASSWORD}, headers={"X-Setup-Token": token})
        assert request("/api/v1/updates/status")["available"]
        request("/api/v1/updates/activate", {}, expected=403, headers={"X-Requested-With": "invalid"})
        request("/api/v1/updates/activate", {}, expected=409)
        request("/api/v1/updates/upload", b"corrupt-tar", expected=409)
        assert phase() == "idle"
        denied = request("/api/v1/diagnostics/capture", expected=403)
        assert denied["error"]["message"] == "active diagnostic lease required", denied
        request("/api/v1/diagnostics/developer", {"seconds": 901, "confirmation": "AUTHORIZE_DIAGNOSTICS"}, expected=422)
        lease = request("/api/v1/diagnostics/developer", {"seconds": 2, "confirmation": "AUTHORIZE_DIAGNOSTICS"})
        assert lease["enabled"]
        capture = request("/api/v1/diagnostics/capture")
        records = capture.decode().splitlines()
        assert json.loads(records[0])["format"] == "nibe-gateway/capture-jsonl"
        assert any(json.loads(line).get("dir") == "rx" for line in records[1:])
        time.sleep(2.1)
        assert not request("/api/v1/diagnostics/developer")["enabled"]
        request("/api/v1/diagnostics/capture", expected=403)
        # Verbose core events actually appear during the lease, and stop after
        # the reload task's one-second expiry tick. Only numeric/enum fields log.
        time.sleep(1.1)
        verbose_count = (root / "gateway.log").read_text().count("device poll completed")
        assert verbose_count > 0, "diagnostic lease did not enable verbose events"
        time.sleep(1.1)
        assert (root / "gateway.log").read_text().count("device poll completed") == verbose_count
        report = json.dumps(request("/api/v1/diagnostics/support"))
        config = json.loads((root / "state/config.json").read_text())
        secrets = json.loads((root / "state/setup-secrets.json").read_text())
        for secret in [PASSWORD, token, config["admin_password_hash"], *secrets.values(), *(cookie.value for cookie in jar)]:
            if isinstance(secret, str): assert secret not in report, "Support export leaked a canary"
        original_config = json.loads((root / "state/config.json").read_text())
        initial_health = request("/health")
        status = request("/api/v1/status")
        assert isinstance(status["uptime_s"], int) and status["ip_addresses"] == ["127.0.0.1"]
        wait(lambda: bool(request("/api/v1/registers/explorer")["verification_overlay"]))
        overlay = json.loads((root / "state/register-verification.json").read_text())
        assert all(entry["source"].startswith("replay:") for entry in overlay.values())
        staged = request("/api/v1/updates/upload", artifacts[1].read_bytes())
        assert staged["state"]["phase"] == "staged"
        request("/api/v1/updates/activate", {}, expected=202)
        wait(lambda: phase() == "probation")
        assert request("/health")["process_instance"] != initial_health["process_instance"]
        time.sleep(.5)
        assert phase() == "probation", "candidate completed before its configured window"
        wait(lambda: phase() == "complete")
        request("/api/v1/updates/status", expected=401)  # Old session dies with the process.
        login()
        assert request("/api/v1/updates/status")["state"]["active"] == "b"
        assert b"release 1" in request("/")
        request("/api/v1/updates/rollback", {}, expected=202)
        wait(lambda: phase() == "rolled_back")
        login()
        assert request("/api/v1/updates/status")["state"]["active"] == "a"
        assert b"release 0" in request("/")
        request("/api/v1/updates/rollback", {}, expected=409)
        migrated = json.loads((root / "state/config.json").read_text())
        assert migrated == {**original_config, "schema_version": 3, "min_reader_version": 2, "test_v3_additive": {"preserve": True}}
        assert request("/health")["ready"], "old release must read additive v3 after manual rollback"
        assert request("/api/v1/registers/explorer")["verification_overlay"] == overlay
        # Repeated candidate process restarts during probation trigger rollback.
        request("/api/v1/updates/upload", artifacts[1].read_bytes())
        request("/api/v1/updates/activate", {}, expected=202)
        wait(lambda: phase() == "probation")
        for _ in range(2):
            before = request("/health")["process_instance"]
            subprocess.run([str(helper)], check=True)
            wait(lambda: request("/health")["ready"] and request("/health")["process_instance"] != before)
            time.sleep(.6)
        wait(lambda: phase() == "rolled_back")
        assert json.loads((state / "state.json").read_text())["restart_count"] == 2
        login()
        # Signed older releases need an explicit flag; rejection leaves slots intact.
        request("/api/v1/updates/upload", artifacts[3].read_bytes(), expected=409)
        request("/api/v1/updates/upload?confirm_downgrade=true", artifacts[3].read_bytes())
        request("/api/v1/updates/upload", artifacts[2].read_bytes())
        request("/api/v1/updates/activate", {}, expected=202)
        wait(lambda: phase() == "rolled_back", seconds=45)
        login()
        assert request("/health")["ready"]
        assert json.loads((root / "state/config.json").read_text()) == migrated
        assert request("/api/v1/registers/explorer")["verification_overlay"] == overlay
        assert (root / "current").readlink() == Path("releases/slot-a")
        print("PASS: real HTTP/auth + signed staging + process restart/readiness + manual/automatic rollback + expiring diagnostics/capture + sanitized support + v3 migration manual/automatic rollback + probation/restart guard + downgrade confirmation + persisted register evidence + security headers + status uptime/IP")
    finally:
        if daemon:
            daemon.terminate()
            daemon.wait(timeout=5)
        if pidfile.exists():
            try: os.kill(int(pidfile.read_text()), signal.SIGTERM)
            except ProcessLookupError: pass
