# ADR 0011: Emulate a read-only MODBUS40 accessory

Date: 2026-10-22 · Status: accepted; deliberately relaxed by ADR 0101 (reads), 0104 (named writes), 0105 (alarm reset)

## Context
The pump talks NIBE's accessory protocol, not Modbus RTU. Once an accessory is configured, the pump expects it to
answer, and raises alarm 251 when it doesn't. On every start, NibePi writes pump settings, emulates an RMU40 and
resets alarms automatically.

## Decision
Emulate only MODBUS40 and answer it reliably. At first, send nothing that could change the pump: no writes, no
RMU40 emulation and no automatic alarm reset.

## Why
- Answering the pump is mandatory. Acting on it is optional.
- The owner is remote and the firmware was unknown. An accidental write could change heating with nobody on site.
- Starting from the minimum that keeps the pump content lets each extra capability be added deliberately, with
  hardware evidence.

## Consequences
NibePi features that depend on writes were dropped until they were verified one at a time.
