#!/usr/bin/env bash
# Boot a fresh-card image (scripts/build-alpine-image.sh) under QEMU raspi0 from its own p1 slot-a files with the
# image as the SD card (grown to 8 GiB, with an older OS's leftovers at p3/p4), twice, then check what the boots did
# on the card. Boot 1 = the first boot: the initramfs completes the layout, wipes p3/p4's first MiB,
# creates and seeds p3 and removes the marker; then finds slot A by the image's random PARTUUID, overlay root, p1/p3
# label mounts, nibe-persist seeding of system/, default runlevel, boot trace. Boot 2 must not touch the layout or p3.
# Not covered (no Pi firmware, no Wi-Fi/UART in QEMU): config.txt, the setup AP, a real card's controller.
# Usage: scripts/qemu-boot-alpine-image.sh IMAGE.img[.xz]   (env T = QEMU seconds per boot, default 300)
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
[[ $# == 1 && -f $1 ]] || { echo 'usage: qemu-boot-alpine-image.sh IMAGE.img[.xz]' >&2; exit 2; }
image=$(realpath "$1")
w=${NIBE_ALPINE_WORKDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway/alpine-work}/qemu-card
rm -rf "$w" 2>/dev/null || docker run --rm -v "$(dirname "$w"):/c" alpine:3.24 rm -rf /c/qemu-card
mkdir -p "$w"
docker run --rm --platform linux/amd64 -v "$repo/packaging/alpine:/a:ro" -v "$image:/card/$(basename "$image"):ro" \
    -v "$w:/w" -e T="${T:-300}" "${NIBE_ALPINE_IMAGE:-alpine:3.24}" sh /a/qemu/boot-card.sh
fail=0
check() { if eval "$2"; then echo "PASS $1"; else echo "FAIL $1"; fail=1; fi; }
log=$w/serial-1.log log2=$w/serial-2.log
boot=$(ls -d "$w"/p3/boot-logs/*/ 2>/dev/null | head -n 1)
nboots=$(ls -d "$w"/p3/boot-logs/*/ 2>/dev/null | wc -l)
echo "image: $(cat "$w/image-bytes.txt") bytes raw (virtual card 8 GiB)"
grep -a 'nibe-initramfs' "$log" | awk '!seen[$0]++' | sed 's/^/  /'
check "first boot: fresh card detected (short layout, marker)" "grep -aq 'first boot: layout short, marker 1, plan layout' '$log'"
check "first boot: partition table completed and re-read" "grep -aq 'first boot: partition table complete' '$log'"
check "first boot: p3 created and seeded, marker removed" "grep -aq 'first boot: complete, nibe-first-boot removed' '$log'"
check "after boot 1: full layout, p4 wiped, no marker, p3 seed (card_image.py card)" "grep -q 'card: full layout' '$w/card-1.txt'"
check "initramfs found slot A by the image's PARTUUID" "grep -aq 'nibe-initramfs: root PARTUUID=[0-9a-f]*-02 is /dev/mmcblk0p2' '$log'"
check "OpenRC started" "grep -q 'rc boot logging started' \"${boot}rc.log\" 2>/dev/null"
check "first boot wrote a boot trace on p3" "[[ -n '$boot' && -f '${boot}timeline.txt' ]]"
check "default runlevel reached" "grep -q 'default=yes' '${boot}timeline.txt' 2>/dev/null"
check "nibe-persist seeded system/tailscale and the clock" "grep -q ' tailscale' '$w/p3-system.txt' && grep -q ' clock' '$w/p3-system.txt'"
check "the factory release is current" "grep -q 'current' '$w/p3-root.txt'"
check "first boot left its trace on p1 (nibe-trial/a-1.log)" "grep -q 'nibe-trial/a-1.log' '$w/p1-after-1.txt'"
check "Wi-Fi fallback ran and found no station profile (fresh card)" "grep -q 'wifi-fallback-no-profile' '${boot}events.txt' 2>/dev/null"
check "disable-bt in effect (PL011 = ttyAMA0)" "grep -q 'disable-bt applied: yes' '${boot}uart.txt' 2>/dev/null"
check "boot 2: card left alone (plan none)" "grep -aq 'first boot: layout full, marker 0, plan none' '$log2'"
check "boot 2: partition table unchanged" "cmp -s '$w/mbr-1.bin' '$w/mbr-2.bin'"
check "boot 2: p3 not recreated (same filesystem UUID)" "[[ -s '$w/p3-uuid-1.txt' ]] && cmp -s '$w/p3-uuid-1.txt' '$w/p3-uuid-2.txt'"
check "boot 2: reached the default runlevel too (2 boot traces on p3)" "[[ $nboots == 2 ]] && grep -q 'default=yes' \"\$(ls -d '$w'/p3/boot-logs/*/ | tail -n 1)timeline.txt\""
check "boot 2: a normal boot leaves p1 alone (no new nibe-trial/ file)" "! grep -q 'nibe-trial/a-2.log' '$w/p1-after-2.txt'"
[[ -n $boot ]] && { echo "-- ${boot#"$w"/}"; cat "${boot}timeline.txt" 2>/dev/null; grep -E '\[ *(failed|crashed) *\]' "${boot}rc-status.txt" 2>/dev/null || true; }
cat "$w/card-1.txt"
echo "Logs: $w (serial-1.log, serial-2.log, card-1.txt, p3/, p1-after-N.txt)"
exit $fail
