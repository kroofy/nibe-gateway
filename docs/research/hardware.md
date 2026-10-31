# Hardware: Pi Zero WH + RS485 HAT + F730

WS01 / M0, 2026-10-22. Labels as in [protocol.md](protocol.md): VERIFIED-SOURCE, HYPOTHESIS. Nothing here has been
checked on the owner's hardware yet. The checks are in `docs/progress/HARDWARE_PENDING.md`.

## 1. What NibePi uses (the existing installation)
| Part | Detail | Label | Evidence |
|---|---|---|---|
| Computer | Raspberry Pi Zero W / WH (BCM2835, ARM1176JZF-S = **ARMv6** + VFP2, 512 MB, 2.4 GHz Wi-Fi + Bluetooth) | VERIFIED-SOURCE | NibePi README.en.md hardware list; brief 02 |
| RS-485 | "RS485 HAT", links to **AB Electronics UK "RS485 Pi"** (also sold by The Pi Hut / Kiwi) | VERIFIED-SOURCE | NibePi README.en.md links `abelectronics.co.uk/p/77/rs485-pi`, `thepihut.com/products/rs485-pizero` |
| Power | Pimoroni "Wide Input SHIM" (3–16 V in) fed from the pump's 12 V terminal | VERIFIED-SOURCE | NibePi README.en.md "12V HAT", "powered directly from the terminals" |
| Pump wiring | 4 wires: 12 V, A, B, GND on the pump's accessory terminal (location differs per model, see NIBE doc 031725-6) | VERIFIED-SOURCE | NibePi README.en.md install step 5, https://www.nibe.fi/nibedocuments/15050/031725-6.pdf |
| Serial device | `/dev/ttyAMA0`, 9600 8N1 | VERIFIED-SOURCE | nibepi default.json:9, backend.js:54 |
| OS | NibePi image 1.1.1 = Raspberry Pi OS **Bullseye** (glibc 2.31), Node.js + Node-RED, root FS mounted **read-only**, remounted rw to save config | VERIFIED-SOURCE | NibePi README.en.md ("imagefile running Raspberry Pi Bullseye"); index.js `mount -o remount,ro /` |
| Config | `/etc/nibepi/config.json` (serial port, registers, mqtt, log flags) | VERIFIED-SOURCE | nibepi index.js:53 `const path = "/etc/nibepi"` |

**Which HAT the owner actually has is not confirmed** (photo or `i2cdetect` won't tell; the AB board is passive).
The analysis below assumes the AB Electronics RS485 Pi; if it is a different board (e.g. Waveshare with a GPIO
direction pin) see §4.

**Update 2026-10-24:** the owner's board is a **Waveshare RS485 CAN HAT** (SP3485, automatic direction by default,
GPIO4/RSE only with other solder links); see [ADR 0040](../decisions/0040-wire-task.md) and
[inventory-2026-10-24.md](inventory-2026-10-24.md). §2 below describes the AB board NibePi documents, not the owner's.

## 2. AB Electronics RS485 Pi (schematic v2.0)
Source: product page https://www.abelectronics.co.uk/p/77/rs485-pi and schematic
https://www.abelectronics.co.uk/docs/pdf/schematic-rs485pi.pdf (fetched 2026-10-22).

| Item | Value | Label |
|---|---|---|
| Transceiver | TI **SN65HVD72** (3.3 V, half duplex, 250 kbps) | VERIFIED-SOURCE (product page + schematic) |
| Pi pins used | **pin 8 = GPIO14 TXD**, **pin 10 = GPIO15 RXD**, pin 1 = 3.3 V, pin 6 = GND ("wiring just four signals") | VERIFIED-SOURCE (product page "Using the RS485 Pi Standalone") |
| Direction (DE/RE) | **Automatic**: TXD drives transistor Q1 (BSN20) via R6 (0 Ω) / R4 (10 k); Q1 pulls the tied DE/RE node (pull-up R5 10 k) — driver enabled only while TXD is low. Unpopulated R7 would allow GPIO17 (pin 11) control instead. | VERIFIED-SOURCE (schematic reading) — HYPOTHESIS that the owner's board is populated as default |
| Protection | TVS diode + two 10 Ω pulse-proof resistors on A/B | VERIFIED-SOURCE |
| Termination | 120 Ω resistor supplied loose, only fit at the end of the bus | VERIFIED-SOURCE (product page) — whether NibePi installs fit it: unknown |
| Software implications | No RTS/GPIO direction handling, no RS-485 ioctl (`TIOCSRS485`) needed; **disable hardware flow control**. Probably no local echo (receiver disabled while driving, idle-high while not) — parser must tolerate echo anyway. | HYPOTHESIS |

