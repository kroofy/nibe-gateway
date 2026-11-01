#!/usr/bin/env bash
# Fresh-card image from a signed Alpine OS bundle: the same squashfs and slot files as an OS update, plus
# the pinned Raspberry Pi firmware, the shared config.txt, an empty slot B and a seeded data partition. Rootless, no
# loop devices: partition table, FAT (mtools/mkfs.fat --offset) and ext4 (mke2fs -d -E offset) are written straight
# into the image file inside the builder container.
# Usage: scripts/build-alpine-image.sh BUNDLE.tar [OUTPUT_DIR]
#   -> OUTPUT_DIR/nibe-gateway-alpine-<version>.img.xz and .img.xz.sha256 (default target/alpine)
# Environment: NIBE_DISK_ID (8 hex digits; default random), NIBE_ALPINE_IMAGE (alpine:3.24),
#   NIBE_ALPINE_WORKDIR (~/.cache/nibe-gateway/alpine-work; keeps the uncompressed sparse image for QEMU checks)
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
[[ $# == 1 || $# == 2 ]] || { echo 'usage: build-alpine-image.sh BUNDLE.tar [OUTPUT_DIR]' >&2; exit 2; }
bundle=$(realpath "$1") outdir=${2:-$repo/target/alpine}
[[ -f $bundle ]] || { echo "bundle $bundle not found" >&2; exit 1; }
manifest=$(tar -xOf "$bundle" manifest.json) || { echo 'not an OS bundle (no manifest.json)' >&2; exit 1; }
version=$(python3 -c 'import json,sys; m=json.loads(sys.argv[1]); assert m["kind"]=="os"; print(m["version"])' "$manifest")
[[ $version != *smoke* ]] || { echo 'refusing a smoke bundle for a fresh card' >&2; exit 2; }
disk_id=${NIBE_DISK_ID:-}
while [[ -z $disk_id || $disk_id == 00000000 ]]; do disk_id=$(od -An -tx4 -N4 /dev/urandom | tr -d ' \n'); done
[[ $disk_id =~ ^[0-9a-f]{8}$ ]] || { echo 'NIBE_DISK_ID must be 8 lowercase hex digits' >&2; exit 2; }
source "$repo/packaging/alpine/firmware.lock"
cache=${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway
firmware=$cache/$RPI_FIRMWARE_DEB
if [[ ! -f $firmware ]]; then
    mkdir -p "$cache"
    curl -fsSL -o "$firmware.part" "$RPI_FIRMWARE_URL"
    mv "$firmware.part" "$firmware"
fi
echo "$RPI_FIRMWARE_SHA256  $firmware" | sha256sum -c --quiet - || { echo 'firmware package checksum mismatch' >&2; exit 1; }
work=${NIBE_ALPINE_WORKDIR:-$cache/alpine-work}
mkdir -p "$work" "$outdir"
docker run --rm --platform linux/amd64 -v "$repo/packaging:/src:ro" -v "$bundle:/bundle/bundle.tar:ro" \
    -v "$firmware:/fw/firmware.deb:ro" -v "$work:/work" -v "$(realpath "$outdir"):/out" \
    -e VERSION="$version" -e DISK_ID="$disk_id" "${NIBE_ALPINE_IMAGE:-alpine:3.24}" sh /src/alpine/build-card.sh
img=$outdir/nibe-gateway-alpine-$version.img.xz
echo "Card image: $img ($(du -h "$img" | cut -f1)), disk id $disk_id"
echo "Verify:  (cd $(dirname "$img") && sha256sum -c $(basename "$img").sha256)"
echo "Flash:   xzcat $img | sudo dd of=/dev/sdX bs=4M conv=fsync status=progress   (or Raspberry Pi Imager: custom image)"
