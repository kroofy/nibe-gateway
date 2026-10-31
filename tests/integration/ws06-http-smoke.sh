#!/usr/bin/env bash
# Real binary + curl: onboarding, authentication, rollback, persistence and reset.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build -p nibe-gateway
ws06_tmp=$(mktemp -d)
ws06_pid=''
cleanup() {
  if [[ -n "$ws06_pid" ]]; then kill "$ws06_pid" 2>/dev/null || true; wait "$ws06_pid" 2>/dev/null || true; fi
  rm -rf "$ws06_tmp"
}
trap cleanup EXIT
start() {
  "${CARGO_TARGET_DIR:-target}/debug/nibe-gateway" --device simulated --bind 127.0.0.1:0 --data-dir "$ws06_tmp/data" > "$ws06_tmp/gateway.log" 2>&1 &
  ws06_pid=$!
  address=''
  for _ in {1..100}; do
    address=$(sed -nE 's/.*address[^0-9]*(127\.0\.0\.1:[0-9]+).*/\1/p' "$ws06_tmp/gateway.log" | head -1)
    if [[ -n "$address" ]]; then break; fi
    if ! kill -0 "$ws06_pid" 2>/dev/null; then cat "$ws06_tmp/gateway.log" >&2; exit 1; fi
    sleep 0.05
  done
  [[ -n "$address" ]] || { cat "$ws06_tmp/gateway.log" >&2; exit 1; }
  base="http://$address"
}
stop() { kill -TERM "$ws06_pid"; wait "$ws06_pid"; ws06_pid=''; }
request() {
  local expected="$1" path="$2"; shift 2
  local actual
  actual=$(curl -sS -o "$ws06_tmp/response.json" -w '%{http_code}' -H 'X-Requested-With: nibe-gateway' -H 'Content-Type: application/json' "$@" "$base$path")
  if [[ "$actual" != "$expected" ]]; then
    echo "expected HTTP $expected, got $actual: $path" >&2
    cat "$ws06_tmp/response.json" >&2
    exit 1
  fi
}
field() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$ws06_tmp/response.json" "$1"; }
start
request 200 /health
request 401 /api/v1/device
request 200 /api/v1/setup/status
[[ $(field network_state) == ap ]]
setup_token=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["setup_token"])' "$ws06_tmp/data/setup-secrets.json")
request 200 /api/v1/network/scan -H "X-Setup-Token: $setup_token"
request 422 /api/v1/network/connect -H "X-Setup-Token: $setup_token" -d '{"ssid":"Home WiFi","password":"wrong"}'
request 200 /api/v1/network/status -H "X-Setup-Token: $setup_token"
[[ $(field state) == ap ]]
request 200 /api/v1/network/connect -H "X-Setup-Token: $setup_token" -d '{"ssid":"Home WiFi","password":"correct-password"}'
[[ $(field state) == connected ]]
request 200 /api/v1/setup/admin -H "X-Setup-Token: $setup_token" -c "$ws06_tmp/cookies" -d '{"password":"ws06-smoke-password"}'
request 401 /api/v1/network/status -H "X-Setup-Token: $setup_token"
request 200 /api/v1/device -b "$ws06_tmp/cookies"
request 409 /api/v1/setup/admin -b "$ws06_tmp/cookies" -d '{"password":"ws06-smoke-password"}'
request 403 /api/v1/discovery/scan -b "$ws06_tmp/cookies" -H 'Origin: https://attacker.invalid' -X POST
request 403 /api/v1/controls/heating-offset -b "$ws06_tmp/cookies" -d '{"offset":2}'
request 200 /api/v1/auth/logout -b "$ws06_tmp/cookies" -X POST
request 401 /api/v1/device -b "$ws06_tmp/cookies"
request 401 /api/v1/auth/login -d '{"password":"wrong"}'
request 200 /api/v1/auth/login -c "$ws06_tmp/cookies" -d '{"password":"ws06-smoke-password"}'
# Real TCP peer isolation: a second loopback address models another LAN client.
# Spoofed forwarding headers cannot bypass its bucket or consume the admin's.
for _ in {1..5}; do
  request 401 /api/v1/auth/login --interface 127.0.0.2 -H 'X-Forwarded-For: 127.0.0.1' -d '{"password":"wrong"}'
done
for _ in {1..10}; do
  request 429 /api/v1/auth/login --interface 127.0.0.2 -H 'X-Forwarded-For: 127.0.0.3' -d '{"password":"ws06-smoke-password"}'
done
request 200 /api/v1/auth/login -c "$ws06_tmp/cookies" -d '{"password":"ws06-smoke-password"}'
# Restart invalidates sessions but preserves admin/hash and verified SSID.
stop
start
request 401 /api/v1/device -b "$ws06_tmp/cookies"
request 200 /api/v1/auth/session
[[ $(field setup_required) == False ]]
request 200 /api/v1/auth/login -c "$ws06_tmp/cookies" -d '{"password":"ws06-smoke-password"}'
request 200 /api/v1/network/status -b "$ws06_tmp/cookies"
[[ $(field state) == connected ]]
request 200 /api/v1/network/recovery -b "$ws06_tmp/cookies" -X POST
[[ $(field state) == recovery_ap ]]
request 422 /api/v1/setup/factory-reset -b "$ws06_tmp/cookies" -d '{"confirmation":"wrong"}'
request 200 /api/v1/setup/factory-reset -b "$ws06_tmp/cookies" -d '{"confirmation":"ERASE"}'
request 401 /api/v1/device -b "$ws06_tmp/cookies"
request 401 /api/v1/network/status -H "X-Setup-Token: $setup_token"
request 200 /api/v1/setup/status
[[ $(field network_state) == ap ]]
python3 - "$ws06_tmp/data" <<'PY'
import json, pathlib, stat, sys
p = pathlib.Path(sys.argv[1])
assert json.loads((p/'config.json').read_text())['admin_password_hash'] is None
assert json.loads((p/'config.json').read_text())['network'] is None
for name in ['config.json', 'setup-secrets.json']:
    assert stat.S_IMODE((p/name).stat().st_mode) == 0o600
    assert 'correct-password' not in (p/name).read_text()
PY
stop
printf 'WS06 HTTP smoke: setup proof, Wi-Fi rollback/connect, cookie auth, CSRF, logout/login, restart persistence, recovery and factory reset passed\n'
