# ADR 0012: Migrate away from NibePi in reversible phases

Date: 2026-10-22 · Status: completed (NibePi retired 2026-10-25)

## Context
The owner is remote, and the pump alarms when the accessory stops answering. Only one program can own the UART.
The working NibePi setup had to stay recoverable.

## Decision
Migrate in steps, each of which can be undone without a site visit:
1. Passive capture while NibePi kept running.
2. A short trial with automatic fallback to NibePi.
3. A longer trial.
4. A new card, with the old card kept as physical rollback.

## Why
- The first real data came at no risk.
- A failure at any step returned to a known-good state on its own.

## Consequences
During the trials this required a fallback watcher (ADR 0093) and binaries compatible with NibePi's OS. Both
became obsolete once the migration completed.
