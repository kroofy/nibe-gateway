# Hardware & feasibility investigation (M0)

> Source: ORIGINAL_BRIEF.md §3

## 3. Hardware and feasibility investigation

- **Board:** Raspberry Pi Zero WH; BCM2835 ARMv6 1 GHz; 512 MB RAM; 2.4 GHz Wi-Fi; soldered 40-pin header.
- **Heat pump:** NIBE F730, connected through existing NibePi RS485 hardware.
- **Reference:** https://github.com/anerdins/nibepi

Before implementing the driver, inspect NibePi source, pin assignments, serial configuration, frame formats, timing, checksums, acknowledgement behavior, register maps, and accessory activation requirements. The F-series MODBUS 40 interface must **not** be assumed to behave like arbitrary generic Modbus RTU. Record unknowns and capture real traffic where needed. Preserve the functioning existing installation until a safe migration and rollback are demonstrated.
