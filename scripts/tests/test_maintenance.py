"""Host-side tooling that outlived the Raspberry Pi OS image: stripping the pinned Tailscale release
(packaging/alpine/tailscale) and the zstd OS-install stream in scripts/ssh-maintenance.sh."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]


class StripTailscaleTest(unittest.TestCase):
    SCRIPT = REPO / "packaging/alpine/tailscale/strip-tailscale.sh"

    def test_rejects_non_arm_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name in ("tailscale", "tailscaled"):
                (Path(tmp) / name).write_text("not an ELF")
            result = subprocess.run(["bash", str(self.SCRIPT), tmp], capture_output=True, text=True,
                                    env={**os.environ, "NIBE_STRIP": "true"})
            self.assertNotEqual(result.returncode, 0)

    def test_strips_pinned_release_and_binaries_still_run(self):
        lock = dict(line.split("=", 1) for line in (REPO / "packaging/alpine/tailscale/tailscale.lock").read_text().split())
        cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "nibe-gateway" / f"tailscale_{lock['TAILSCALE_VERSION']}_arm.tgz"
        if not cache.is_file() or not (shutil.which("llvm-strip") or shutil.which("arm-linux-gnueabihf-strip")):
            self.skipTest("pinned Tailscale tarball or strip tool not available")
        with tempfile.TemporaryDirectory() as tmp:
            prefix = f"tailscale_{lock['TAILSCALE_VERSION']}_arm"
            subprocess.run(["tar", "-xzf", str(cache), "-C", tmp, "--strip-components=1",
                            f"{prefix}/tailscale", f"{prefix}/tailscaled"], check=True)
            before = {n: (Path(tmp) / n).stat().st_size for n in ("tailscale", "tailscaled")}
            subprocess.run(["bash", str(self.SCRIPT), tmp], check=True, capture_output=True)
            for name in ("tailscale", "tailscaled"):
                path = Path(tmp) / name
                self.assertLess(path.stat().st_size, before[name] * 0.8)
                self.assertTrue(os.access(path, os.X_OK))
                sections = subprocess.run(["readelf", "-S", "-W", str(path)], capture_output=True, text=True).stdout
                self.assertNotIn(".debug_", sections)
                if shutil.which("qemu-arm-static"):
                    out = subprocess.run(["qemu-arm-static", str(path), "version" if name == "tailscale" else "--version"],
                                         capture_output=True, text=True, timeout=60).stdout
                    self.assertTrue(out.startswith(lock["TAILSCALE_VERSION"]), out)
            sums = (Path(tmp) / "SHA256SUMS").read_text().split()
            self.assertEqual(sums[1::2], ["tailscale", "tailscaled"])


class OsInstallPipeTest(unittest.TestCase):
    """scripts/ssh-maintenance.sh nibe_os_install: zstd -19 on the host, zstd -dc | os-install on the Pi."""

    def run_install(self, remote_status=0, level=None):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            bin_dir = work / "bin"
            bin_dir.mkdir()
            (bin_dir / "zstd").write_text('#!/bin/bash\necho "zstd $*" >> "$NIBE_TEST_LOG"\ncat "${@: -1}"\n')
            (bin_dir / "ssh").write_text('#!/bin/bash\necho "ssh ${@: -1}" >> "$NIBE_TEST_LOG"\n'
                                         'cat > "$NIBE_TEST_STDIN"\nexit "$NIBE_TEST_REMOTE"\n')
            for tool in bin_dir.iterdir():
                tool.chmod(0o755)
            bundle = work / "nibe-os.tar"
            bundle.write_bytes(b"bundle bytes")
            log = work / "log"
            env = {**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}", "NIBE_SSH_HOST": "nibe-admin@192.0.2.1",
                   "NIBE_TEST_LOG": str(log), "NIBE_TEST_STDIN": str(work / "stdin"), "NIBE_TEST_REMOTE": str(remote_status)}
            if level:
                env["NIBE_OS_ZSTD_LEVEL"] = level
            result = subprocess.run(["bash", "-c", f'source "{REPO}/scripts/ssh-maintenance.sh" && nibe_os_install "$1"', "_",
                                     str(bundle)], env=env, capture_output=True, text=True)
            return result, (log.read_text() if log.exists() else ""), \
                ((work / "stdin").read_bytes() if (work / "stdin").exists() else b"")

    def test_streams_bundle_with_zstd_19_into_os_install(self):
        result, log, sent = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("zstd -q -T0 -19 -c --", log)
        self.assertIn("ssh zstd -dc | sudo -n /usr/lib/nibe-gateway/nibe-updater os-install /dev/stdin", log)
        self.assertEqual(sent, b"bundle bytes")

    def test_remote_failure_and_bad_level_are_errors(self):
        result, _, _ = self.run_install(remote_status=1)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("os-install failed", result.stderr)
        result, log, _ = self.run_install(level="22")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(log, "")


if __name__ == "__main__":
    unittest.main()
