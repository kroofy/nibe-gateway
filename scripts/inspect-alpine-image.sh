#!/usr/bin/env bash
# Mount-free inspection of a fresh-card image (scripts/build-alpine-image.sh): partition table = the card layout,
# random disk id, FAT32 bootfs with the pinned firmware, config.txt selecting slot-a/, slot A's cmdline naming the
# image's PARTUUID, an empty slot-b/ and p4, the provisioning template, the squashfs in p2 and the seeded ext4 p3
# (current -> releases/slot-a, state/ owned 995:992, system/). Reads the image only (mtools, debugfs, Python).
# Usage: scripts/inspect-alpine-image.sh IMAGE.img[.xz]
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
[[ $# == 1 && -f $1 ]] || { echo 'usage: inspect-alpine-image.sh IMAGE.img[.xz]' >&2; exit 2; }
image=$(realpath "$1")
docker run --rm --platform linux/amd64 -v "$repo/packaging/alpine:/a:ro" -v "$image:/card/$(basename "$image"):ro" \
    "${NIBE_ALPINE_IMAGE:-alpine:3.24}" sh -c '
set -e
apk add -q --no-progress python3 mtools e2fsprogs-extra xz coreutils >/dev/null
src=$(ls /card/*)
case $src in *.xz) xz -dc "$src" | cp --sparse=always /dev/stdin /tmp/card.img; img=/tmp/card.img ;; *) img=$src ;; esac
python3 /a/card_image.py inspect "$img"'
