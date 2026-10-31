# ADR 0040: The gateway is a MODBUS40 accessory whose replies to the pump come first

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted

## Context
The pump talks NIBE's accessory protocol, not Modbus RTU. Once an accessory is configured, the pump expects a
reply within a tight window and raises alarm 251 when it doesn't get one. On the single-core Pi, HTTP, password
hashing, disk and decoding all compete for the CPU. The owner's RS485 board (Waveshare RS485 CAN HAT) switches
direction by itself in its factory configuration.

## Decision
- Emulate only a MODBUS40 accessory.
- Serial handling runs on its own thread, outside the application runtime, with a modest priority raise within
  the normal scheduler. A raise that can't be applied is reported.
- The reply is sent before observations are published. Under load, observations are dropped. A reply that would
  be late is not sent.
- A few consecutive missed deadlines are tolerated. UART errors are recovered by reopening the port with bounded
  backoff. When recovery is exhausted the process exits, so the supervisor restarts it. Pump silence alone is
  degraded, not fatal. Timing values are configurable hypotheses, tuned on hardware.
- Replayed captures never count as permission to transmit.

## Why
- A lost observation is harmless. A missed reply raises an alarm on the pump.
- Transient hiccups should not kill the accessory, but retrying silently forever hides a broken state.
- A late reply is only noise on the bus.
- Replay must not become a way to send bytes to a real bus.

## Rejected
- Ending the wire task on the first miss or UART error (the first design): the pump lost its accessory while
  health still said "ready".
- Realtime scheduling: it can starve housekeeping and needs more privilege.
- RMU40 emulation, as NibePi did: nothing the gateway offers needs it.
- RS485 direction control by GPIO or RTS: the installed board doesn't need it. A board with non-factory links
  would need it added first.
