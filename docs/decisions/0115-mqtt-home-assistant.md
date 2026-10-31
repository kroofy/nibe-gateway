# ADR 0115: Optional MQTT with Home Assistant discovery and a minimal built-in client

Date: 2026-10-25 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
Others want to integrate the pump with Home Assistant, Node-RED, openHAB or ioBroker, and people migrating from
NibePi used its MQTT topics. The owner runs no broker.

## Decision
- MQTT is optional and off by default, with a minimal built-in client and Home Assistant discovery.
- Commands are off unless switched on. They go through the same implementation as HTTP (ADR 0125).
- Topics follow the gateway's own layout, with NibePi-like suffixes, so a migration mostly means changing a prefix.

## Why
- MQTT is the common denominator for home automation. Home Assistant can bridge on to Apple Home and Google Home.
- A minimal client keeps the ARMv6 binary small and free of dependencies.
- Sharing the command path means sharing the safeguards.

## Rejected
- Native Matter: immature, and no confirmed Apple support for the heat-pump device type.
- TLS for now: it adds size and build cost for brokers that usually sit on the LAN. Revisit on demand.
- A NibePi compatibility layer with its exact topics, payloads and behaviour: more surface to maintain, and
  quirks worth not copying.

## Consequences
With commands on, anyone who can publish to the broker can write any register. The broker's access control is
then the only protection.
