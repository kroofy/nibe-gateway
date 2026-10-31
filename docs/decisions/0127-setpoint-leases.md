# ADR 0127: The heating-offset lease extends to a few setpoint registers

Date: 2026-10-31 · Status: accepted (owner decision); extends ADR 0123, a further exception to ADR 0113

## Context
A planner migrating from NibePi lifts or lowers hot-water start and stop temperatures and the heating stop
temperature for a while, as well as the heating offset. If the planner or Home Assistant dies in between, the
pump keeps the temporary setpoint until a human notices. Since ADR 0125 every register is writable, so these
setpoints no longer need named controls, but only the offset had the dead-man's switch of ADR 0123.

## Decision
- A register write to one of a short, fixed list of absolute setpoints may carry a lease, with the rules of ADR
  0123: the baseline is the value read from the pump before the first leased write, renewals keep it, a write
  without a lease ends it without a restore, leases are durable and bounded, and a lapsed lease is restored once
  confirmed by a read-back.
- The list is hot-water start and stop temperature (economy) and the heating stop temperature. Adding a register
  needs an owner decision and a hardware check.
- One coordinator holds every lease, the heating offset included, so the rules exist once; only how a register
  is written and read differs. Leases are offered wherever register writes are (HTTP and MQTT).
- Saved leases stay readable by earlier releases, so an update rollback still restores the heating offset.

## Why
- Same failure mode as the offset, same remedy; the gateway is the one party that is always present.
- A second implementation of the same rules would drift from the hardware-verified one.

## Rejected
- A separate coordinator for the setpoints beside the offset lease: duplicated rules (built first, then merged).
- Leases on any register: counters the pump changes itself have no meaningful baseline. Degree minutes was
  requested and explicitly excluded by the owner: writing back an old count would distort the pump's own control.
- A gateway-side schedule ("set 30 °C until 18:00"): automation, rejected in ADR 0113 and 0123.

## Consequences
Restores are gateway-initiated writes of whatever the pump read before the lease, possibly hours earlier. As
for the heating offset, a restore the pump does not confirm is re-sent periodically until a read-back shows the
baseline; one the gateway itself refuses (for example writes switched off) is reported and not retried. A writer
that wants a value to stay must write without a lease. The offset lease now runs on the shared coordinator and needs its hardware check repeated.
