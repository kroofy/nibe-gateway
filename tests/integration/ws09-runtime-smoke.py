#!/usr/bin/env python3
"""Synthetic PTY unplug/replug, public wire readiness and NibePi fallback.

No host systemd and no real UART. Failures are real serial I/O errors through
Linux PTYs; symlink retargeting models a device that can be reopened.
"""
import importlib.util
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
subprocess.run(["cargo", "build", "-p", "nibe-gateway", "--features", "serial"], check=True)
BINARY = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug/nibe-gateway"
client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
spec = importlib.util.spec_from_file_location("fallback", ROOT / "packaging/nibepi/fallback.py")
fallback = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fallback)

with tempfile.TemporaryDirectory(prefix="nibe-ws09-") as tmp:
    work = Path(tmp)
    link = work / "uart"
    master, slave = pty.openpty()
    link.symlink_to(os.ttyname(slave))
    os.close(slave)
    with socket.socket() as reserve:
        reserve.bind(("127.0.0.1", 0))
        port = reserve.getsockname()[1]
    url = f"http://127.0.0.1:{port}/health"
    config = work / "config.json"
    config.write_text(json.dumps(dict(device="hardware", serial=str(link), network="simulated",
                                     bind=f"127.0.0.1:{port}", data_dir=str(work / "state"),
                                     wire=dict(reopen_attempts=4, reopen_backoff_ms=100, thread_nice=0))))
    with (work / "log").open("w+") as log:
        process = subprocess.Popen([str(BINARY), "--config", str(config)], stdout=log, stderr=log)
        def health():
            try:
                response = client.open(url, timeout=1)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                return response.status, json.load(response)

        def wait(predicate, limit=10):
            end = time.monotonic() + limit
            while time.monotonic() < end:
                try:
                    value = predicate()
                    if value:
                        return value
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.02)
            raise AssertionError("condition timed out; " + (work / "log").read_text())

        def frame():
            os.write(master, bytes([92, 0, 32, 105, 0, 73]))
            assert select.select([master], [], [], 1)[0], "no ACK"
            assert os.read(master, 256) == b"\x06"

        try:
            wait(lambda: health()[0] == 200)
            assert not fallback.healthy(url, 60000), "no frame must not pass fallback health"
            frame()
            wait(lambda: health()[1]["last_frame_age_ms"] is not None)
            assert fallback.healthy(url, 60000)
            # Real serial EOF/EIO from unplug. Keep the port absent until recovery is visible.
            os.close(master)
            master = None
            link.unlink()
            wait(lambda: health()[0] == 503 and health()[1]["wire_recovering"])
            master, slave = pty.openpty()
            link.symlink_to(os.ttyname(slave))
            os.close(slave)
            time.sleep(.25)
            wait(lambda: frame() or True)
            wait(lambda: health()[0] == 200)
            assert health()[1]["wire_reopen_attempts"] >= 1
            assert not health()[1]["device_fatal"]
            # Frame-age watchdog against the actual PTY health, with a shortened test threshold.
            assert not fallback.healthy(url, 0)
            fake = work / "systemctl"
            fake.write_text('#!/usr/bin/env python3\nimport os,sys\nwith open(os.environ["CALL_LOG"],"a") as f:f.write(" ".join(sys.argv[1:])+"\\n")\n')
            fake.chmod(0o755)
            time.sleep(1.05)
            env = {**os.environ, "NIBE_SYSTEMCTL": str(fake), "CALL_LOG": str(work / "calls"),
                   "NIBE_FALLBACK_LATCH": str(work / "latch"), "NIBE_HEALTH_URL": url,
                   "NIBE_FALLBACK_FRAME_TIMEOUT_MS": "1000"}
            subprocess.run([sys.executable, str(ROOT / "packaging/nibepi/fallback.py")], env=env, check=True)
            assert (work / "calls").read_text().splitlines() == ["stop nibe-gateway.service", "start nodered.service"]
            # Permanent unplug exhausts a bounded retry budget and exits nonzero.
            os.close(master)
            master = None
            link.unlink()
            assert process.wait(timeout=10) != 0
            print("PASS WS09 PTY: startup/no-frame watchdog, UART EIO/EOF, 503 during reopen, recovered ACK/readiness, stale-frame fallback ordering, permanent unplug nonzero exit")
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                process.wait(timeout=10)
            if master is not None:
                os.close(master)
