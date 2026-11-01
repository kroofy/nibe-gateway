# Development setup

Run commands from the repository root. Native development needs Rust/Cargo,
Python 3, Bash, and `just`; the frontend additionally needs Node 22+ and pnpm
(or Node's corepack, which `scripts/build-frontend.sh` falls back to).
Keep Node off the production Pi. The supported Pi toolchain is separate from
system Rust, so an Arch installation without rustup works.

```bash
# Requires mise or a working rustup; does not use sudo or change global defaults.
./scripts/bootstrap-tools.sh
export PATH="$HOME/.local/share/nibe-build-tools/bin:$PATH"
just dev
just test
just build-pi
```

`bootstrap-tools.sh` installs Rust 1.90.0 through existing rustup or mise,
the ARMv6 standard library, cargo-zigbuild 0.20.1, Zig 0.14.1 in a Python venv,
and just 1.40.0. With mise, Rust is installed under its user data directory;
`with-build-tools.sh` selects it only for the invoked command. Python venv
bootstraps its own pip; no system pip is needed. Override `NIBE_BUILD_TOOLS`
before bootstrap to relocate its venv/executables. A C compiler and normal
native Rust build dependencies are needed to install Cargo tools.

This workspace's initial host has system Rust 1.99.1 at `/usr/bin/rustc`, mise,
and no functional rustup/just initially. The procedure above was run successfully
on 2026-10-22, without sudo. Unconfigured mise shims are detected before selecting
rustup. No desktop configuration or repository Rust override is installed.

Native Rust commands can use system Cargo, or the pinned toolchain:

```bash
./scripts/with-build-tools.sh cargo test --workspace
./scripts/with-build-tools.sh cargo fmt --all -- --check
./scripts/with-build-tools.sh cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/check-tooling.sh
```

`just dev` uses the simulator and the `dev` feature on loopback. Production
`build-pi` never enables the dev feature; `NIBE_PI_FEATURES=serial`,
`network-manager` or `serial,network-manager` adds the hardware adapters for bench
work (anything else, including `dev`, is rejected). The appliance image starts the
hardware/LAN profile (`/etc/nibe-gateway/hardware.json`); a simulated
bundle is built with `NIBE_IMAGE_PROFILE=simulated scripts/build-alpine-os-bundle.sh`.
`scripts/build-frontend.sh` requires the committed `apps/admin/pnpm-lock.yaml` and
runs `pnpm install --frozen-lockfile` and `pnpm build`.

SSH diagnostics require a previously trusted SSH config alias and key:

```bash
NIBE_SSH_HOST=my-nibe just logs
NIBE_SSH_HOST=my-nibe just debug
```

`debug` is read-only: services, updater/OS state, health, network, memory and recent warnings; authorized, expiring
verbose tracing and capture export live in the admin API/UI (see
[OTA and diagnostics](ota-and-diagnostics.md)). SSH uses batch authentication and
strict host key checking. Export diagnostics deliberately and sanitize them before
sharing. `just deploy` / `just rollback` sign a release locally, upload it
over key-only SSH and drive the on-device updater; they need
`NIBE_SSH_HOST` and `NIBE_SIGNING_KEY` and a Pi with key-only SSH.

See [cross compilation](cross-compile.md) and [image build](image-build.md).

## Checks

CI runs the first group. The second group starts real processes on loopback and touches no hardware.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
./scripts/check-tooling.sh
./scripts/build-frontend.sh

pnpm --dir apps/admin typecheck
pnpm --dir apps/admin test:e2e      # needs a built UI and Playwright's Chromium
./tests/integration/http-smoke.sh default
./tests/integration/http-smoke.sh dev
./tests/integration/provisioning-http-smoke.sh
./tests/integration/update-smoke.py
python3 tests/integration/wire-runtime-smoke.py
python3 crates/device-f730/tests/replay-http-smoke.py
python3 crates/device-f730/tests/serial-pty-smoke.py
python3 scripts/soak-local.py --duration-seconds 180 --report /tmp/nibe-soak.json

# Protocol reference and fixtures
python3 docs/research/tools/nibe_ref.py selftest
python3 docs/research/tools/nibe_ref.py gen /tmp/nibe-regen && diff -r /tmp/nibe-regen fixtures/serial-captures --exclude=README.md --exclude='real-f730-*'
PROPTEST_CASES=20000 cargo test -p nibe-protocol

# The dev feature is refused for Pi builds (expect exit 2)
NIBE_PI_FEATURES=dev ./scripts/build-pi.sh
```
