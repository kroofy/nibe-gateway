# Inventory 2026-10-24: bench Pi and RS485 HAT

The owner was remote. The Pi was disconnected from the F730 throughout and no frames were captured.
Labels: VERIFIED-HW (checked on the device), VERIFIED-SOURCE (manufacturer/owner document), UNVERIFIED.

## Bench Pi (`nibe-bench`)
| Item | Value | Label |
|---|---|---|
| Board | Raspberry Pi Zero W Rev 1.1, ARMv6 | VERIFIED-HW |
| OS | Raspberry Pi OS Lite armhf `2026-10-06-raspios-trixie-armhf-lite` (Raspbian 13 trixie), kernel `6.18.50+rpt-rpi-v6`, glibc 2.41 | VERIFIED-HW |
| Image changes | cloud-init `user-data`/`network-config`; `console=serial0` removed and `cfg80211.ieee80211_regdom=SE` added in `cmdline.txt`; `enable_uart=1` + `dtoverlay=disable-bt` in `config.txt`; Tailscale 1.104.2 static ARM tarball | VERIFIED-HW |
| Serial | `/dev/serial0 -> ttyAMA0` (PL011); GPIO14/15 = TXD0/RXD0, idle high; no process owns the UART; serial getty and Bluetooth/hciuart inactive; no serial console on the kernel command line | VERIFIED-HW |
| Power | `vcgencmd get_throttled` = `0x0` on the bench supply (root needed; as `pi` the device file is not readable) | VERIFIED-HW |
| HAT ID EEPROM | none exposed | VERIFIED-HW |
| Gateway | `~/nibe/releases/bench-20261024-87acf34e81ac` (binary sha256 `87acf34e…`, features `serial,network-manager`, built from `6d4a5ba`) | VERIFIED-HW |
| Simulated check | Ran simulated with the UART hidden (systemd `PrivateDevices`): `/health` ok/ready, UI 200, unauthenticated device API 401; service stopped afterwards | VERIFIED-HW |

This is stock Pi OS with the binary installed, **not** the pi-gen appliance image: the root is writable and grown to
the whole card, and there is no p3 data partition, read-only overlay, A/B updater layout or watchdog. HP-10…HP-19,
HP-22 and HP-23 remain open. The image targets Bookworm while this card runs Trixie; the binary needs glibc ≥ 2.28 only.

## RS485 HAT
The owner's board is a **Waveshare RS485 CAN HAT** (purchase link ASIN B09JKJCMHN), not the AB Electronics RS485 Pi
that NibePi's README links to. See [ADR 0040](../decisions/0040-wire-task.md).

| Item | Value | Label |
|---|---|---|
| RS485 | SP3485 transceiver on the Pi UART (GPIO14/15) | VERIFIED-SOURCE ([manual](https://files.waveshare.com/upload/2/29/RS485-CAN-HAT-user-manuakl-en.pdf)) |
| Direction | Automatic TX/RX switching in the factory configuration; GPIO4 (RSE) control only with different solder links | VERIFIED-SOURCE; links on the owner's board UNVERIFIED |
| CAN | MCP2515 (SPI) + 3.3 V CAN transceiver; unused by the gateway | VERIFIED-SOURCE |
| Power | 3.3 V from the Pi header; no 12 V input on the HAT | VERIFIED-SOURCE |
| Terminals | H, L (CAN), B, A (RS485); the F730 uses A/B only | VERIFIED-SOURCE |
| Board revision, 120 Ω termination, solder links | not inspected | UNVERIFIED |
| Pump-side wiring (A/B polarity, GND, 12 V supply to the Pi) | existing NibePi wiring, not inspected | UNVERIFIED |
| Pi power at the pump | Pimoroni Wide Input SHIM (3–16 V in, 5 V to the header) fed from the pump's 12 V, as in NibePi; the HAT gets 3.3 V from the Pi | Owner report 2026-10-24; stacking and wiring not photographed |

The gateway has no RS485 direction-control option and relies on automatic direction, which matches the factory
configuration. If the owner's links select GPIO4/RSE, stop: a direction option has to be added first
(research/hardware.md §4).

## Open
- Photos of both sides of the owner's board (revision, termination, solder links) and of the pump terminal wiring.
- Pi power at the pump: owner confirmed a Pimoroni Wide Input SHIM; check `get_throttled` under load at the pump (Wi-Fi TX brown-outs).
- The original NibePi card has not been imaged (HP-02); its flows and `/etc/nibepi` are needed to replicate features.
- Housekeeping after bench work: the card stores the Wi-Fi password in plain text (boot `network-config`, NetworkManager);
  remove `nibe-bench` from the tailnet and wipe the card when done.

## Owner's NibePi configuration (read 2026-10-24, before the card was reused)
A read-only copy was taken to read these settings, then deleted at the owner's request (it held MQTT credentials and a price-API token; never committed).
NibePi `system`: F730, firmware 9701 at the time, read-only root, serial `fSeries`. Polled registers: 40004, 47041, 48132.
Node-RED tabs: Start, Settings, SMHI, Indoor, Airflow, Price, Hotwater, RMU, Diagnostic, Graph, Plejd, Test.

| Feature | Owner setting | Gateway status |
|---|---|---|
| MQTT + Home Assistant discovery (topic `nibe/modbus/`) | enabled | missing |
| Hot water auto-luxury (diff 15, time 15) and hot-water priority | enabled | not ported by decision (ADR 0113); 48132 to become a plain writable control (HP-32) |
| Alarm 251 auto-reset | always on (code) | done, ADR 0105 |
| Price control (SE3, priceai), SMHI weather, indoor sensor, DM reset | disabled | not needed now |
| Plejd lighting flows | present | out of scope |

The owner reused the NibePi SD card for the appliance image on 2026-10-24 (HP-02 already waived), so there is no NibePi fallback any more; the bench card is the pump's fallback.

