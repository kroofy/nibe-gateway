# ADR 0113: The gateway offers data and controls, not automation

Date: 2026-10-25 · Status: accepted (owner decision)

## Context
NibePi shipped automations that act on thresholds: hot-water auto-luxury, hot-water priority, and weather, indoor,
price and fan control.

## Decision
- The gateway reads, exposes and records pump data.
- It performs named, verified writes on request.
- It contains no threshold or schedule logic that changes how the pump behaves.
- Housekeeping that never touches pump behaviour is not automation. Examples: word-order detection, Wi-Fi recovery,
  update rollback.

## Why
- Deciding *when* to act belongs to the consumers: Home Assistant, Node-RED, scripts. They have more context and
  are easier to change.
- Logic inside the gateway would be hidden and hard to override. Every automation would also be one more way to
  change the pump without anyone asking.

## Exception
Alarm 251 auto-reset (ADR 0105). The gateway causes that alarm itself, and otherwise it would need a manual reset
after every update.

## Amendment 2026-10-29
A second exception: the heating-offset control lease (ADR 0123). When a writer's lease lapses, the gateway writes
the offset back to the value it had before. It only undoes a write the lapsed writer made; it decides nothing itself.
