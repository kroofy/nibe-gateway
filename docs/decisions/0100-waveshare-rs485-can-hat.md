# ADR 0100: Treat the owner's Waveshare RS485 CAN HAT as auto-direction

Date: 2026-10-24 · Status: accepted

## Context
Early research assumed an AB Electronics board, because NibePi links to one. The installed board is a Waveshare
RS485 CAN HAT, which uses automatic direction control in its factory configuration.

## Decision
- No GPIO or RTS direction handling.
- The CAN half stays unused.

## Why
- Supporting an unconfirmed board configuration would add code and risk for nothing.
- The gateway has since worked on the bus this way.

## Consequences
If a board with non-factory links ever turns up, direction support has to be added first.
