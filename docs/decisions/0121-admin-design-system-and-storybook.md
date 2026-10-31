# ADR 0121: The admin UI is built from a documented component library

Date: 2026-10-27 · Status: accepted

## Context
The admin UI had grown as a few large files. Visual units (panels, status cards, reading cards, table cells)
existed only as repeated markup and class names, so every new page copied markup and no place showed every state
of a component. The owner wants the existing design captured as named components and then used by the app.

## Decision
The UI is a library of components, from primitives up to compound cards with named parts, and pages are composed
only from it. Storybook (current major) documents every component, page and app route, rendering the real modules
against recorded gateway data. The extraction itself changed nothing visible.

## Why
Named boundaries make pages consistent and reviewable, and one catalogue of states lets design changes be judged
before they reach a page. Keeping the existing class names and plain global CSS kept the change pixel-neutral and
left the browser tests and the cascade untouched. Recorded data shows realistic density without a running gateway.

## Rejected
- CSS modules or CSS-in-JS: would have changed the cascade and added runtime or tooling for no user benefit.
- A mocking library for stories: a small in-browser stand-in suffices and keeps dependencies minimal.
- Rewriting the look during extraction: would have hidden regressions behind intended changes (done separately, ADR 0122).

## Consequences
New UI starts in the library, with a story. Storybook is a development dependency only; the gateway bundle stays
React-only. Stories must be kept in step with the API through re-recorded fixtures.
