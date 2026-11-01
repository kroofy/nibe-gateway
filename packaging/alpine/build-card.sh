#!/bin/sh
# Fresh-card image from a signed Alpine OS bundle, rootless and without loop devices. Runs in the builder
# container started by scripts/build-alpine-image.sh. Mounts: /src = packaging (ro), /bundle/bundle.tar (ro),
# /fw/firmware.deb (ro, pinned in packaging/alpine/firmware.lock), /work scratch, /out results.
# Environment: VERSION (bundle version), DISK_ID (8 hex digits, random per image).
# Layout and checks: packaging/alpine/card_image.py (the owner's card). The image holds only p1 and p2 cut
# to the squashfs; the first boot completes p2-p4 and seeds p3 (packaging/alpine/initramfs/init).
set -eu
src=/src/alpine
work=/work/card
out=/out
image=$work/nibe-gateway-alpine-$VERSION.img
step() { echo "== $*"; }
py() { python3 "$src/card_image.py" "$@"; }

step "builder tools"
apk add -q --no-progress python3 dosfstools mtools squashfs-tools xz binutils sfdisk >/dev/null
rm -rf "$work"; mkdir -p "$work/bundle" "$work/fw" "$out"

step "bundle $VERSION: manifest checksums"
tar -xf /bundle/bundle.tar -C "$work/bundle" manifest.json boot.tar rootfs.img
python3 - "$work/bundle" <<'PY'
import hashlib, json, pathlib, sys
b = pathlib.Path(sys.argv[1])
m = json.loads((b / "manifest.json").read_text())
def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()
assert m["kind"] == "os" and m["compatibility"] == "nibe-gateway-os-v1", "not an OS bundle"
assert sha(b / "rootfs.img") == m["rootfs_sha256"], "rootfs.img checksum mismatch"
assert sha(b / "boot.tar") == m["boot_sha256"], "boot.tar checksum mismatch"
print(f"bundle {m['version']}: rootfs.img {m['rootfs_size']} bytes, checksums match the signed manifest")
PY
mkdir -p "$work/bundle/boot"
tar -xf "$work/bundle/boot.tar" -C "$work/bundle/boot"
if grep -qw 'nibe.smoke=1' "$work/bundle/boot/cmdline.txt"; then
    echo "refusing a smoke bundle for a fresh card" >&2; exit 1
fi

step "firmware (pinned raspi-firmware)"
(cd "$work/fw" && ar x /fw/firmware.deb && tar -xzf data.tar.gz)
fw=$work/fw/usr/lib/raspi-firmware
# shellcheck disable=SC1091
. "$src/firmware.lock"
[ "$(md5sum "$fw/start.elf" | cut -d' ' -f1)" = "$RPI_START_ELF_MD5" ] || { echo "start.elf is not the card's" >&2; exit 1; }

step "root: what the first boot seeds p3 from"
unsquashfs -q -no-progress -d "$work/root" "$work/bundle/rootfs.img" opt/nibe-gateway/factory/manifest.json \
    etc/passwd etc/group sbin/mke2fs etc/ssh >/dev/null
awk -F: '$1 == "nibe-gateway" {f=1} END {exit !f}' "$work/root/etc/passwd" &&
    awk -F: '$1 == "nibe-gateway" {f=1} END {exit !f}' "$work/root/etc/group" ||
    { echo "nibe-gateway account missing in the root" >&2; exit 1; }
for need in opt/nibe-gateway/factory/manifest.json sbin/mke2fs etc/ssh; do
    [ -e "$work/root/$need" ] || [ -L "$work/root/$need" ] || { echo "root lacks /$need (first boot needs it)" >&2; exit 1; }
done

step "partition table (disk id $DISK_ID): p1, p2 = the squashfs; p3/p4 come with the first boot"
rm -f "$image"
# shellcheck disable=SC2046  # two words: p2 sectors, image bytes
set -- $(py p2 "$work/bundle/rootfs.img")
p2_sectors=$1
truncate -s "$2" "$image"
py mbr "$image" "$DISK_ID" "$p2_sectors"
sfdisk -d "$image"

step "p1 bootfs: firmware, config.txt, slot-a, slot-b, provisioning template, README"
p1=$((16384 * 512))
mkfs.fat --offset 16384 -h 16384 -F 32 -s 4 -n bootfs -i "$(od -An -tx4 -N4 /dev/urandom | tr -d ' ')" "$image" 524288 >/dev/null
export MTOOLS_SKIP_CHECK=1
for f in "$fw"/bootcode.bin "$fw"/start*.elf "$fw"/fixup*.dat "$fw"/LICENCE.broadcom; do
    mcopy -i "$image@@$p1" "$f" "::/$(basename "$f")"
done
mcopy -i "$image@@$p1" "$src/boot/config.txt" ::/config.txt
mcopy -i "$image@@$p1" "$src/boot/nibe-provision.json.template" ::/nibe-provision.json.template
sed "s/@VERSION@/$VERSION/; s/@DISKID@/$DISK_ID/" "$src/boot/README.txt" > "$work/README.txt"
mcopy -i "$image@@$p1" "$work/README.txt" ::/README.txt
cat > "$work/first-boot.txt" <<'EOF'
Fresh NIBE Gateway card. The first boot completes the partition table (slot A, data, slot B) and creates the data
partition, then deletes this file. Do not copy this file to another card: it tells the first boot to recreate the
data partition. If the first boot fails, write the image to the card again.
EOF
mcopy -i "$image@@$p1" "$work/first-boot.txt" ::/nibe-first-boot
py cmdline "$work/bundle/boot/cmdline.txt" "$DISK_ID" > "$work/cmdline.txt"
cp "$work/cmdline.txt" "$work/bundle/boot/cmdline.txt"
mmd -i "$image@@$p1" ::/slot-a ::/slot-b
mcopy -s -i "$image@@$p1" "$work/bundle/boot/"* ::/slot-a/

step "p2 slot A: the bundle's squashfs"
dd if="$work/bundle/rootfs.img" of="$image" bs=512 seek=1064960 conv=notrunc status=none

step "inspection"
py inspect "$image"

step "compress"
rm -f "$out/$(basename "$image").xz" "$out/$(basename "$image").xz.sha256"
xz -T0 -6 -k -c "$image" > "$out/$(basename "$image").xz"
(cd "$out" && sha256sum "$(basename "$image").xz" > "$(basename "$image").xz.sha256")
echo "image: $(stat -c %s "$image") bytes raw, $(du -k "$image" | cut -f1) KiB allocated," \
    "$(stat -c %s "$out/$(basename "$image").xz") bytes xz"
