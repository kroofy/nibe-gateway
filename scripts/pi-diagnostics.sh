#!/usr/bin/env bash
# Read-only diagnostics of the Alpine appliance over key-only SSH (ADR 0117/0118). Logs are busybox syslogd's
# /var/log/messages (RAM), services are OpenRC, boot traces live on the data partition.
# Usage: NIBE_SSH_HOST=nibe-admin@<pi> scripts/pi-diagnostics.sh logs|logs-all|debug|traces [--fetch DIR]
#   logs      follow our services, NetworkManager, wpa_supplicant and tailscaled (from the last 400 lines, then live)
#   logs-all  follow everything in /var/log/messages
#   debug     services, updater/OS state, /health, network, memory, recent warnings and kernel messages
#   traces    summary of the boot traces (scripts/alpine-trial-report.sh; --fetch DIR copies them)
set -euo pipefail
: "${NIBE_SSH_HOST:?Set NIBE_SSH_HOST to your SSH config alias (key authentication required)}"
# The host must be a hostname/SSH alias, never a command or an option.
[[ $NIBE_SSH_HOST =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.@-]*$ ]] || { echo 'Invalid SSH host' >&2; exit 2; }
# syslog lines are "Mon DD HH:MM:SS host facility.priority TAG[pid]: message": match the tag (field 6), not the
# host name (which is nibe-gateway on every line).
ours='^(nibe-|supervise-daemon|polkitd|NetworkManager|wpa_supplicant|tailscaled)'
case "${1:-}" in
    logs) remote="sudo -n tail -n 400 -F /var/log/messages | awk '\$6 ~ /$ours/ { print; fflush() }'" ;;
    logs-all) remote='sudo -n tail -n 100 -F /var/log/messages' ;;
    debug) remote="echo '== services'; rc-status -a
echo '== updater'; sudo -n /usr/lib/nibe-gateway/nibe-updater client status; echo
echo '== OS'; sudo -n /usr/lib/nibe-gateway/nibe-updater os-status; cat /proc/cmdline
echo '== health'; curl -s -m 5 --noproxy '*' http://127.0.0.1:8080/health; echo
echo '== network'; nmcli general; nmcli -t -f NAME,TYPE,AUTOCONNECT,ACTIVE con show; ip -brief addr 2>/dev/null || ip addr
echo '== memory'; free -m; uptime
echo '== warnings (last 40)'; sudo -n grep -E 'warn|err|crit' /var/log/messages | tail -n 40
echo '== kernel (last 20)'; sudo -n dmesg | tail -n 20" ;;
    traces) shift; exec "$(dirname -- "$0")/alpine-trial-report.sh" "$@" ;;
    *) echo 'usage: pi-diagnostics.sh logs|logs-all|debug|traces [--fetch DIR]' >&2; exit 2 ;;
esac
exec ssh -o BatchMode=yes -o StrictHostKeyChecking=yes "$NIBE_SSH_HOST" "$remote"
