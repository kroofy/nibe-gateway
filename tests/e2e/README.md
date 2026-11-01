# Real-browser admin integration

Run from the repository root with pnpm, Rust, and the mise-installed global
`playwright` CLI available on PATH:

```bash
pnpm --dir apps/admin install --frozen-lockfile
playwright install chromium
pnpm --dir apps/admin build
node tests/e2e/run.mjs
# equivalent: pnpm --dir apps/admin test:e2e
```

The dependency-free Node runner resolves the global Playwright package (including
mise's generated shim), starts `cargo run -p nibe-gateway --features dev --
--device simulated` on an ephemeral loopback port, and supplies a fresh temporary
`--data-dir` and the production static build. It cleans up its process and state
in `finally`. It uses real Chromium and real HTTP/SSE; no API response mocking.
It leaves screenshots under ignored `tests/e2e/artifacts/`, including a failure
screenshot if a browser assertion fails. The UI's package has no Playwright
runtime dependency. Install Chromium once if the CLI's browser version changed.

Checks cover setup proof/password confirmation, cookie policy, login failure,
logout, live scenario changes, stale/disconnect recovery, disabled controls
(UI and backend), catalog search/raw/decoded/verification, passive discovery,
Wi-Fi scan/bad password/AP fallback/verified connect, explicit updater absence,
sanitized report download, authorized diagnostics, mobile navigation/layout,
and session expiry. Updates test unavailable behavior when no updater is installed; no real update or radio operation is claimed. Dev routes are used only
by the test to change deterministic device scenarios; the UI never calls them.
