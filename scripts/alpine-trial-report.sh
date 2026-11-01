#!/usr/bin/env bash
# Summarise the traces an Alpine trial or smoke boot left behind, read over SSH from whichever slot is
# running now (normally Raspberry Pi OS after the trial returned):
#   /boot/firmware/nibe-trial/<slot>-<n>.log          initramfs stages (+ dmesg tail on failure), FAT, world-readable
#   /var/lib/nibe-gateway/system/boot-logs/<NNNN>-<slot>-<mode>/   userspace snapshots (root only, read with sudo -n)
# Usage: NIBE_SSH_HOST=nibe-admin@nibe-gateway.local scripts/alpine-trial-report.sh [--fetch DIR]
#   --fetch DIR   also copy both trees to DIR/<timestamp>/ for a full read
# Read-only on the Pi. Needs the same SSH access as scripts/ssh-maintenance.sh.
set -euo pipefail
: "${NIBE_SSH_HOST:?set NIBE_SSH_HOST=nibe-admin@<pi>}"
ssh_pi() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$NIBE_SSH_HOST" "$@"; }
fetch=
case ${1:-} in
--fetch) fetch=${2:?--fetch needs a directory} ;;
'') ;;
*) echo 'usage: alpine-trial-report.sh [--fetch DIR]' >&2; exit 2 ;;
esac

ssh_pi 'sudo -n sh -s' <<'REMOTE'
trial=/boot/firmware/nibe-trial
logs=/var/lib/nibe-gateway/system/boot-logs
echo "== now running: slot $(sed -n 's/.*nibe\.slot=\([ab]\).*/\1/p' /proc/cmdline), $(uname -r), up $(cut -d. -f1 /proc/uptime) s"
echo "== os-state: $(cat /var/lib/nibe-gateway/update-state/os-state.json 2>/dev/null || echo none)"
dt() { tr -d '\000' < "/proc/device-tree/$1" 2>/dev/null || echo absent; }
echo "== this slot: serial0=$(dt aliases/serial0) bluetooth=$(dt soc/serial@7e201000/bluetooth/status) $(ls /dev/ttyAMA* 2>/dev/null | tr '\n' ' ')"
echo
echo "== initramfs traces ($trial)"
if ls "$trial"/*.log >/dev/null 2>&1; then
    for f in $(ls -tr "$trial"/*.log); do
        echo "-- ${f##*/} ($(wc -c < "$f") bytes, written $(date -r "$f" '+%F %T' 2>/dev/null))"
        sed -n '/^--- dmesg tail/q;p' "$f" | cut -c1-200
        if grep -q '^--- dmesg tail' "$f"; then
            echo "   dmesg tail (last 25 of the saved lines):"
            sed -n '/^--- dmesg tail/,$p' "$f" | tail -n 25 | sed 's/^/   /' | cut -c1-200
        fi
    done
else
    echo "(none: no trial/smoke boot reached the initramfs, or traces were never written)"
