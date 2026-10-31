#!/usr/bin/env python3
"""Session C only: hand UART ownership back to NibePi on gateway failure.

Run from the root oneshot/timer, never installed/enabled on the new image.
A successful fallback is latched. Explicitly re-arm after inspecting the fault.
"""
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.request


def healthy(url, maximum_age_ms):
    try:
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(url, timeout=2) as response:
            body = json.loads(response.read(16385))
        age = body.get("last_frame_age_ms")
        return (body.get("ready") is True and body.get("status") == "ok"
                and body.get("device_fatal") is False and body.get("wire_recovering") is False
                and type(age) in (int, float) and math.isfinite(age) and 0 <= age < maximum_age_ms)
    except (OSError, ValueError, urllib.error.URLError):
        return False


def main():
    gateway = os.environ.get("NIBE_GATEWAY_UNIT", "nibe-gateway.service")
    legacy = os.environ.get("NIBEPI_UNIT", "nodered.service")
    for unit in (gateway, legacy):
        if not re.fullmatch(r"[a-zA-Z0-9_.@-]+\.service", unit):
            raise ValueError("configure valid distinct .service unit names")
    if gateway == legacy:
        raise ValueError("gateway and NibePi units must differ")
    latch = Path(os.environ.get("NIBE_FALLBACK_LATCH", "/run/nibe-fallback/latched"))
    if latch.exists():
        return
    age_ms = int(os.environ.get("NIBE_FALLBACK_FRAME_TIMEOUT_MS", "60000"))
    if not 1000 <= age_ms <= 300000:
        raise ValueError("frame timeout must be 1000..300000 ms")
    if healthy(os.environ.get("NIBE_HEALTH_URL", "http://127.0.0.1:8099/health"), age_ms):
        return
    systemctl = os.environ.get("NIBE_SYSTEMCTL", "/bin/systemctl")
    # NibePi OS root may be read-only. Installation leaves legacy enabled and
    # gateway disabled at boot; fallback changes only runtime ownership.
    # A failed stop must never start a second bus master. systemctl waits for
    # complete unit teardown; an explicit stop cancels Restart=on-failure.
    subprocess.run([systemctl, "stop", gateway], check=True, timeout=30)
    subprocess.run([systemctl, "start", legacy], check=True, timeout=30)
    latch.parent.mkdir(parents=True, exist_ok=True)
    latch.touch(mode=0o600)
    print("Gateway unhealthy/stale; stopped gateway and restored NibePi; fallback latched")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(f"NibePi fallback failed (will retry): {error}")
