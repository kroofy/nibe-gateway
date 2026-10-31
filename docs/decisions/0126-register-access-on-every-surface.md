# ADR 0126: Every integration surface reads and writes registers by number

Date: 2026-10-31 · Status: accepted (owner decision); extends ADR 0125 to MQTT, amends ADR 0115

## Context
People migrating from NibePi used its MQTT topics to read and write any register by number. After ADR 0125 the
gateway's HTTP API could write any register, but it read only polled registers or catalog probes, and MQTT
offered three named controls. The owner wants HTTP and MQTT equally capable for any register number, without
copying NibePi's topic layout or behaviour one to one.

## Decision
- HTTP and MQTT both offer: write any register (scaled or raw value) and read any register now, getting the
  pump's answer back. Both surfaces call one shared implementation, so their rules are identical.
- MQTT uses the gateway's own layout: commands sit beside the existing value topic of each register, with
  NibePi-like suffixes, so a migration mostly means changing a topic prefix.
- MQTT register commands follow the existing MQTT command switch. HTTP needs a session or a control-scoped key.
- Continuous reads stay in the configured poll list; a one-off read is not added to the live snapshot or history.

## Why
- The same capability on every surface means a client never has to switch transport for one operation.
- One implementation keeps the single-shot, read-back write rules from drifting between surfaces.
- Reads never change the pump, so limiting them to catalog entries only got in the way.

## Rejected
- A NibePi compatibility layer (its exact topics, payloads, discovery and poll-list commands): more surface
  than the owner wants to maintain, and NibePi's behaviour includes quirks worth not copying.

## Consequences
Anyone who can publish to the broker while MQTT commands are enabled can write any register; the broker's own
access control is then the only protection.

## Amendment 2026-10-31: poll choices at runtime
Changing which registers are polled meant editing a file inside the OS image, which an OS update overwrites. The
owner wants this as convenient as NibePi. Each register can now be polled fast, slow or not at all from the admin
UI, HTTP and MQTT, through the same shared implementation. Choices are saved in the gateway's settings file on the
data partition as deviations from the configuration, so they survive restarts and updates, and returning a
register to its configured mode leaves nothing behind. Polled registers appear in history and Home Assistant.

