# ADR 0123: A writer may lease the heating offset; the gateway restores it when the lease lapses

Date: 2026-10-29 · Status: accepted (owner decision); a second exception to ADR 0113

## Context
An external planner will soon write the heating offset through the API. If the planner or its server dies right after
writing a low offset, the offset stays there and the house cools until a human notices. The planner cannot clean up
after its own death, and the owner does not want the gateway to talk to other systems to find out.

## Decision
- A heating-offset write may carry a lease. While the lease holds, the holder renews it by writing again with a lease.
- If the lease lapses, the gateway writes the baseline back once through the same named control, with all of ADR
  0104's gates. Nothing else is automated, and no other control or device is involved.
- The baseline is the offset read back from the pump before the first leased write. Renewals keep it.
- A write without a lease, from any path, ends the lease without a restore: whoever wrote last decided the value.
- Leases are bounded in length, survive a restart, and a lease that lapsed while the gateway was down is restored at
  start. The lease, its holder and the last restore are visible in the status.

## Why
- A dead-man's switch is the only protection that works when the writer itself is gone, and the gateway is the one
  party that is always present.
- The read-back baseline is the owner's own normal setting, which need not be zero. A configured value would go
  stale whenever the owner changes the setting on the pump.
- Ending the lease on a plain write keeps humans and other integrations in charge; the gateway never fights them.
- Restoring an absolute value is safe to retry after a transient failure, unlike the blind retries ADR 0104 forbids.

## Rejected
- A configured baseline, default 0: wrong for an owner whose normal offset is not 0.
- Timed writes owned by the gateway ("set −10 for an hour"): moves scheduling into the gateway (ADR 0113).
- Watching the planner or other devices (heartbeats, battery systems): couples the gateway to systems it should not know.

## Consequences
The lease is the second exception to ADR 0113, after the alarm-251 reset (ADR 0105), and like it only undoes the
effect of something outside the pump. A writer that wants a value to persist must write without a lease. Each new
leased control needs its own decision and hardware test.

## Amendment 2026-10-29: durable intent and confirmed restoration
All writers share the lease coordinator so an owner's supersession cannot race a restore or be hidden by renewal.
Intent and cancellation must be durable before dispatch; unavailable storage refuses the write.
Queue acceptance is insufficient evidence of restoration: retain the baseline until a terminal accepted outcome
and a subsequent fresh reading confirm it. A new baseline likewise requires a reading after the preceding write
completed. This favours temporarily refusing a lease over restoring a cached optimiser value later.

## Amendment 2026-10-29: rejected supersession retains protection
A definitely rejected owner write leaves the previous lease in force: an attempt that never reached the pump
cannot supersede the holder's setting. Restore that intent durably; ambiguous outcomes still cancel it because
the owner may have changed the pump. Completion evidence belongs to the leased control independently of other
controls, so unrelated activity cannot prevent confirmation or force needless repeated restoration.

## Amendment 2026-10-29: crash-safe owner supersession
An owner attempt retains the lease with a pending supersession until its outcome is known. Acceptance or an
ambiguous outcome ends the lease; a definite rejection retains it. If a restart hides the outcome, the first
fresh offset reading decides: a match with the owner's requested value ends the lease; otherwise the original
lease and its expiry remain. This favours retaining protection when the owner write never landed, without
replaying an uncertain owner write. A cached reading cannot settle that decision. ADR 0104 gates still apply.

## Amendment 2026-10-31: setpoint leases
The same lease now also covers a few hot-water and heating setpoints, and the offset can be leased through the register
write on every surface, MQTT included (ADR 0127).
