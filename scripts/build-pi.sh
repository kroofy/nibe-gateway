#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo"
[[ $# == 0 ]] || { echo 'usage: build-pi.sh (release target/features are fixed)' >&2; exit 2; }
# Do not inherit host CPU/linker flags; they invalidate the compatibility proof.
if [[ -n ${RUSTFLAGS:-}${CARGO_ENCODED_RUSTFLAGS:-} ]]; then
    echo 'Unset RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS for the supported Pi build.' >&2
    exit 1
fi
# NIBE_PI_LIBC=musl (default): static musl (ADR 0117), what the Alpine appliance runs.
# NIBE_PI_LIBC=gnu: glibc <= 2.28, dynamically linked (the retired Raspberry Pi OS base; kept because it is cheap).
case "${NIBE_PI_LIBC:=musl}" in
    gnu) triple=arm-unknown-linux-gnueabihf; zig_target=$triple.2.28; default_dir=pi ;;
    musl) triple=arm-unknown-linux-musleabihf; zig_target=$triple; default_dir=pi-musl ;;
    *) echo 'NIBE_PI_LIBC must be gnu or musl' >&2; exit 2 ;;
esac
export CARGO_TARGET_DIR="${NIBE_TARGET_DIR:-$repo/target/$default_dir}"
# Optional production adapters for the hardware bench (docs/hardware-test-plan.md).
# Only an explicit allowlist is accepted: the `dev` feature (/__dev routes,
# simulated controls) can never be compiled into a Pi binary through this script.
features=()
export NIBE_PI_FEATURES=${NIBE_PI_FEATURES-serial,network-manager}
case "$NIBE_PI_FEATURES" in
    '') ;;
    serial|network-manager|serial,network-manager|network-manager,serial)
        features=(--features "nibe-gateway/${NIBE_PI_FEATURES//,/,nibe-gateway/}") ;;
    *) echo "NIBE_PI_FEATURES must be empty, serial, network-manager or serial,network-manager (never dev)" >&2; exit 2 ;;
esac
# Rust's musl targets link statically (crt-static) by default; zig supplies musl and the C compiler
# for bundled SQLite.
"$repo/scripts/with-build-tools.sh" cargo zigbuild --locked --release --no-default-features "${features[@]}" \
    --target "$zig_target" -p nibe-gateway -p nibe-updater -p nibe-system --bin nibe-gateway --bin nibe-updater --bin nibe-system
release="$CARGO_TARGET_DIR/$triple/release"
binary="$release/nibe-gateway"
for program in nibe-gateway nibe-updater nibe-system; do
    "$repo/scripts/check-pi-elf.py" --libc "$NIBE_PI_LIBC" "$release/$program"
done
printf 'Pi binary: %s\n' "$binary"

# Bind feature evidence to the exact executable used by image assembly.
python3 - "$binary" "$NIBE_PI_FEATURES" <<'PYFEATURES'
import hashlib, json, pathlib, sys
binary = pathlib.Path(sys.argv[1])
pathlib.Path(str(binary) + ".features.json").write_text(json.dumps({
    "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "features": sorted(filter(None, sys.argv[2].split(",")))
}) + "\n")
PYFEATURES
