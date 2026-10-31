# ADR 0101: Opt-in read requests on the live bus

Date: 2026-10-24 · Status: accepted (owner decision); relaxes ADR 0011 and 0041

## Context
The first real capture showed that the owner's F730 pushes no register values. An accessory that only acknowledges
sees nothing. NibePi gets its values by answering read tokens with read requests.

## Decision
- Allow read requests on hardware behind an explicit opt-in.
- They keep the same bounds as probes, and settings may be read too.
- Writes stay impossible.

## Why
- A read request cannot change the pump.
- The alternative is configuring the pump to push values, which needs a site visit and a change on the pump.
- The opt-in keeps the default passive.

## Consequences
The gateway now transmits more than acknowledgements, so trials watch for NAKs, missed deadlines and pump alarms.
