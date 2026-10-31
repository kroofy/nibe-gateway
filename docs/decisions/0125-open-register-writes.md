# ADR 0125: Every register is writable through the API; the caller decides what to write

Date: 2026-10-31 · Status: accepted (owner decision); supersedes ADR 0104's "named controls only" rule

## Context
External clients (the owner's planner) need settings beyond the heating offset: hot-water mode and start/stop
temperatures, compressor limits, the heating stop temperature, holiday mode. Under ADR 0104 each one needed its own
control and a supervised hardware test first. The owner judged that too slow and wants to decide what to write.

## Decision
- On hardware with read requests enabled, any register can be written with any value the wire can encode. The
  gateway checks no allowlist, catalog flag or catalog range. On by default; the owner can switch it off, which
  leaves the named, gated controls of ADR 0104.
- Writes remain authenticated (admin session or a control-scoped key), one at a time, sent once, never retried,
  and read back.
- The heating offset still goes through its own control, so an open write ends a lease like any unleased write
  (ADR 0123).

## Why
- The owner, not the gateway, is responsible for the pump's settings, and the display and myUplink already allow
  the same changes.
- Single-shot writes with a read-back keep the outcome observable without the double-apply risk of retries.

## Rejected
- Keep ADR 0104 and add one control per register: correct but too slow for the owner's needs.
- An open write limited to catalog `read_write` entries within catalog range: the owner chose fully open.

## Consequences
A client bug or a leaked control key can change any setting, including alarm and protocol registers. Unverified
registers may decode, scale or behave differently than the catalog says. The brief's "no generic write endpoint"
rule and AGENTS.md rule 6 are relaxed accordingly.
