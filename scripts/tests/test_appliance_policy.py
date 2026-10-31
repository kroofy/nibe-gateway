"""Lint the NibePi fallback units in a synthetic filesystem and execute the appliance's Polkit rule."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class AppliancePolicyTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("systemd-analyze"), "systemd-analyze unavailable")
    def test_unit_lint(self):
        with tempfile.TemporaryDirectory(prefix="nibe-unit-lint-") as tmp:
            root = Path(tmp)
            units = root / "etc/systemd/system"
            units.mkdir(parents=True)
            names = []
            for relative in ("packaging/nibepi/nibe-fallback.service", "packaging/nibepi/nibe-fallback.timer"):
                source = ROOT / relative
                shutil.copy(source, units)
                names.append(source.name)
            # Stubs satisfy verify's executable-path check, not runtime QA.
            for binary in ("usr/bin/python3", "bin/true"):
                target = root / binary
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("#!/bin/sh\nexit 0\n")
                target.chmod(0o755)
            for target in ("sysinit", "basic", "shutdown", "network", "local-fs", "timers", "multi-user"):
                (units / f"{target}.target").write_text("[Unit]\nDefaultDependencies=no\n")
            for service in ("NetworkManager", "avahi-daemon"):
                (units / f"{service}.service").write_text("[Service]\nExecStart=/bin/true\n")
            subprocess.run(["systemd-analyze", "verify", "--man=no", "--generators=no", f"--root={root}", *names],
                           check=True, capture_output=True, text=True)

    @unittest.skipUnless(shutil.which("node"), "Node needed to execute JavaScript rule")
    def test_polkit_grants_only_allowlisted_actions_to_service_account(self):
        rule = ROOT / "packaging/common/49-nibe-gateway-network.rules"
        program = r'''
const vm = require('node:vm'), fs = require('node:fs'), assert = require('node:assert/strict');
let check;
vm.runInNewContext(fs.readFileSync(process.argv[1], 'utf8'), {polkit: {
 Result: {YES:'yes', NO:'no', NOT_HANDLED:'pass'}, addRule: fn => check=fn }});
const action = suffix => ({id:'org.freedesktop.NetworkManager.'+suffix});
for (const id of ['network-control','wifi.scan','wifi.share.open','wifi.share.protected','settings.modify.own','checkpoint-rollback']) {
 assert.equal(check(action(id), {user:'nibe-gateway'}), 'yes');
 assert.equal(check(action(id), {user:'unrelated'}), 'pass');
}
for (const id of ['settings.modify.system','settings.modify.hostname','enable-disable-wifi'])
 assert.equal(check(action(id), {user:'nibe-gateway'}), 'no');
assert.equal(check({id:'org.freedesktop.login1.reboot'}, {user:'nibe-gateway'}), undefined);
'''
        subprocess.run(["node", "-e", program, str(rule)], check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
