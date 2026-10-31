# Provisioning backends

`SimulatedNetwork` never touches the host network. `network-manager` enables the
zbus system-bus implementation; the gateway also needs `--network network-manager`
to use it with the simulated pump for an isolated bench test. The selected
interface defaults to wlan0. SSIDs must be UTF-8, 1–32 bytes. Initial hardware
scope supports open station networks and WPA2 personal; enterprise/WEP are not
supported. The setup and recovery AP is open (ADR 0130).
Never test this backend on the remote owner's active network interface.

The API contract is in `docs/progress/WS06-network-auth-persistence.md`.
Provisioner serializes operations, verifies activation, saves only successful
connections, rolls back bad passwords, and never opens AP after station outage.
If NetworkManager refuses the AP (first boot, factory reset, failed setup
connect), the gateway stays up, reports `offline` with `last_error`, and retries
that AP every 30 s doubling to 5 min (ADR 0130 amendment).
Checkpoints bound rollback even if the process is interrupted. The app and NM
stores cannot participate in a single filesystem transaction (ADR 0060).

## System integration (bench/image maintainer)

Use a gateway-owned 0700 state directory on the persistent partition, and persist
NetworkManager's root-owned keyfiles/state separately, as WS03 already prepares.
The service runs as nibe-gateway with the narrow polkit allow list in
`packaging/common/49-nibe-gateway-network.rules` (every other NetworkManager
action is denied). The open AP needs `wifi.share.open`; a unit test ties the AP's
security mode to that list. Do not grant the service unrestricted D-Bus access
or change the remote development host network.
NM shared IPv4 uses dnsmasq-base, supplied by the image. Choose the actual radio
country using authorized local regulatory settings before enabling the AP.

`resources/nibe-gateway.service` is the Avahi advertisement template for port
8080. Once an authenticated LAN listener is deliberately enabled, install it
with `install -m644 crates/network/resources/nibe-gateway.service
/etc/avahi/services/nibe-gateway.service` and reload avahi-daemon; configure the
appliance hostname `nibe-gateway` and check `avahi-resolve-host-name
nibe-gateway.local` from a second LAN machine. Adjust the service port if the
listener port changes. The image's WS03 disabled template remains disabled for
its simulator/loopback factory configuration, which is not a reachable LAN
service. Do not enable mDNS on the developer host as part of local verification.

If Wi-Fi is unavailable, an authorized local maintenance operator can restart
with `--recover-network` plus the same data directory and `--network
network-manager`; join the open `NIBE-Gateway-XXXX` AP and log in with the
existing admin password. No default admin password or automatic GPIO reset
exists. The authenticated factory-reset endpoint requires `ERASE`, rotates the
setup token and reopens the open setup AP.
