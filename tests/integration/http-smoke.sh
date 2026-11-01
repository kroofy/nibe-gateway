#!/usr/bin/env bash
# Real executable + curl contract, including SPA fallback and SIGTERM with open SSE.
set -euo pipefail
cd "$(dirname "$0")/../.."
mode="${1:-default}"
case "$mode" in
  default) cargo build -p nibe-gateway; extra=() ;;
  dev) cargo build -p nibe-gateway --features dev; extra=(--allow-simulated-controls) ;;
  *) echo 'usage: http-smoke.sh [default|dev]' >&2; exit 2 ;;
esac
tmp_dir=$(mktemp -d)
gateway_pid=''
sse_pid=''
cleanup() {
  if [[ -n "$sse_pid" ]]; then kill "$sse_pid" 2>/dev/null || true; wait "$sse_pid" 2>/dev/null || true; fi
  if [[ -n "$gateway_pid" ]]; then kill "$gateway_pid" 2>/dev/null || true; wait "$gateway_pid" 2>/dev/null || true; fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
mkdir "$tmp_dir/admin"
printf '<html><body>static smoke</body></html>' > "$tmp_dir/admin/index.html"
# Port zero avoids collisions with another agent's server; read the assigned port from the log.
"${CARGO_TARGET_DIR:-target}/debug/nibe-gateway" --data-dir "$tmp_dir/data" --device simulated --bind 127.0.0.1:0 --freeze-time --admin-dir "$tmp_dir/admin" "${extra[@]}" > "$tmp_dir/gateway.log" 2>&1 &
gateway_pid=$!
address=''
for _ in {1..100}; do
  address=$(sed -nE 's/.*address[^0-9]*(127\.0\.0\.1:[0-9]+).*/\1/p' "$tmp_dir/gateway.log" | head -1)
  if [[ -n "$address" ]]; then break; fi
  if ! kill -0 "$gateway_pid" 2>/dev/null; then cat "$tmp_dir/gateway.log" >&2; exit 1; fi
  sleep 0.05
done
[[ -n "$address" ]] || { cat "$tmp_dir/gateway.log" >&2; exit 1; }
base="http://$address"
[[ $(curl -sS -o /dev/null -w '%{http_code}' "$base/api/v1/device") == 401 ]]
setup_token=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["setup_token"])' "$tmp_dir/data/setup-secrets.json")
curl -fsS -c "$tmp_dir/cookies" -H 'X-Requested-With: nibe-gateway' -H "X-Setup-Token: $setup_token" -H 'Content-Type: application/json' -d '{"password":"smoke-admin-password"}' "$base/api/v1/setup/admin" > /dev/null
curl() { command curl -b "$tmp_dir/cookies" -H 'X-Requested-With: nibe-gateway' "$@"; }
curl -fsS "$base/health" > "$tmp_dir/health.json"
curl -fsS "$base/api/v1/device" > "$tmp_dir/device.json"
curl -fsS "$base/openapi.json" > "$tmp_dir/openapi.json"
curl -fsS -X POST "$base/api/v1/discovery/scan" > "$tmp_dir/discovery.json"
curl -fsS "$base/settings" > "$tmp_dir/spa.html"
[[ $(curl -sS -o "$tmp_dir/missing.json" -w '%{http_code}' "$base/api/v1/unknown") == 404 ]]
[[ $(curl -sS -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{"value":10}' "$base/api/v1/registers/1") == 404 ]]
if [[ "$mode" == default ]]; then
  [[ $(curl -sS -o "$tmp_dir/control.json" -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{"offset":2}' "$base/api/v1/controls/heating-offset") == 403 ]]
  [[ $(curl -sS -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{"name":"cold_weather"}' "$base/__dev/scenario") == 404 ]]
else
  curl -fsS -X POST -H 'Content-Type: application/json' -d '{"offset":2}' "$base/api/v1/controls/heating-offset" > "$tmp_dir/control.json"
  curl -fsS -H 'Content-Type: application/json' -d '{"seconds":300,"confirmation":"AUTHORIZE_DIAGNOSTICS"}' "$base/api/v1/diagnostics/developer" > /dev/null
  curl -fsS -X POST -H 'Content-Type: application/json' -d '{"kind":"stale"}' "$base/__dev/fault" > /dev/null
  curl -fsS -X POST -H 'Content-Type: application/json' -d '{"milliseconds":6000}' "$base/__dev/time/advance" > "$tmp_dir/stale.json"
  curl -fsS -X POST "$base/__dev/reset" > "$tmp_dir/reset.json"
fi
python3 - "$tmp_dir" "$mode" <<'PY'
import json, pathlib, sys
p, mode = pathlib.Path(sys.argv[1]), sys.argv[2]
read = lambda name: json.loads((p / name).read_text())
assert read('health.json')['status'] == 'ok'
assert read('device.json')['identity']['verification'] == 'synthetic'
assert read('device.json')['measurements'][0]['value']['value'] == 7.5
assert not read('device.json')['stale']
assert not read('discovery.json')['active_probing_supported']
assert 'static smoke' in (p/'spa.html').read_text()
assert read('missing.json')['error']['code'] == 'unsupported'
if mode == 'default':
    assert read('control.json')['error']['code'] == 'controls_disabled'
    assert not any(path.startswith('/__dev/') for path in read('openapi.json')['paths'])
else:
    assert read('control.json')['command']['offset'] == 2
    assert read('stale.json')['stale']
    assert not read('reset.json')['stale']
PY
curl -fsSN --max-time 10 "$base/api/v1/events" > "$tmp_dir/events.txt" &
sse_pid=$!
for _ in {1..50}; do
  if rg -q 'event: state' "$tmp_dir/events.txt"; then break; fi
  sleep 0.05
done
rg -q 'event: state' "$tmp_dir/events.txt"
# Open SSE must close during graceful shutdown, rather than keeping systemd waiting.
kill -TERM "$gateway_pid"
for _ in {1..50}; do
  if ! kill -0 "$gateway_pid" 2>/dev/null; then break; fi
  sleep 0.05
done
if kill -0 "$gateway_pid" 2>/dev/null; then echo 'gateway failed to shut down with open SSE' >&2; exit 1; fi
wait "$gateway_pid"
gateway_pid=''
wait "$sse_pid"
sse_pid=''
printf 'HTTP smoke (%s): health, device, OpenAPI, passive discovery, controls, static SPA, SSE, SIGTERM passed\n' "$mode"
