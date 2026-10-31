# ADR 0122: One role-based colour palette, with all text readable to WCAG AA

Date: 2026-10-27 · Status: accepted

## Context
After ADR 0121 the stylesheet still held the colours as they had accumulated: over a hundred values, most of them
accidental near-duplicates of the same few roles, and the muted text greys failed WCAG AA contrast on most screens.

## Decision
The palette is a small set of tokens named by role (surfaces, borders, text, brand, on-brand, status). Component
styles use only tokens. Every text token meets AA (4.5:1) on every surface it is used on; disabled controls are
the only exception. Links in running text are underlined, and scrollable regions are keyboard-reachable.

## Why
Near-duplicates carried no meaning, made consistent changes impossible and hid the contrast problem. Merging by
role rather than by visual distance keeps status colours distinct from the green tints they resemble. Darkening the
muted text was the change that fixed contrast, so it was done in the same deliberate visual change.

## Rejected
- Merging purely by colour distance: groups warning and error backgrounds with neutral tints.
- Keeping the old greys and fixing contrast per component: reintroduces drift.

## Consequences
The UI looks the same in layout, with darker small text. New colours need a new role, not a new value; the
Storybook palette audit shows any literal colour that creeps back.
