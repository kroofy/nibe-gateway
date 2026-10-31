#!/usr/bin/env bash
# Boot the last Alpine build (scripts/build-alpine-os-bundle.sh) under QEMU raspi0 in the builder container, from a
# test SD image with the real partition layout, and print the probe (mounts, rc-status, processes, /health, logs).
# Verifies kernel + our initramfs + squashfs + overlay + OpenRC + services. Not covered: Wi-Fi, UART, watchdog.
# Usage: scripts/qemu-boot-alpine.sh [--trial]   (env T = QEMU seconds, default 420; work dir as the build script)
#        NO_PROBE=1 boots the image unmodified (e.g. a smoke bundle, which ends by rebooting: QEMU then exits).
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
work=${NIBE_ALPINE_WORKDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway/alpine-work}
[[ -f $work/out/rootfs.img && -d $work/root ]] || { echo "no build in $work: run scripts/build-alpine-os-bundle.sh" >&2; exit 1; }
qemu=$work/qemu
mkdir -p "$qemu"
trial=; [[ ${1:-} == --trial ]] && trial=1
docker run --rm --platform linux/amd64 -v "$repo/packaging/alpine/qemu:/qemu:ro" -v "$qemu:/w" -v "$work/out:/out:ro" \
    -v "$work/root:/root-src:ro" -e TRIAL="$trial" -e T="${T:-420}" -e PROBE="$([[ -n ${NO_PROBE:-} ]] || echo 1)" "${NIBE_ALPINE_IMAGE:-alpine:3.24}" \
    sh -c 'sh /qemu/make-disk.sh >/dev/null && sh /qemu/boot.sh'
log=$qemu/serial.log
grep -aq 'OpenRC' "$log" || { echo "OpenRC never started; see $log" >&2; tail -20 "$log" >&2; exit 1; }
awk '!seen[$0]++' "$log" | grep -aE '^ \* |nibe-initramfs' | grep -aE 'ERROR|nibe|Mounting|FATAL' || true
awk '/=== PROBE begin/,/=== PROBE end/' "$log" | awk '!seen[$0]++'
echo "== left on the card (p1 nibe-trial/, p3 system/boot-logs/): $qemu/card"
for f in "$qemu"/card/p1/nibe-trial/*.log; do [[ -e $f ]] && { echo "-- p1 ${f##*/}"; sed -n '/^--- dmesg tail/q;p' "$f" | cut -c1-160; }; done
for d in "$qemu"/card/p3/boot-logs/*/; do [[ -d $d ]] && { echo "-- p3 $(basename "$d")"; cat "$d/timeline.txt" 2>/dev/null; }; done
grep -aq '=== PROBE end' "$log" || [[ -n ${NO_PROBE:-} ]] || { echo "probe did not finish within T=${T:-420}s; see $log" >&2; exit 1; }
