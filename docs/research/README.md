# Research (WS01 / M0)

| Document | Contents |
|---|---|
| [protocol-summary-for-implementers.md](protocol-summary-for-implementers.md) | **Start here (WS04).** Frame layout, checksum + escape rules, ACK/NAK, tokens, read/write formats, reply state machine, value decoding, timing |
| [protocol.md](protocol.md) | Evidence table for every protocol fact (VERIFIED-SOURCE / VERIFIED-3P-CAPTURE / CONFLICT / HYPOTHESIS) with file citations and commits |
| [hardware.md](hardware.md) | RS485 HAT (AB Electronics RS485 Pi, auto direction), GPIO14/15, serial params, Pi Zero W PL011 vs mini-UART, `disable-bt` |
| [nibepi-analysis.md](nibepi-analysis.md) | What the owner's NibePi actually does: start-up writes, alarm auto-reset, RMU emulation, cloud ping, raw-traffic logging hook |
| [register-catalog.md](register-catalog.md) | F730 catalog provenance, cross-check result, schema v1, register states |
| [feasibility.md](feasibility.md) | Verdict, proven facts, assumptions, unknowns, blockers, phased migration + rollback from NibePi |
| [history-storage.md](history-storage.md) | On-device history: tier sizes, flush cadence vs SD writes, crash safety, clock handling |
| [toolchain.md](toolchain.md) | ARMv6 cross-compile options (zigbuild / cross / musl), verification, pi-gen notes |
| [tools/nibe_ref.py](tools/nibe_ref.py) | Python reference codec + parser (oracle), self-test against published bytes, fixture generator |
| [tools/build_f730_catalog.py](tools/build_f730_catalog.py) | Regenerates `fixtures/register-catalogs/f730.json` |

Upstream sources were cloned to `/tmp` only (never into this repo):
anerdins/nibepi `1aa36b0`, bebben88/NibePi `00e584c`, yozik04/nibe `00f4ae8`, openhab/openhab-addons `d0a0a95`
(nibeheatpump binding), raspberrypi/documentation `ddea98e`, RPi-Distro/pi-gen README (master), AB Electronics
RS485 Pi product page and schematic v2.0, Rust platform-support page, cross-rs Dockerfile/crosstool config,
cargo-zigbuild README. All fetched 2026-10-22.

Nothing has been verified on the owner's hardware. Hardware steps: `docs/progress/HARDWARE_PENDING.md` (WS01 HW-0…HW-5 are now HP-01, HP-03…HP-06, HP-08), run in the order of [`docs/hardware-test-plan.md`](../hardware-test-plan.md).
