#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo"
source "$repo/scripts/ssh-maintenance.sh"
: "${NIBE_SIGNING_KEY:?Set NIBE_SIGNING_KEY to a local 0600 release signing key file}"
"$repo/scripts/build-frontend.sh"
# NIBE_PI_LIBC=musl (default) ships the static musl build the Alpine appliance runs; gnu is the retired
# Raspberry Pi OS build.
# The release target stays arm-unknown-linux-gnueabihf: it is the updater's platform label (ARMv6 hard-float Pi),
# and a static binary needs nothing from the system C library.
case ${NIBE_PI_LIBC:-musl} in
    musl) default_binary=$repo/target/pi-musl/arm-unknown-linux-musleabihf/release/nibe-gateway ;;
    gnu) default_binary=$repo/target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway ;;
    *) echo "NIBE_PI_LIBC must be gnu or musl" >&2; exit 2 ;;
esac
binary=${NIBE_PI_BINARY:-$default_binary}
version=$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("Cargo.toml").read_text())["workspace"]["package"]["version"])')
local_dir=$(mktemp -d)
remote_dir=''
cleanup() {
    rm -rf -- "$local_dir"
    if [[ -n $remote_dir ]]; then nibe_ssh "rm -rf -- '$remote_dir'" || true; fi
}
trap cleanup EXIT
cargo run --quiet -p nibe-updater --bin nibe-release -- sign "$NIBE_SIGNING_KEY" "$version" arm-unknown-linux-gnueabihf "$binary" "$repo/apps/admin/dist" "$local_dir/release.tar"
remote_candidate=$(nibe_ssh 'mktemp -d /var/tmp/nibe-deploy.XXXXXXXX')
[[ $remote_candidate =~ ^/var/tmp/nibe-deploy\.[a-zA-Z0-9]{8}$ ]] || { echo 'Unexpected remote temporary path' >&2; exit 1; }
remote_dir=$remote_candidate
scp -o BatchMode=yes -o StrictHostKeyChecking=yes "$local_dir/release.tar" "$NIBE_SSH_HOST:$remote_dir/release.tar"
nibe_ssh "sudo -n /usr/lib/nibe-gateway/nibe-updater client upload '$remote_dir/release.tar'"
nibe_ssh 'sudo -n /usr/lib/nibe-gateway/nibe-updater client activate'
nibe_wait complete
