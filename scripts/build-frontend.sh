#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
if [[ ! -f $repo/apps/admin/package.json ]]; then
    echo 'Admin UI not present; frontend build skipped.'
    exit 0
fi
cd "$repo/apps/admin"
# Uses pnpm (packageManager in package.json) with a committed pnpm-lock.yaml.
if [[ ! -f pnpm-lock.yaml ]]; then
    echo 'Admin UI requires a committed pnpm-lock.yaml for a frozen install.' >&2
    exit 1
fi
if command -v pnpm >/dev/null 2>&1; then
    pnpm=(pnpm)
elif command -v corepack >/dev/null 2>&1; then
    # Node >= 16.9 ships corepack; it fetches the pinned packageManager version.
    export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
    pnpm=(corepack pnpm)
else
    echo 'pnpm (or Node corepack) is required to build the admin UI.' >&2
    exit 1
fi
"${pnpm[@]}" install --frozen-lockfile
"${pnpm[@]}" build
