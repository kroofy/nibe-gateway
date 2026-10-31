#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/tool-versions.sh
source "$(dirname "$0")/tool-versions.sh"
export PATH="$NIBE_BUILD_TOOLS/bin:$PATH"
if command -v rustup >/dev/null && rustup --version >/dev/null 2>&1; then
    exec rustup run "$NIBE_RUST_VERSION" "$@"
elif command -v mise >/dev/null; then
    exec mise exec "rust@$NIBE_RUST_VERSION" -- "$@"
fi
echo 'Install mise or rustup, then run scripts/bootstrap-tools.sh.' >&2
exit 1
