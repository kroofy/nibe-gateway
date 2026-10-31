# ADR 0123: A writer may lease a setpoint; the gateway restores it when the lease lapses

Date: 2026-10-29 · Updated: 2026-10-31 · Status: accepted (owner decisions)

## Context
An external planner changes the heating offset and a few setpoints for a while. If the planner or its server dies
in between, the pump keeps the temporary value until a human notices. The planner cannot clean up after its own
death, and the owner does not want the gateway to watch other systems to find out.

## Decision
- A write to the heating offset, or to one of a short, fixed list of absolute setpoints, may carry a lease. The
  holder renews it by writing again with a lease. Adding a register to the list needs an owner decision and a
  hardware check.
- The baseline is the value read from the pump before the first leased write, from a reading taken after any
  preceding write completed. Renewals keep it. When the lease lapses, the gateway writes the baseline back, and
  keeps the lease until the pump accepted that write and a reading taken after it shows the baseline.
- A write without a lease, from any surface, ends the lease without a restore. A write that definitely never
  reached the pump does not. An uncertain outcome is settled by the next fresh reading.
- One coordinator holds every lease, on every surface. Leases are bounded, stored durably before the write is sent,
  survive restarts and stay readable by earlier releases.

## Why
- A dead-man's switch is the only protection that works when the writer itself is gone, and the gateway is the one
  party that is always present.
- The read-back baseline is the owner's own normal setting, which a configured value would not track.
- Ending the lease on a plain write keeps humans in charge, so the gateway never fights them. A write that never
  landed must not remove the protection.
- Restoring an absolute value is safe to repeat, unlike a blind retry of a write (ADR 0125).

## Rejected
- A configured baseline, default 0: wrong for an owner whose normal offset is not 0.
- Timed writes owned by the gateway ("−10 for an hour"): scheduling inside the gateway (ADR 0113).
- Watching the planner or other devices: couples the gateway to systems it should not know.
- Leases on any register: counters the pump changes itself, such as degree minutes, have no meaningful baseline,
  and writing back an old count would distort the pump's own control.
- A separate coordinator per kind of register: the rules would drift.

## Consequences
A lease restore is an exception to ADR 0113. A writer that wants a value to stay must write without a lease.
