# ADR 0094: Faithful capture import and an explicit local resource budget

Date: 2026-10-23 · Status: accepted; NibePi import is historical (NibePi retired)

## Context
Testing needed real bus traffic, and Pi memory limits needed a measurable target before any hardware run.

## Decision
- Import NibePi's raw bus log faithfully:
  - never invent transmissions or timing;
  - ignore all other log lines.
- Define a local soak test with explicit memory budgets as its pass/fail criteria.

## Why
- Replay is only useful if it reflects the bus as it was.
- Logs can contain secrets.
- A budget turns "fits in 512 MB" into a check. It leaves about half the RAM to the OS.

## Consequences
A soak on the host is a regression signal, not evidence of how the Pi behaves.
