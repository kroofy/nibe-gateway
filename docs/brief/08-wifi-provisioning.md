# First-boot Wi-Fi provisioning

> Source: ORIGINAL_BRIEF.md §10

## 10. First-boot Wi-Fi provisioning

On first boot without valid network configuration:

1. Start a temporary AP with a unique SSID such as `NIBE-Gateway-A4F2` and unique secure setup credentials.
2. Present a captive portal or known setup URL to connected phone/computer.
3. Scan networks and accept SSID/password.
4. Attempt station-mode connection and verify success.
5. Advertise on LAN using mDNS (`nibe-gateway.local`), then finish admin/device setup.

NetworkManager owns network operations; Rust calls an abstract network interface backed by D-Bus. Consider Pi Zero WH's single Wi-Fi radio when transitioning from AP to station mode; the setup connection may drop. Make setup recoverable and network changes transactional where possible. Never automatically expose an unsecured AP on ordinary Wi-Fi outages. Require an explicit recovery/factory-reset mechanism. Keep Wi-Fi credentials protected.
