#!/bin/sh
# Runs in the builder container: a test SD image with the A/B layout and the same labels as a real card:
# p1 bootfs (vfat, config.txt with the slot marker, slot-a/ = the bundle boot files), p2 slot A (the squashfs),
# p3 nibe-data (ext4, seeded like scripts/build-alpine-image.sh, state/ owned 995:992), p4 empty. TRIAL=1 adds a "trying" os-state.
set -eu
apk add -q --no-progress sfdisk e2fsprogs dosfstools mtools >/dev/null
w=/w; o=/out
[ -d /root-src ] || { echo "mount the built root at /root-src" >&2; exit 1; }
rm -f $w/disk.img; truncate -s 256M $w/disk.img
printf 'label: dos\nlabel-id: 0x15be6592\nstart=2048, size=65536, type=c\nsize=131072, type=83\nsize=131072, type=83\nsize=131072, type=83\n' | sfdisk -q $w/disk.img
# p1
truncate -s 32M $w/p1.img; mkfs.vfat -n bootfs $w/p1.img >/dev/null
printf '[all]\nauto_initramfs=1\nenable_uart=1\ndtoverlay=disable-bt\n[all]\n# nibe-slot (managed by nibe-updater; do not edit)\nos_prefix=slot-a/\n' > $w/config.txt
export MTOOLS_SKIP_CHECK=1
mcopy -i $w/p1.img $w/config.txt ::/config.txt; mmd -i $w/p1.img ::/slot-a ::/slot-b; mcopy -s -i $w/p1.img $o/boot/* ::/slot-a/
# p3: seeded like a fresh card (scripts/build-alpine-image.sh), owned by 995:992
rm -rf $w/p3; mkdir -p $w/p3/releases $w/p3/system $w/p3/update-state $w/p3/state
cp -a /root-src/opt/nibe-gateway/factory $w/p3/releases/slot-a; ln -s releases/slot-a $w/p3/current
printf '{"phase":"idle","active":"a","previous":null,"candidate":null,"version":null,"outcome":null}\n' > $w/p3/update-state/state.json
[ -z "${TRIAL:-}" ] || printf %s "{\"phase\":\"trying\",\"from\":\"b\",\"to\":\"a\",\"version\":\"2026.10.25-alpine.1\",\"at\":1,\"detail\":null}" > $w/p3/update-state/os-state.json
chmod 700 $w/p3/update-state $w/p3/state; chown 995:992 $w/p3/state
truncate -s 64M $w/p3.img; mkfs.ext4 -q -L nibe-data -d $w/p3 $w/p3.img
dd if=$w/p1.img of=$w/disk.img bs=512 seek=2048 conv=notrunc status=none
dd if=$o/rootfs.img of=$w/disk.img bs=512 seek=67584 conv=notrunc status=none
dd if=$w/p3.img of=$w/disk.img bs=512 seek=198656 conv=notrunc status=none
sfdisk -l $w/disk.img
