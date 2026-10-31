set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

bootstrap:
    ./scripts/bootstrap-tools.sh

dev *args:
    cargo run -p nibe-gateway --features dev -- --device simulated {{args}}

test:
    cargo test --workspace --all-features
    ./scripts/check-tooling.sh

build-pi:
    ./scripts/build-pi.sh

deploy: build-pi
    ./scripts/release-action.sh deploy

logs:
    ./scripts/pi-diagnostics.sh logs

debug:
    ./scripts/pi-diagnostics.sh debug

rollback:
    ./scripts/release-action.sh rollback

# Signed Alpine OS bundle and the fresh-card image built from it (docs/dev/image-build.md).
os-bundle key version:
    ./scripts/build-alpine-os-bundle.sh {{key}} {{version}}

image key version: (os-bundle key version)
    ./scripts/build-alpine-image.sh target/alpine/nibe-os-alpine-{{version}}.tar
