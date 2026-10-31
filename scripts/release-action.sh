#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
case "${1:-}" in
    deploy|rollback) action=$1 ;;
    *) echo 'usage: release-action.sh deploy|rollback' >&2; exit 2 ;;
esac
implementation="$repo/scripts/$action-release.sh"
if [[ ! -x $implementation ]]; then
    echo "Missing deployment implementation: $implementation" >&2
    exit 1
fi
exec "$implementation"
