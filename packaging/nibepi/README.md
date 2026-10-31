# NibePi OS fallback (Session C)

Only install after Session A and the approved longer read-only trial. These
files are **not** part of the new appliance image. Inventory the actual legacy
unit in A1; `nodered.service` is an example, not a verified name.

While the NibePi root is deliberately writable for installation:

```sh
sudo install -d /usr/lib/nibe-gateway /etc/nibe-gateway
sudo install -m755 packaging/nibepi/fallback.py /usr/lib/nibe-gateway/fallback.py
sudo install -m644 packaging/nibepi/nibe-fallback.{service,timer} /etc/systemd/system/
sudo install -m600 packaging/nibepi/fallback.env.example /etc/nibe-gateway/fallback.env
# Edit fallback.env with the A1 unit and the trial's loopback health port.
# Keep NibePi enabled and the gateway disabled at boot: a reboot returns to NibePi.
sudo systemctl disable nibe-gateway.service
sudo systemctl enable <inventoried-nibepi-unit>
sudo systemctl daemon-reload
sudo systemctl enable nibe-fallback.timer
```

Remount the NibePi root read-only using its existing procedure. After the trial
service has received its first MODBUS40 frame, arm `systemctl start
nibe-fallback.timer`. It probes every 5 seconds (up to 1 second timer slack).
Missing/invalid health, non-ready/recovering/fatal wire state, or a null frame age
triggers fallback immediately; age >= 60 seconds triggers fallback even if HTTP
is otherwise healthy. Only checksum-valid address-0x20 frames refresh this age.
The timeout is configurable in the root-owned environment file; 60 seconds is a
hardware hypothesis, and log timestamps are not physical timing measurements.

Fallback **waits for gateway stop to succeed before starting NibePi**. On stop
failure it exits nonzero and the next timer tick retries; it never starts a
second UART owner. On success a private `/run/nibe-fallback/latched` prevents
repeated legacy restarts. No remount, SD writes, or enable/disable operation is
needed during fallback. A reboot clears the latch and starts the still-enabled
legacy service. The 90-second boot delay gives legacy startup time; it does not
authorize running the gateway automatically alongside it.

To re-arm after investigating a fault: stop the timer, verify the gateway is
stopped, stop NibePi, start the gateway, confirm ACKs and `/health`, remove only
`/run/nibe-fallback/latched`, then start the timer. Maintain the independent
Session A dead-man timer for short trials. UART exclusivity is mandatory.

Local checks: `python3 -B -m unittest discover -s scripts/tests -v` and
`python3 tests/integration/ws09-runtime-smoke.py`. They use a fake systemctl and
PTY only. Real unit teardown, legacy recovery and alarms remain HP-24.
