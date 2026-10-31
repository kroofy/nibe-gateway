# ADR 0109: Probation ends on evidence, not on a timer

Date: 2026-10-25 · Status: accepted (owner decision); supersedes the completion rule of ADR 0096

## Context
A fixed ten-minute probation dominated update time. Across real updates, nothing ever changed after the first
complete poll round. The failures probation can catch show up early: startup crashes, broken pump communication,
restart loops.

## Decision
- Probation completes after a short period of continuous health, once one complete poll round has been answered.
- The fixed window remains the upper bound, and the fallback when no rounds happen.

## Why
- It proves what matters, that the gateway talks to the pump, instead of waiting.
- Failures that only appear after a reboot or under specific load are not caught by waiting longer anyway.
