# ADR 0113: The gateway offers data and controls, not automation

Date: 2026-10-25 · Updated: 2026-10-31 · Status: accepted (owner decision)

## Context
NibePi shipped automations that act on thresholds: hot-water auto-luxury, hot-water priority, and weather, indoor,
price and fan control.

## Decision
- The gateway reads, exposes and records pump data, and writes on request.
- It contains no threshold or schedule logic that changes how the pump behaves.
- Housekeeping that never touches pump behaviour is not automation. Examples: word-order detection, Wi-Fi
  recovery, update rollback.
- Exceptions, each only undoing the effect of something outside the pump: the automatic reset of alarm 251
  (ADR 0105) and the restore of a lapsed lease (ADR 0123).

## Why
- Deciding *when* to act belongs to the consumers: Home Assistant, Node-RED, scripts. They have more context and
  are easier to change.
- Logic inside the gateway would be hidden and hard to override. Every automation would also be one more way to
  change the pump without anyone asking.

## Rejected
- Porting NibePi's automations.
- Gateway-owned schedules such as "set this value until 18:00".
