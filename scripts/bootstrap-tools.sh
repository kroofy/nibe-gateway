#!/usr/bin/env bash
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/tool-versions.sh
source "$script_dir/tool-versions.sh"
if command -v rustup >/dev/null && rustup --version >/dev/null 2>&1; then
    rustup toolchain install "$NIBE_RUST_VERSION" --profile minimal --component rustfmt,clippy
elif command -v mise >/dev/null; then
    mise install "rust@$NIBE_RUST_VERSION"
else
    echo 'Install mise (https://mise.jdx.dev/) or rustup first. System Rust is left untouched.' >&2
    exit 1
fi
"$script_dir/with-build-tools.sh" rustup target add arm-unknown-linux-musleabihf arm-unknown-linux-gnueabihf
python3 -m venv "$NIBE_BUILD_TOOLS"
"$NIBE_BUILD_TOOLS/bin/python3" -m pip install "ziglang==$NIBE_ZIG_VERSION"
cat > "$NIBE_BUILD_TOOLS/bin/zig" <<'EOF'
#!/bin/sh
exec "$(dirname "$0")/python3" -m ziglang "$@"
EOF
chmod +x "$NIBE_BUILD_TOOLS/bin/zig"
"$script_dir/with-build-tools.sh" cargo install cargo-zigbuild --version "$NIBE_ZIGBUILD_VERSION" --locked --root "$NIBE_BUILD_TOOLS"
"$script_dir/with-build-tools.sh" cargo install just --version "$NIBE_JUST_VERSION" --locked --root "$NIBE_BUILD_TOOLS"
printf 'Tools installed. Add this directory to PATH: %s/bin\n' "$NIBE_BUILD_TOOLS"
