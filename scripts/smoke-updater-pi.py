#!/usr/bin/env python3
"""Verify a real signed artifact with the ARMv6 updater on QEMU ARM1176.
Does not restart a service or touch the host filesystem outside a temp root.
"""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
if len(sys.argv) != 3:
    raise SystemExit("usage: smoke-updater-pi.py PI_UPDATER RASPBIAN_SYSROOT")
binary, sysroot = [str(Path(arg).resolve()) for arg in sys.argv[1:]]
subprocess.run(["cargo", "build", "-p", "nibe-updater", "--bins"], cwd=repo, check=True)
target = Path(os.environ.get("CARGO_TARGET_DIR", repo / "target")) / "debug"
with tempfile.TemporaryDirectory(prefix="nibe-updater-arm1176-") as tmp:
    root = Path(tmp)
    admin = root / "admin"
    admin.mkdir()
    (admin / "index.html").write_text("signed fixture UI")
    subprocess.run([str(target / "nibe-release"), "keygen", str(root / "private"), str(root / "public")], check=True)
    artifact = root / "release.tar"
    subprocess.run([str(target / "nibe-release"), "sign", str(root / "private"), "0.1.0", "arm-unknown-linux-gnueabihf", binary, str(admin), str(artifact)], check=True)
    slot = root / "releases/slot-a"
    slot.mkdir(parents=True)
    with tarfile.open(artifact) as archive:
        (slot / "manifest.json").write_bytes(archive.extractfile("manifest.json").read())
        with tarfile.open(fileobj=io.BytesIO(archive.extractfile("payload.tar").read())) as payload:
            payload.extractall(slot, filter="data")
    (root / "current").symlink_to("releases/slot-a")
    (root / "update-state").mkdir()
    (root / "update-state/state.json").write_text(json.dumps(dict(phase="idle", active="a", previous=None, candidate=None, version=None, outcome=None)))
    socket = root / "control.sock"
    qemu = shutil.which("qemu-arm-static") or shutil.which("qemu-arm")
    assert qemu, "install qemu-user-static"
    process = subprocess.Popen([qemu, "-cpu", "arm1176", "-L", sysroot, binary, "serve", "--root", str(root), "--key", str(root / "public"), "--socket", str(socket)])
    try:
        for _ in range(100):
            if socket.exists(): break
            assert process.poll() is None, "ARM1176 updater exited"
            time.sleep(.05)
        result = subprocess.run([str(target / "nibe-updater"), "client", "upload", str(artifact), "--socket", str(socket)], check=True, capture_output=True, text=True)
        assert json.loads(result.stdout)["status"]["phase"] == "staged"
        assert (root / "releases/slot-b/nibe-gateway").read_bytes() == Path(binary).read_bytes()
        assert (root / "current").readlink() == Path("releases/slot-a")
        print("PASS ARM1176: Ed25519/SHA256 verification, signed tar parsing and durable inactive-slot staging")
    finally:
        process.terminate()
        process.wait(timeout=5)
