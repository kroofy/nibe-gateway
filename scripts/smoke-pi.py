#!/usr/bin/env python3
"""Run the actual release gateway's simulator on an emulated ARM1176 CPU."""
import http.cookiejar
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main(binary, sysroot):
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    with tempfile.TemporaryFile(mode="w+") as log, tempfile.TemporaryDirectory(prefix="nibe-arm-auth-") as state:
        process = subprocess.Popen(
            ["qemu-arm-static", "-cpu", "arm1176", "-L", str(Path(sysroot).resolve()),
             str(Path(binary).resolve()), "--device", "simulated", "--data-dir", state, "--bind", f"127.0.0.1:{port}"],
            stdout=log, stderr=log, env={**os.environ, "RUST_LOG": "info"},
        )
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"gateway exited with status {process.returncode}")
                try:
                    with opener.open(f"http://127.0.0.1:{port}/health", timeout=2) as response:
                        assert response.status == 200
                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(0.2)
            else:
                raise RuntimeError("gateway health did not become ready within 30s")
            proof = json.loads((Path(state) / "setup-secrets.json").read_text())["setup_token"]
            setup = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/setup/admin",
                data=json.dumps({"password": "arm1176-simulation-only"}).encode(),
                headers={"Content-Type": "application/json", "X-Requested-With": "nibe-gateway", "X-Setup-Token": proof})
            with opener.open(setup, timeout=60) as response:
                assert json.load(response)["authenticated"]
            with opener.open(f"http://127.0.0.1:{port}/api/v1/status", timeout=2) as response:
                status = json.load(response)
                assert status["mode"] == "simulated", status
                assert status["controls_enabled"] is False, status
            print("PASS ARM1176/QEMU: HTTP readiness, authenticated setup/status, controls disabled")
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=10) == 0, "gateway did not stop cleanly"
            print("PASS ARM1176/QEMU: graceful SIGTERM")
        except BaseException:
            log.seek(0)
            sys.stderr.write(log.read())
            raise
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: smoke-pi.py BINARY RASPBIAN_SYSROOT")
    main(*sys.argv[1:])
