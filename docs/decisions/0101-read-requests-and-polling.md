# ADR 0101: The gateway reads registers by asking, on the pump's free read tokens

Date: 2026-10-24 · Updated: 2026-10-31 · Status: accepted (owner decision)

## Context
The owner's F730 pushes no register values, so an accessory that only acknowledges sees nothing. NibePi gets its
values by answering read tokens with read requests. The pump offers about one read opportunity per second.

## Decision
- On hardware, read requests are allowed behind an explicit switch. Settings may be read too.
- A bounded set of registers is polled round-robin. Interactive reads and write read-backs go first. A read that
  times out is skipped, not retried.
- Which registers are polled, and how often, can be changed at runtime. The choices survive restarts and updates.
  A one-off read does not enter the live snapshot or history.

## Why
- A read request cannot change the pump.
- User actions should not wait behind background polling, and one silent register must not stall the rest.
- A bounded list keeps the refresh rate predictable.

## Rejected
- Configuring the pump to push values: it needs a site visit and a change on the pump.
- Poll choices kept only in the image's configuration: every OS update overwrote them.

## Consequences
The gateway transmits more than acknowledgements. The refresh interval grows with the list, so staleness
thresholds must match it.