fi
echo
echo "== userspace boot logs ($logs)"
if [ -d "$logs" ] && [ -n "$(ls "$logs" 2>/dev/null)" ]; then
    for d in $(ls -d "$logs"/* | sort); do
        echo "-- ${d##*/}"
        grep -E '^(mode|started|kernel|image):' "$d/meta.txt" 2>/dev/null | cut -c1-160 | sed 's/^/   /'
        # Timing: uptimes on the Alpine slot. The Pi answered on none of its addresses until it was back on the
        # other slot, so "away" is roughly the last uptime below plus one firmware boot (~20 s) on each side.
        last=$(tail -n 1 "$d/timeline.txt" 2>/dev/null | sed -n 's/.* up=\([0-9.]*\) .*/\1/p')
        ev() {
            [ -f "$d/events.txt" ] || { echo "n/a"; return; }
            v=$(sed -n "s/^$1 up=//p" "$d/events.txt" | head -n 1); echo "${v:-$2}"
        }
        echo "   timing (uptime s): default runlevel $(ev default-runlevel never)," \
             "NetworkManager first connected $(ev nm-connected never), smoke reboot $(ev smoke-reboot -)," \
             "shutdown $(ev shutdown -), last record ${last:-?}"
        case $(ev nm-connected never) in
        never | n/a) [ -n "$last" ] && echo "   Pi unreachable on the network for the whole Alpine boot:" \
            "~$(awk -v l="$last" 'BEGIN { printf "%d", l + 40 }') s away from Raspberry Pi OS (uptime + ~2 firmware boots)" ;;
        esac
        if [ -f "$d/uart.txt" ]; then
            echo "   $(sed -n 1p "$d/uart.txt") ($(sed -n 's/^aliases\/serial0: /serial0=/p' "$d/uart.txt"),"\
                 "$(grep -o '/dev/ttyAMA[0-9]*' "$d/uart.txt" | sort -u | tr '\n' ' '))"
        else
            # Older traces: the TTY the PL011 (0x20201000) got tells the same (ttyAMA0 = disable-bt applied).
            pl011=$(grep -o 'serial: ttyAMA[0-9]* at MMIO 0x20201000' "$d/dmesg.txt" 2>/dev/null | grep -o 'ttyAMA[0-9]*')
            case $pl011 in
            ttyAMA0) echo "   disable-bt applied: yes (from dmesg: PL011 is ttyAMA0)" ;;
            '') echo "   disable-bt applied: unknown (no uart.txt, no PL011 line in dmesg)" ;;
            *) echo "   disable-bt applied: no (from dmesg: PL011 is $pl011, Bluetooth holds it)" ;;
            esac
        fi
        [ -f "$d/events.txt" ] && echo "   events: $(tr '\n' ' ' < "$d/events.txt")"
        n=$(wc -l < "$d/timeline.txt" 2>/dev/null || echo 0)
        echo "   timeline: $n snapshots; last:"
        tail -n 3 "$d/timeline.txt" 2>/dev/null | sed 's/^/     /'
        echo "   NetworkManager profiles (name:type:autoconnect:active):"
        grep -E '^[^ ]+:[^:]+:(yes|no):(yes|no)$' "$d/nmcli.txt" 2>/dev/null | sed 's/^/     /'
        bad=$(grep -E '\[ *(stopped|crashed|failed|starting|stopping) *\]' "$d/rc-status.txt" 2>/dev/null \
            | awk '{print $1"("$3")"}' | tr '\n' ' ')
        echo "   services not started: ${bad:-none}"
        echo "   NetworkManager: $(sed -n '2p' "$d/nmcli.txt" 2>/dev/null | tr -s ' ' | cut -c1-120)"
        echo "   wlan0: $(grep -A3 ' wlan0:' "$d/ip-addr.txt" 2>/dev/null | grep -o 'inet [0-9.]*/[0-9]*' | head -1)"
        echo "   health: $(head -c 200 "$d/health.txt" 2>/dev/null)"
        [ -d "$d/shutdown" ] && echo "   shutdown snapshot: $d/shutdown/"
        echo "   errors in messages (last 10):"
        grep -iE 'error|fail|fatal|panic|segfault|watchdog|nibe-reboot|boot-guard' "$d/messages.txt" 2>/dev/null \
            | tail -n 10 | cut -c1-200 | sed 's/^/     /'
        echo "   kernel warnings (last 8):"
        grep -iE 'error|fail|brcmf|firmware|ttyAMA|regdom|oops|bug' "$d/dmesg.txt" 2>/dev/null \
            | tail -n 8 | cut -c1-200 | sed 's/^/     /'
    done
else
    echo "(none: userspace never reached nibe-trace with the data partition mounted)"
fi
REMOTE

if [[ -n $fetch ]]; then
    dest=$fetch/$(date +%Y%m%d-%H%M%S)
    mkdir -p "$dest"
    ssh_pi 'sudo -n tar -C / -cf - boot/firmware/nibe-trial var/lib/nibe-gateway/system/boot-logs 2>/dev/null' \
        | tar -x -C "$dest" || true
    echo "== copied to $dest"
fi
