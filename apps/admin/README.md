# Gateway administration

Local Vite + React + TypeScript maintenance UI. React and React DOM are the only
runtime dependencies. Fonts, icons and assets are served locally. Navigation is
hash based so gateway SPA hosting needs no custom routes.

From the repository root:

```bash
pnpm --dir apps/admin install --frozen-lockfile
pnpm --dir apps/admin build
cargo run -p nibe-gateway -- --device simulated
# Open http://127.0.0.1:8080
```

Create an administrator with the unique `setup_token` from
`.data/setup-secrets.json` (or `<--data-dir>/setup-secrets.json`), using a password
of at least 12 characters. The token is a private device credential; do not
check that file into git or put it in a URL. On a real device a client on the open
setup AP needs no token (ADR 0130); the form hides the field when
`/api/v1/setup/status` reports `setup_token_required: false`. Login uses the gateway's
HttpOnly cookie. The simulator's Wi-Fi fixtures are `Home WiFi` with
`correct-password`, and open `Guest WiFi`.

## Design system and Storybook

UI components live in `src/ui/` (tokens, primitives, forms, layout, data display, charts, shell,
patterns; one stylesheet `src/ui/styles.css`). Pages in `src/pages/` are composed only from them
(ADR 0121). Storybook shows every component, page and app route with recorded data; no gateway
needed:

```bash
pnpm --dir apps/admin storybook          # http://127.0.0.1:6006
pnpm --dir apps/admin build-storybook    # static site in storybook-static/
node apps/admin/scripts/verify-storybook.mjs   # renders every story in Chromium, runs play tests
pnpm --dir apps/admin capture:fixtures   # re-record API fixtures from a simulated gateway
```

Start at **Introduction** and **Foundations / Composition map**. Add a story next to any new
component (`Name.stories.tsx`). Storybook is a dev dependency and never part of the gateway build.

For frontend development, keep the gateway running and use:

```bash
pnpm --dir apps/admin dev
# Optional different API address:
GATEWAY_URL=http://127.0.0.1:18085 pnpm --dir apps/admin dev
```

The proxy keeps cookies, SSE and POST CSRF metadata on the same browser origin.
To regenerate the checked-in API schemas from the running gateway:

```bash
pnpm --dir apps/admin generate:api
# Optional URL argument:
pnpm --dir apps/admin generate:api http://127.0.0.1:18085/openapi.json
pnpm --dir apps/admin typecheck
```

Built assets are served from `apps/admin/dist` by default, or via the gateway's
`--admin-dir` option. UI build is required before starting the gateway so its
static directory is discovered. No Rust source changes are required.

```bash
playwright install chromium
pnpm --dir apps/admin build
pnpm --dir apps/admin test:e2e
```

See [browser suite](../../tests/e2e/README.md). Live updates refetch state after
SSE notifications/reconnection/resync. Cached values remain visible with stale
warnings on communication loss. Controls are disabled by default; the UI does
not expose raw writes or development mutation routes. Update/restart actions
require the independent updater; network recovery requires an admin session. Device uptime/IP are not invented when
absent from the API. Support downloads contain allowlisted metadata/counters,
not secrets, logs, identifiers or raw register values.


WS07 maintenance integration: Updates now uses authenticated signed upload,
activation, rollback and restart endpoints, enabling them only when the updater
is reachable. Network recovery uses WS06's secured recovery route. Diagnostic
leases and replay capture export are available in Debug; support reports come
from the server-side allowlist. See [OTA and diagnostics](../../docs/dev/ota-and-diagnostics.md).
