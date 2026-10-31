#!/usr/bin/env bash
# Boot a fresh-card image (scripts/build-alpine-image.sh) under QEMU raspi0 from its own p1 slot-a files with the
# image as the SD card, then check what the first boot did on the card. Verifies: initramfs finds slot A by the
# image's random PARTUUID, the overlay root, p1/p3 label mounts, nibe-persist seeding of system/, the default
# runlevel and the boot trace. Not covered (no Pi firmware, no Wi-Fi/UART in QEMU): config.txt, the setup AP.
# Usage: scripts/qemu-boot-alpine-image.sh IMAGE.img[.xz]   (env T = QEMU seconds, default 300)
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
[[ $# == 1 && -f $1 ]] || { echo 'usage: qemu-boot-alpine-image.sh IMAGE.img[.xz]' >&2; exit 2; }
image=$(realpath "$1")
w=${NIBE_ALPINE_WORKDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway/alpine-work}/qemu-card
rm -rf "$w" 2>/dev/null || docker run --rm -v "$(dirname "$w"):/c" alpine:3.24 rm -rf /c/qemu-card
mkdir -p "$w"
docker run --rm --platform linux/amd64 -v "$repo/packaging/alpine/qemu:/qemu:ro" -v "$image:/card/$(basename "$image"):ro" \
    -v "$w:/w" -e T="${T:-300}" "${NIBE_ALPINE_IMAGE:-alpine:3.24}" sh /qemu/boot-card.sh
fail=0
check() { if eval "$2"; then echo "PASS $1"; else echo "FAIL $1"; fail=1; fi; }
log=$w/serial.log
boot=$(ls -d "$w"/p3/boot-logs/*/ 2>/dev/null | head -n 1)
grep -a 'nibe-initramfs' "$log" | awk '!seen[$0]++' | sed 's/^/  /'
check "initramfs found slot A by the image's PARTUUID" "grep -aq 'nibe-initramfs: root PARTUUID=[0-9a-f]*-02 is /dev/mmcblk0p2' '$log'"
check "OpenRC started" "grep -q 'rc boot logging started' \"${boot}rc.log\" 2>/dev/null"
check "first boot wrote a boot trace on p3" "[[ -n '$boot' && -f '${boot}timeline.txt' ]]"
check "default runlevel reached" "grep -q 'default=yes' '${boot}timeline.txt' 2>/dev/null"
check "nibe-persist seeded system/tailscale and the clock" "grep -q ' tailscale' '$w/p3-system.txt' && grep -q ' clock' '$w/p3-system.txt'"
check "the factory release is current" "grep -q 'current' '$w/p3-root.txt'"
check "a normal (committed) boot leaves p1 alone (no nibe-trial/)" "! grep -q 'nibe-trial' '$w/p1-after.txt'"
check "Wi-Fi fallback ran and found no station profile (fresh card)" "grep -q 'wifi-fallback-no-profile' '${boot}events.txt' 2>/dev/null"
check "disable-bt in effect (PL011 = ttyAMA0)" "grep -q 'disable-bt applied: yes' '${boot}uart.txt' 2>/dev/null"
[[ -n $boot ]] && { echo "-- ${boot#"$w"/}"; cat "${boot}timeline.txt" 2>/dev/null; grep -E '\[ *(failed|crashed) *\]' "${boot}rc-status.txt" 2>/dev/null || true; }
echo "Logs: $w (serial.log, p3/, p1-after.txt)"
exit $fail
