# ADR 0093: Automatic return to NibePi during the migration trial

Date: 2026-10-23 · Status: retired (NibePi retired 2026-10-25)

## Context
The longer trial of our gateway on NibePi's OS (ADR 0012) needed an automatic way back to NibePi if our gateway
failed while the owner was away.

## Decision
An independent watcher:
- stops our gateway first;
- only then starts NibePi;
- latches, rather than switching back and forth.

## Why
- Two programs owning the UART is worse than none, so stop must succeed before start.
- Latching avoids flapping.
- Independence means a broken gateway cannot prevent its own fallback.
