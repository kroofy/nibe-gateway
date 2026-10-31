#!/usr/bin/env python3
"""Fixed-duration simulated/replay soak, RSS gates, SSE and signed update cycles.

Linux host test only. Default RSS budget leaves half of a 512 MiB Pi for the OS,
NM, page cache and tmpfs. It does not establish ARM CPU or physical UART timing.
The update worker reuses the real signed A/B HTTP smoke (never host systemd).
Load stops at the deadline; an in-flight update may drain for up to 120 seconds.
"""
import argparse
import http.cookiejar
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
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def rss_kib(pid):
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    except (FileNotFoundError, ProcessLookupError):
        pass
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration-seconds", type=int, default=180)
    parser.add_argument("--update-interval-seconds", type=int, default=60)
    parser.add_argument("--max-total-rss-mib", type=int, default=256)
    parser.add_argument("--max-gateway-rss-mib", type=int, default=96)
    parser.add_argument("--max-growth-mib", type=int, default=16)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if min(args.duration_seconds, args.update_interval_seconds, args.max_total_rss_mib,
           args.max_gateway_rss_mib, args.max_growth_mib) <= 0:
        parser.error("duration, cadence and thresholds must be positive")
    subprocess.run(["cargo", "build", "-p", "nibe-gateway", "-p", "nibe-updater", "--bins"], cwd=ROOT, check=True)
    binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug/nibe-gateway"
    processes, readers, responses, logs = [], [], [], []
    stop = threading.Event()
    errors, samples, cycles = [], [], []
    sse_lines = {}
    with tempfile.TemporaryDirectory(prefix="nibe-soak-") as tmp:
        work = Path(tmp)
        updates = work / "updates"
        updates.mkdir()
        clients = []
        try:
            for mode in ("simulated", "replay"):
                with socket.socket() as reserve:
                    reserve.bind(("127.0.0.1", 0))
                    port = reserve.getsockname()[1]
                state = work / mode
                log = (work / f"{mode}.log").open("w+")
                logs.append(log)
                process = subprocess.Popen([str(binary), "--device", mode, "--network", "simulated",
                                            "--bind", f"127.0.0.1:{port}", "--data-dir", str(state)],
                                           stdout=log, stderr=log)
                processes.append(process)
                base = f"http://127.0.0.1:{port}"
                client = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                           urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
                end = time.monotonic() + 20
                while True:
                    try:
                        with client.open(base + "/health", timeout=2) as response:
                            if json.load(response)["ready"]:
                                break
                    except (OSError, ValueError):
                        pass
                    if process.poll() is not None or time.monotonic() > end:
                        raise RuntimeError(f"{mode} did not become ready")
                    time.sleep(.1)
                token = json.loads((state / "setup-secrets.json").read_text())["setup_token"]
                setup = urllib.request.Request(base + "/api/v1/setup/admin",
                        data=json.dumps(dict(password="local-soak-only-password")).encode(),
                        headers={"X-Requested-With": "nibe-gateway", "X-Setup-Token": token,
                                 "Content-Type": "application/json"})
                with client.open(setup, timeout=15) as response:
                    assert json.load(response)["authenticated"]
                clients.append((mode, base, client))
                # One consumer drains, another stays connected without reading.
                for consume in (True, False):
                    response = client.open(base + "/api/v1/events", timeout=20)
                    responses.append(response)
                    if consume:
                        sse_lines[mode] = 0
                        def drain(stream=response, name=mode):
                            try:
                                while not stop.is_set():
                                    if not stream.readline():
                                        if not stop.is_set():
                                            errors.append(f"{name} SSE stream ended early")
                                        return
                                    sse_lines[name] += 1
                            except Exception as error:
                                if not stop.is_set():
                                    errors.append(f"{name} SSE consumer failed: {error}")
                        thread = threading.Thread(target=drain, daemon=True)
                        thread.start()
                        readers.append(thread)

            started = time.monotonic()
            deadline = started + args.duration_seconds

            def update_worker():
                next_cycle = started
                while not stop.is_set() and time.monotonic() < deadline:
                    if stop.wait(max(0, next_cycle - time.monotonic())):
                        return
                    if time.monotonic() >= deadline:
                        return
                    cycle = len(cycles) + 1
                    cycle_start = time.monotonic()
                    with (work / f"update-{cycle}.log").open("w+") as log:
                        try:
                            result = subprocess.run([sys.executable, str(ROOT / "tests/integration/ws07-update-smoke.py")],
                                                    cwd=ROOT, env={**os.environ, "TMPDIR": str(updates)},
                                                    stdout=log, stderr=log, timeout=120)
                            cycles.append(dict(cycle=cycle, seconds=round(time.monotonic() - cycle_start, 3),
                                               passed=result.returncode == 0))
                            if result.returncode:
                                log.seek(0)
                                errors.append("update cycle failed: " + log.read()[-6000:])
                                return
                        except subprocess.TimeoutExpired:
                            errors.append("update cycle exceeded 120 seconds")
                            return
                    next_cycle = max(next_cycle + args.update_interval_seconds, time.monotonic())

            worker = threading.Thread(target=update_worker, daemon=True)
            worker.start()
            baselines = {}
            while time.monotonic() < deadline:
                elapsed = time.monotonic() - started
                gateway_rss = {mode: rss_kib(p.pid) for (mode, _, _), p in zip(clients, processes)}
                # Update helpers can reparent daemonized gateway children; match
                # this run's unique temp-root argument rather than parent PID.
                update_rss = 0
                for entry in Path("/proc").iterdir():
                    if entry.name.isdigit():
                        try:
                            command = (entry / "cmdline").read_bytes()
                            if str(updates).encode() in command:
                                update_rss += rss_kib(int(entry.name))
                        except (FileNotFoundError, ProcessLookupError, PermissionError):
                            pass
                total = sum(gateway_rss.values()) + update_rss
                samples.append(dict(seconds=round(elapsed, 3), gateway_rss_kib=gateway_rss,
                                    update_rss_kib=update_rss, total_rss_kib=total))
                if total > args.max_total_rss_mib * 1024:
                    errors.append("combined gateway/updater RSS exceeds budget")
                for mode, amount in gateway_rss.items():
                    if amount > args.max_gateway_rss_mib * 1024:
                        errors.append(f"{mode} RSS exceeds budget")
                    if elapsed >= min(30, args.duration_seconds / 3):
                        baseline = baselines.setdefault(mode, amount)
                        if amount - baseline > args.max_growth_mib * 1024:
                            errors.append(f"{mode} post-warmup RSS growth exceeds budget")
                for (mode, base, client), process in zip(clients, processes):
                    if process.poll() is not None:
                        raise RuntimeError(f"{mode} exited during soak")
                    with client.open(base + "/health", timeout=3) as response:
                        assert json.load(response)["ready"]
                    with client.open(base + "/api/v1/status", timeout=3) as response:
                        status = json.load(response)
                        assert status["mode"] == mode and not status["controls_enabled"]
                if errors:
                    break
                stop.wait(min(1, max(0, deadline - time.monotonic())))
            load_seconds = round(time.monotonic() - started, 3)
            stop.set()
            worker.join(timeout=125)
            if worker.is_alive():
                errors.append("update worker did not drain")
            if len(cycles) < 2:
                errors.append("fewer than two complete signed-update cycles; increase duration")
            if load_seconds < args.duration_seconds - .1:
                errors.append("fixed load duration not completed")
            report = dict(passed=not errors, duration_seconds=load_seconds, modes=[c[0] for c in clients],
                          sse_clients_per_mode=2, sse_lines_received=sse_lines.copy(), signed_update_cycles=cycles, errors=errors,
                          post_warmup_growth_kib={m: max(0, max(s["gateway_rss_kib"][m] for s in samples
                              if s["seconds"] >= min(30, args.duration_seconds / 3)) - baseline)
                              for m, baseline in baselines.items()},
                          thresholds=dict(total_rss_mib=args.max_total_rss_mib, gateway_rss_mib=args.max_gateway_rss_mib,
                                          growth_mib=args.max_growth_mib),
                          peak_total_rss_kib=max(s["total_rss_kib"] for s in samples),
                          peak_gateway_rss_kib={m: max(s["gateway_rss_kib"][m] for s in samples) for m, _, _ in clients},
                          samples=samples, evidence="host Linux, synthetic replay, debug executables; ARM timing and Pi RSS unverified")
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps({k: v for k, v in report.items() if k != "samples"}, indent=2))
            return 0 if report["passed"] else 1
        finally:
            stop.set()
            # Stop servers before closing HTTPResponse from another thread.
            # Concurrent close/readline can race in Python's chunk decoder.
            for process in processes:
                if process.poll() is None:
                    process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            for reader in readers:
                reader.join(timeout=5)
            for response in responses:
                response.close()
            for log in logs:
                log.close()


if __name__ == "__main__":
    sys.exit(main())
