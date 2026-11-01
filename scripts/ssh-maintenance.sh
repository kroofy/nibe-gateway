#!/usr/bin/env bash
# Sourced by deploy/rollback. SSH identity/host verification are never weakened.
: "${NIBE_SSH_HOST:?Set NIBE_SSH_HOST to your key-authenticated SSH config alias}"
[[ $NIBE_SSH_HOST =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.@-]*$ ]] || { echo 'Invalid SSH host' >&2; exit 2; }
nibe_ssh() { ssh -o BatchMode=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=10 "$NIBE_SSH_HOST" "$1"; }
nibe_wait() {
    local expected=$1 result phase wait_seconds=${NIBE_UPDATE_WAIT_SECONDS:-2100}
    [[ $wait_seconds =~ ^[1-9][0-9]{0,4}$ ]] || { echo 'Invalid NIBE_UPDATE_WAIT_SECONDS' >&2; return 2; }
    wait_seconds=$((10#$wait_seconds))
    for ((attempt=0; attempt<wait_seconds; attempt++)); do
        # A single-core Pi can miss an SSH banner while the new release starts; retry, don't abort.
        if ! result=$(nibe_ssh 'sudo -n /usr/lib/nibe-gateway/nibe-updater client status'); then
            sleep 5
            continue
        fi
        phase=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["status"]["phase"])' <<< "$result")
        case "$phase" in
            "$expected")
                # The independent updater already checked three real readiness
                # probes, activation probation and release identity. Recheck over SSH before success.
                # Slots are root:nibe-gateway 0750, so the maintenance account reads the manifest via sudo.
                # The appliance has no Python: fetch both documents over SSH, compare here.
                local manifest health
                manifest=$(nibe_ssh 'sudo -n cat /var/lib/nibe-gateway/current/manifest.json') || return 1
                health=$(nibe_ssh "curl -s --noproxy '*' -m 3 http://127.0.0.1:8080/health") || return 1
                python3 -c 'import json,sys; m=json.loads(sys.argv[1]); h=json.loads(sys.argv[2]); assert h["ready"] and h["status"]=="ok" and h["version"]==m["version"] and h["release_id"]==m["sha256"], "health does not match the activated release"' "$manifest" "$health" || return 1
                printf '%s\n' "$result"
                return 0 ;;
            failed|rolled_back) printf '%s\n' "$result" >&2; echo 'Deployment failed or automatically rolled back.' >&2; return 1 ;;
        esac
        sleep 1
    done
    echo 'Timed out waiting for updater; inspect status before retrying.' >&2
    return 1
}
# Full OS update: stream a signed OS bundle compressed with zstd -19 (−19 % on the wire
# against the default level, same 8 MiB window so the Pi's decompression memory is unchanged). os-install on the
# Pi verifies signature and checksums before arming tryboot, then reboots once into the new slot; os-confirm
# commits it after the probation. Usage: nibe_os_install BUNDLE.tar
nibe_os_install() {
    local bundle=${1:-} level=${NIBE_OS_ZSTD_LEVEL:-19} status
    [[ -f $bundle ]] || { echo "OS bundle not found: $bundle" >&2; return 2; }
    [[ $level =~ ^([1-9]|1[0-9])$ ]] || { echo 'Invalid NIBE_OS_ZSTD_LEVEL (1-19)' >&2; return 2; }
    zstd -q -T0 "-$level" -c -- "$bundle" \
        | nibe_ssh 'zstd -dc | sudo -n /usr/lib/nibe-gateway/nibe-updater os-install /dev/stdin'
    status=("${PIPESTATUS[@]}")
    (( status[0] == 0 )) || { echo 'zstd failed on the host' >&2; return 1; }
    (( status[1] == 0 )) || { echo 'os-install failed or the connection broke; check os-status' >&2; return 1; }
}
