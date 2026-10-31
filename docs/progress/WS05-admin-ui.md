# WS05 — Admin UI

- **Milestone:** M1/M3
- **Status:** done-hw (used on the owner's Pi; phone-width pass HP-19 open)
- **Owns:** apps/admin, tests/e2e
- **Brief:** [07-admin-ui.md](../brief/07-admin-ui.md), [06-http-api.md](../brief/06-http-api.md), [08-wifi-provisioning.md](../brief/08-wifi-provisioning.md), [10-verification.md](../brief/10-verification.md)

## Checklist
- [x] Vite+React+TS app, pnpm, small deps; dev proxy /api to Rust service
- [x] Pages: login/initial setup, overview (connectivity/versions/comm status/freshness; uptime explicitly unavailable in current API), live readings (SSE), controls (disabled state), register explorer (search, raw/decoded, verification), Wi-Fi setup, updates (unavailable pending WS07), diagnostics/sanitized support JSON, protected debug view
- [x] Typed API client generated from live /openapi.json, including WS06 schemas
- [x] Production build served by Rust gateway's existing static-directory hook
- [x] Playwright e2e against `cargo run -- --device simulated` (setup/login, readings/staleness, explorer/discovery, provisioning, update-unavailable state, diagnostics, mobile, session expiry)
- [x] Design system in `src/ui` (tokens, primitives, forms, layout, data display, charts, shell, patterns); pages in `src/pages` built from it; Storybook 10 with docs, page and full-app stories (ADR 0121)

## WS06 contract coordination

Final integrated contract (2026-10-22), verified against Rust and the running
OpenAPI document:

- `GET /api/v1/auth/session`; `GET /api/v1/setup/status` for public setup context.
- `POST /api/v1/setup/admin` accepts `{password}` plus `X-Setup-Token` from the
  device's private setup handoff; `POST /api/v1/auth/login` accepts `{password}`;
  `POST /api/v1/auth/logout` revokes the HttpOnly cookie session.
- Every POST sends `X-Requested-With: nibe-gateway`; all calls use same-origin
  credentials. Password/token fields stay in component memory and are cleared
  on success. No localStorage/URLs/logging of credentials; 401 on protected
  requests returns to sign-in, disposes SSE, and removes protected UI state.
- `GET /api/v1/network/status`; `GET /api/v1/network/scan` returns
  `{networks:[{ssid,signal,secured}]}`; `POST /api/v1/network/connect` accepts
  `{ssid,password}` and returns verified state, `setup_ssid`, `last_error`, and
  `simulated`. The UI clears Wi-Fi passwords after each attempt, refetches on
  failure, and explains single-radio reconnection. Overview uses this network
  service, separately from synthetic device-domain network scenarios.
- No IP address is returned by the current contract; the UI reports unavailable.
  Updates/restart/recovery stay disabled pending WS07 service integration.

## Verify

Run from the repository root:

```bash
pnpm --dir apps/admin install --frozen-lockfile
pnpm --dir apps/admin build
playwright install chromium
pnpm --dir apps/admin test:e2e
cargo test --workspace
git diff --check
```

All passed on 2026-10-22. Browser runner uses globally installed mise Playwright
1.64.x, starts a fresh simulated gateway with temporary persistent state and an
ephemeral loopback port, drives real Chromium against the built UI, and cleans
up browser/server/state. **10 browser checks passed**, no uncaught browser
exceptions; desktop 1440 px and phone 390 px screenshots were visually inspected.
Screenshots are reproducible under ignored `tests/e2e/artifacts/`: setup,
overview-desktop, live-readings, stale-readings, registers, wifi-connected,
updates, and mobile overview/readings/registers/Wi-Fi/updates. The Rust workspace
suite passed 33 tests after concurrent WS04 modules became available.

For API regeneration and development (two terminals):

```bash
cargo run -p nibe-gateway -- --device simulated
pnpm --dir apps/admin generate:api
pnpm --dir apps/admin typecheck
pnpm --dir apps/admin dev
```

Vite defaults to proxying to `http://127.0.0.1:8080`; set `GATEWAY_URL` to override.
Also verified real-browser setup, cookie persistence, CSRF POST, SSE and Wi-Fi
scan through Vite on port 18086 proxying the gateway on 18085. Build before
starting the production gateway so it discovers `apps/admin/dist/index.html`.
See `apps/admin/README.md` and `tests/e2e/README.md` for repeatable setup.

## Open issues / hardware-pending

- WS07 must provide an authorized updater/restart/recovery service before those
  UI actions can run. Browser tests assert truthful disabled availability; no
  actual update/rollback is claimed. Support output is allowlisted JSON metadata
  and counters, not an appliance log archive.
- API uptime and network IP are not currently reported; UI shows that explicitly.
- Physical AP/station transition, mDNS, F730 readings and Pi resource/browser
  behavior need local hardware. Exact phone/desktop, connection failure/recovery,
  SSE freshness and support-report steps are appended under **WS05** in
  [HARDWARE_PENDING.md](HARDWARE_PENDING.md).

## Log

- 2026-10-22: Implemented Vite/React/TypeScript with pnpm lockfile and only
  React/React DOM runtime dependencies; local assets, hash navigation and
  responsive layouts. Added a dependency-free OpenAPI schema generator and
  integrated the final WS06 auth/network contract from the live gateway.
  Implemented all maintenance screens, bounded single-flight SSE snapshot reads
  with reconnect/resync and freshness aging, disabled safety-gated controls,
  paginated register search/inspection and passive discovery, Wi-Fi wrong-password
  recovery/verified connect, sanitized support download and authorized diagnostics.
  No raw writes or development mutation tools are exposed in the UI.
- 2026-10-22: Production output is 219.02 KB JS (68.89 KB gzip) plus 12.33 KB CSS
  (3.60 KB gzip), with no CDN resources. Built UI served by the existing Rust
  static-directory hook without changing Rust. Verified TypeScript/build,
  10 real-browser integration checks, desktop/phone screenshots, Vite proxy,
  complete Rust workspace tests and whitespace checks. Added ADR 0050 and exact
  hardware follow-up. Only owned paths and mandated progress/ADR/hardware tracking
  were edited; concurrent work was preserved. No commits.
- 2026-10-25 (bookkeeping): Access page (API keys, password change, SSH/Tailscale through the root helper; ADR 0110/0111, shown live on hardware), Integrations (MQTT, ADR 0115) and History page (0.1.14, ADR 0114), plus the reload banner for outdated tabs (0.1.6). Playwright restored: 12/12 browser checks. Open: HP-19 on the real Pi.
- 2026-10-26: Register explorer shows per-installation notes (ADR 0119): "Show: Relevant on this pump / All
  registers" (defaults to relevant when notes exist; registers without a note but with a reading always show),
  badges "Checked against the display" / "Suspicious" / "Unmaintained" / "Not used on this pump", label under the
  name. e2e replay step covers the filter and badges (screenshots `replay-register-notes*.png`). schema.ts regenerated.
- 2026-10-27: **Design system + Storybook (ADR 0121).** Extracted the shipped UI into `src/ui/` (76 exported components and parts,
  compound cards with named parts, CSS split into per-component partials with tokens) and split `App.tsx` into
  `src/pages/*` built only from those components. Storybook 10.6.4 (latest; react-vite, docs, a11y), 140 stories:
  foundations (colours, type, spacing, live palette audit, composition map of every page), every component and
  state, every page in simulated and F730-hardware states, and the full app per route, phone width, stale/
  reconnecting and outdated tab. Data comes from responses recorded from a simulated gateway plus the real F730
  catalog through an in-browser fetch/EventSource stand-in; 10 stories have interaction tests (play functions).
  Fixed while verifying (no visual change): typing `-2` in Heating offset produced `02` (controlled number input
  turned a lone `-` into 0; now kept as text and parsed on submit); field help is announced as the control's
  description instead of being part of its label; row-action columns have a screen-reader header.
  **Verify:**
  ```bash
  pnpm --dir apps/admin install --frozen-lockfile
  pnpm --dir apps/admin typecheck && pnpm --dir apps/admin build
  node tests/e2e/capture-pages.mjs /tmp/after      # 23 page screenshots; compare with a pre-change run
  pnpm --dir apps/admin test:e2e                    # 14/14 browser checks
  pnpm --dir apps/admin build-storybook
  node apps/admin/scripts/verify-storybook.mjs      # every story in Chromium: 140 rendered, 0 failed
  pnpm --dir apps/admin storybook                   # http://127.0.0.1:6006
  cargo test --workspace
  ```
  Results: before/after screenshots 0 changed pixels (23 × desktop 1440 / phone 390); page text identical apart
  from the hidden "Actions" header; e2e 14/14; Storybook 140/140 stories render with no console errors and all play
  functions pass, in the static build and the dev server; Rust workspace tests pass.
  **Open:** a11y panel reports pre-existing `color-contrast` in 103 stories, `link-in-text-block` (Wi-Fi notice
  link) in 3, `scrollable-region-focusable` (register table on phones) in 1; fixing them changes the look and is a
  separate owner decision. Gateway bundle +5 KB JS / +2.7 KB CSS (ADR 0121).
- 2026-10-27: Storybook docs audit (every docs page and story at 1440 px in Chromium, checked for raw Markdown and
  horizontal overflow): MDX pipe tables were not rendered (plain MDX has no tables) → `remark-gfm` in
  `.storybook/main.ts`; the app's global `td { white-space: nowrap }` / `td:first-child { min-width }` leaked into
  Storybook's own docs tables (Introduction, every props table ran off the page) → `.storybook/docs-overrides.css`,
  scoped to docs chrome outside rendered stories. Re-audit: 165 pages, 0 issues.
- 2026-10-27: **Colour consolidation (ADR 0122).** 115 colours → 31 role-based tokens; component CSS has no
  literal colours left. Text meets AA 4.5:1 everywhere (`--color-text-muted` `#5f6f63`, `--color-warning`
  `#846420`); notice links underlined; register table scroll region keyboard-focusable. Verify: build, then
  `node tests/e2e/capture-pages.mjs /tmp/after` against a pre-change capture (23/23 same size, 0.4–2.6 % pixels
  recoloured, reviewed side by side); `node apps/admin/scripts/verify-storybook.mjs` → 140 stories, 0 failed,
  **0 a11y findings** (was 104); e2e 14/14; `cargo test --workspace` passes.
- 2026-10-27: **0.1.24 deployed to the owner's Pi** (signed app OTA, slot b → a, 0.1.23 kept as rollback; updater
  readiness and probation passed in about a minute). Verified live: `/health` 0.1.24 ready; served JS/CSS hashes
  equal the local build; muted-text token in the served CSS; device `hardware`/`connected`, not stale, 46 readings;
  35 MiB RAM used; sign-in page renders at 1440 and 390 px with no browser errors. Signed-in pages not viewed by the
  agent (no admin password); owner to glance at Overview, Controls (type `-2` in Heating offset without applying)
  and Register explorer.

### 2026-10-31 — Audit log page with active leases (ADR 0128; implementation logged in WS02)
- New page "Audit log" (`#audit`, `pages/Audit.tsx`): an Active leases panel (every lease from
  `register_leases`: register, holder, leased value, value restored, time left) and a Changes table (time, actor and
  source, action/target with the redacted request, OK/Failed with the outcome), a source filter (All, Admin / API,
  MQTT, Gateway, Pump) and "Show older" paging. New `audit` icon, `api.audit`, regenerated `schema.ts`.
- Storybook: `Screens/Pages` `Audit log / Nothing recorded` and `Audit log / Leases and changes` (fixtures
  `auditEntries`, `registerLeases`; the hardware mock's status now carries `register_leases`).
- e2e: new step "audit log lists the changes made above, without secrets" (Wi-Fi change, refused heating offset,
  no Wi-Fi passwords, MQTT filter empty). Screenshot `tests/e2e/artifacts/audit-log.png`.
- Verify: `./scripts/build-frontend.sh`; `node tests/e2e/run.mjs` (15 passed); `pnpm --dir apps/admin
  build-storybook`.

