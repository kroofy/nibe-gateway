# API verification

`http.rs` and `../integration/scenarios.rs` are Cargo integration-test targets of
`nibe-api`; they start real ephemeral-port Axum HTTP servers and exchange bytes
over TCP. No in-process route shortcuts or external services are needed.

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
./tests/integration/http-smoke.sh default
./tests/integration/http-smoke.sh dev
./tests/integration/provisioning-http-smoke.sh
```

The smoke script starts the actual executable, curls the API, checks SPA/API
fallbacks, and verifies SIGTERM while an SSE connection is open. It uses a
random port and removes its temporary UI and processes. Needs curl, rg, Python 3
and Cargo. `CARGO_TARGET_DIR` is respected if set.

OpenAPI snapshots include the whole generated document. Review intentional
contract changes, then update both explicitly:

```bash
UPDATE_OPENAPI=1 cargo test -p nibe-api --test http_contract openapi_snapshot
UPDATE_OPENAPI=1 cargo test -p nibe-api --features dev --test http_contract openapi_snapshot
```

Normal test runs never rewrite snapshots. The default-feature suite asserts that
debug routes/schemas are absent even when callers ask to enable simulated
controls. The dev suite exercises faults, time advance, stale cache, recovery,
rejected commands, history and reset. Core tests ensure unknown register data
survives and a hardware adapter never receives unverified commands. Simulator
tests cover fixture states and boost expiry. Queue saturation/adapter deadlines
and a non-consuming event receiver verify application isolation. Real UART
ACK/timing isolation still needs hardware tests.

Provisioning contracts use real admin cookies; setup proof is created in a temporary private data directory. The provisioning test covers unauthorized reads/writes/SSE, wrong-password rollback, successful station verification, claim-once setup, CSRF/Origin rejection, logout/login, recovery and reset/revocation. `provisioning-http-smoke.sh` additionally verifies persistence across actual binary restarts.
