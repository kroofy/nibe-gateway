"""Exercise the real supervisor (`nibe-system supervise`) with HTTP and Unix datagram sockets."""
import http.server
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

REPO = Path(__file__).resolve().parents[2]


def supervisor():
    """`nibe-system supervise` (ADR 0117 replaced supervise.py); built on demand for this test."""
    binary = Path(os.environ.get("NIBE_SYSTEM_BIN", REPO / "target/debug/nibe-system"))
    if "NIBE_SYSTEM_BIN" not in os.environ:
        subprocess.run(["cargo", "build", "-q", "-p", "nibe-system"], cwd=REPO, check=True)
    return [str(binary), "supervise"]


class SupervisorTest(unittest.TestCase):
    def test_readiness_watchdog_and_signal_forwarding(self):
        state = {"healthy": False}

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200 if state["healthy"] else 503)
                self.end_headers()
                self.wfile.write(json.dumps({"status": "ok", "ready": state["healthy"]}).encode())

            def log_message(self, *_):
                pass

        with tempfile.TemporaryDirectory(prefix="nibe-notify-") as directory:
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as receiver:
                address = str(Path(directory) / "notify")
                receiver.bind(address)
                receiver.settimeout(2)
                process = subprocess.Popen(
                    [*supervisor(), sys.executable, "-c",
                     "import time; time.sleep(120)"],
                    env={**os.environ, "NOTIFY_SOCKET": address, "WATCHDOG_USEC": "600000",
                         "NIBE_HEALTH_URL": f"http://127.0.0.1:{server.server_port}/health"},
                )
                try:
                    self.assertIn(b"Waiting", receiver.recv(4096))
                    state["healthy"] = True
                    messages = [receiver.recv(4096), receiver.recv(4096)]
                    self.assertTrue(any(b"READY=1" in msg for msg in messages))
                    self.assertTrue(any(b"WATCHDOG=1" in msg for msg in messages))
                    state["healthy"] = False
                    # Drain any successful probe already in flight; the next two
                    # failed probes must not refresh the watchdog.
                    while b"Waiting" not in receiver.recv(4096):
                        pass
                    self.assertIn(b"Waiting", receiver.recv(4096))
                    process.send_signal(signal.SIGTERM)
                    self.assertEqual(process.wait(timeout=3), 0)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
                    server.shutdown()
                    server.server_close()
                    thread.join()

    def test_own_watchdog_without_notify_socket(self):
        """OpenRC (ADR 0117): no NOTIFY_SOCKET, so the supervisor enforces the watchdog itself."""
        state = {"healthy": False}

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200 if state["healthy"] else 503)
                self.end_headers()
                self.wfile.write(json.dumps({"status": "ok", "ready": state["healthy"]}).encode())

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env = {key: value for key, value in os.environ.items()
               if key not in ("NOTIFY_SOCKET", "WATCHDOG_USEC")}
        env.update(NIBE_HEALTH_URL=f"http://127.0.0.1:{server.server_port}/health",
                   NIBE_START_TIMEOUT_SEC="1.5", NIBE_WATCHDOG_SEC="0.6")
        command = [*supervisor(), sys.executable, "-c", "import time; time.sleep(120)"]
        try:
            # Never healthy: stopped once the start timeout passes.
            started = time.monotonic()
            result = subprocess.run(command, env=env, timeout=15, stderr=subprocess.PIPE)
            self.assertEqual(result.returncode, 1)
            self.assertGreaterEqual(time.monotonic() - started, 1.4)
            self.assertIn(b"watchdog: gateway not healthy after start", result.stderr)
            # Healthy, then failing: stopped about NIBE_WATCHDOG_SEC after the last good probe.
            state["healthy"] = True
            process = subprocess.Popen(command, env=env, stderr=subprocess.PIPE)
            try:
                time.sleep(2.5)
                self.assertIsNone(process.poll(), "healthy beyond the start timeout keeps running")
                state["healthy"] = False
                self.assertEqual(process.wait(timeout=10), 1)
                self.assertIn(b"watchdog: gateway unhealthy", process.stderr.read())
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                process.stderr.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_child_exit_is_always_failure_unless_stopping(self):
        for status, expected in ((0, 1), (7, 7), (-signal.SIGTERM, 143)):
            code = (f"raise SystemExit({status})" if status >= 0 else
                    "import os,signal; os.kill(os.getpid(), signal.SIGTERM)")
            result = subprocess.run(
                [*supervisor(), sys.executable, "-c", code],
                env={key: value for key, value in os.environ.items() if key != "NOTIFY_SOCKET"},
                timeout=5,
            )
            self.assertEqual(result.returncode, expected)


if __name__ == "__main__":
    unittest.main()
