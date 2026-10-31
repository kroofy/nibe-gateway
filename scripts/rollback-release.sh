#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
source "$repo/scripts/ssh-maintenance.sh"
nibe_ssh 'sudo -n /usr/lib/nibe-gateway/nibe-updater client rollback'
nibe_wait rolled_back
