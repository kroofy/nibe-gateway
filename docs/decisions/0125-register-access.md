# ADR 0125: Any register can be read and written by number, through one write path

Date: 2026-10-24 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
External clients (the owner's planner, Home Assistant, scripts migrating from NibePi) need to set hot-water,
compressor, heating and holiday settings, and to read any register, over HTTP and MQTT alike. A named control per
register, each with a supervised hardware test, was too slow for the owner. The owner, not the gateway, is
responsible for the pump's settings, and the display and myUplink already allow the same changes.

## Decision
- Open writes, on by default: on hardware with read requests enabled, any register can be written with any value
  the wire can encode. No allowlist, catalog flag or catalog range is checked.
- Every register write, from every surface, goes through one shared implementation. It is authenticated (admin
  session, control-scoped key, or MQTT with commands on), one at a time, sent once, never retried, and read back.
- HTTP and MQTT both read and write any register by number, with identical rules.
- With open writes off, the only writes are named controls, each for a single register that is verified on
  hardware, enabled in the config, and written within the catalog's range.

## Why
- A write that times out has an unknown outcome, so a retry could apply it twice or at the wrong time. The
  read-back is the evidence.
- One implementation keeps those rules from drifting between surfaces.
- Reads never change the pump, so limiting them to catalog entries only got in the way.

## Rejected
- Named controls only, one per register (the earlier rule): safe, but too slow for the owner.
- Open writes limited to catalog read-write entries within range: the owner chose fully open.
- A generic write that retries on timeout: it could apply a change twice.

## Consequences
A client bug or a leaked control key can change any setting, including alarm and protocol registers. Unverified
registers may decode, scale or behave differently than the catalog says.
