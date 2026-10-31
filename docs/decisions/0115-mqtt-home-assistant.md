# ADR 0115: Optional MQTT with Home Assistant discovery

Date: 2026-10-25 · Status: accepted (owner decision)

## Context
Others want to integrate the pump with Home Assistant, Node-RED, openHAB or ioBroker. The owner runs no broker.

## Decision
- Optional, off by default.
- A minimal built-in MQTT client.
- Home Assistant discovery.
- Commands only for controls already enabled, through the same path as the REST API.
- No topic that writes a register by number.

## Why
- MQTT is the common denominator for home automation. Home Assistant can bridge on to Apple Home and Google Home.
- A minimal client keeps the ARMv6 binary small and free of dependencies.
- Sharing the command path means sharing the safeguards (ADR 0104).

## Rejected
- Native Matter: immature, and no confirmed Apple support for the heat-pump device type.
- TLS for now: adds size and build cost for brokers that usually sit on the LAN. Revisit on demand.