## 3. Pi Zero W UART facts
Source: Raspberry Pi documentation, `documentation/asciidoc/computers/configuration/interfaces.adoc`
(github.com/raspberrypi/documentation, commit `ddea98e9`, section "Configure UARTs").

| Fact | Label |
|---|---|
| Pi Zero/1/2/3 have UART0 = **PL011** and UART1 = **mini UART**. | VERIFIED-SOURCE |
| On **Zero W / Zero 2 W** the *primary* UART (GPIO14/15, `/dev/serial0`) is **UART1 = mini UART (`/dev/ttyS0`)**; the PL011 (`/dev/ttyAMA0`) is the *secondary* UART wired to **Bluetooth**. | VERIFIED-SOURCE |
| Mini UART: no framing/parity error detection, no break detection, small FIFO, **baud rate tied to the VPU core clock**; disabled unless the core clock is fixed (`enable_uart=1` fixes core at 250 MHz). | VERIFIED-SOURCE |
| `dtoverlay=disable-bt` (+ `sudo systemctl disable hciuart`) makes PL011 (`ttyAMA0`) the primary UART on GPIO14/15 and disables Bluetooth. `dtoverlay=miniuart-bt` instead moves Bluetooth to the mini UART. | VERIFIED-SOURCE |
| The serial console must be disabled on the primary UART (`raspi-config` → Interface Options → Serial Port: login shell **No**, hardware **Yes**; i.e. remove `console=serial0,115200` from `cmdline.txt`). | VERIFIED-SOURCE |
| Because NibePi talks to `/dev/ttyAMA0` on a Zero W, the NibePi image **must** have moved Bluetooth off the PL011 (`disable-bt`/`pi3-disable-bt` or `miniuart-bt`). | HYPOTHESIS (deduced; image not inspected — 2.5 GB download, see HARDWARE_PENDING) |

### Recommended configuration for our image (WS03/WS06)
```
# /boot/firmware/config.txt (Bookworm+/Trixie; /boot/config.txt on Bullseye)
enable_uart=1
dtoverlay=disable-bt
# cmdline.txt: no "console=serial0,115200"
# systemctl disable --now hciuart.service   (if present)
```
* Device: `/dev/ttyAMA0` (equivalently `/dev/serial0` after `disable-bt`). The gateway should log what
  `/dev/serial0` resolves to at start-up and refuse `/dev/ttyS0` unless explicitly configured.
* Bluetooth is not needed (provisioning uses a Wi-Fi AP, brief 08). Wi-Fi is on SDIO and unaffected.
* Permissions: run as a dedicated user in group `dialout`.
* termios: raw (`cfmakeraw`), `B9600`, `CS8`, `~PARENB`, `~CSTOPB`, `~CRTSCTS`, `CLOCAL|CREAD`, no IXON/IXOFF.
  PL011 has a receive-timeout interrupt, so short frames arrive promptly without a low-latency flag.
* Single-core 1 GHz ARMv6: run the serial task on its own OS thread (or a dedicated current-thread runtime) so
  HTTP load cannot delay ACKs; consider `SCHED_FIFO` low priority only if measurements show missed deadlines
  (NibePi had `chrt -f 99` commented out: backend.js:24-32).

## 4. If the HAT is a different model
Boards like the Waveshare RS485 CAN HAT drive DE/RE from a GPIO (often GPIO4/GPIO18) or need RTS. nibegw.c v1.31
mentions fixing serial settings "for RPi Zero 2 W + Waveshare RS485 CAN hat". Make direction control a config
option (`none` default | `gpio:<n>` | `rts`) in WS04, defaulting to `none` for the AB board. **Do not guess on
hardware**: identify the board from a photo first (HARDWARE_PENDING).

## 5. Electrical / installation safety
* The Pi is powered from the pump's 12 V accessory supply; brown-outs during heavy Wi-Fi TX are possible on
  a Zero W — keep the gateway's SD writes minimal (brief 13) and log under-voltage (`vcgencmd get_throttled`).
* The bus is shared with the pump's own accessory traffic (e.g. RMU40). A misbehaving transmitter can disturb
  other accessories: never transmit except as a reply to our own address.
* Nothing in this project requires opening the pump; the existing NibePi wiring is reused unchanged.
