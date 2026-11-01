#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo"
source "$repo/scripts/tool-versions.sh"
export PATH="$NIBE_BUILD_TOOLS/bin:$PATH"
while IFS= read -r -d '' file; do
    bash -n "$file"
done < <(find scripts packaging/alpine -name '*.sh' -print0)
sh -n packaging/alpine/initramfs/init packaging/common/nibe-persist
# Keep bytecode out of the checkout.
PYTHONPYCACHEPREFIX="${TMPDIR:-/tmp}/nibe-tooling-pycache" python3 -m compileall -q scripts packaging/alpine
python3 -B -m unittest discover -s scripts/tests -v
if command -v just >/dev/null && just --version >/dev/null 2>&1; then
    just --list >/dev/null
fi
