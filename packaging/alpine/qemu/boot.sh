#!/bin/sh
# Runs in the builder container: add the probe to a copy of the squashfs, then boot the slot under QEMU raspi0.
set -e
apk add -q --no-progress qemu-system-arm dtc squashfs-tools >/dev/null 2>&1
if [ -n "${PROBE:-}" ]; then
    rm -rf /tmp/r; unsquashfs -q -d /tmp/r /out/rootfs.img >/dev/null
    install -m 755 /qemu/probe.start /tmp/r/etc/local.d/probe.start
    ln -s /etc/init.d/local /tmp/r/etc/runlevels/default/local
    mksquashfs /tmp/r /w/probe.img -noappend -comp zstd -b 131072 -quiet -no-progress
    dd if=/w/probe.img of=/w/disk.img bs=512 seek=67584 conv=notrunc status=none
fi
# QEMU's raspi0 lacks the BCM2835 PM/power block (the kernel oopses probing it): disable it and with it the watchdog
# in a copy of the dtb. QEMU applies no overlays: the PL011 is ttyAMA0 only because the shipped dtb has disable-bt
# pre-applied (ADR 0118) - this run checks exactly that.
cp /out/boot/bcm2708-rpi-zero-w.dtb /tmp/q.dtb
for node in $(fdtget -l /tmp/q.dtb /soc | grep -E '^(watchdog|power)'); do fdtput -t s /tmp/q.dtb "/soc/$node" status disabled; done
cmdline="$(sed 's/root=PARTUUID=00000000-02/root=PARTUUID=15be6592-02/; s/console=tty1/console=ttyAMA0,115200 earlycon=pl011,0x20201000 keep_bootcon/; s/ quiet//' /out/boot/cmdline.txt)"
echo "cmdline: $cmdline"
timeout "${T:-600}" qemu-system-arm -M raspi0 -m 512 -display none -no-reboot \
  -kernel /out/boot/kernel.img -dtb /tmp/q.dtb -initrd /out/boot/initramfs \
  -append "$cmdline" -drive file=/w/disk.img,if=sd,format=raw -serial stdio -serial null > /w/serial.log 2>&1 || true
# What the boot left on the card: initramfs traces on p1, userspace boot logs on p3.
apk add -q --no-progress mtools e2fsprogs-extra >/dev/null 2>&1
dd if=/w/disk.img of=/tmp/p1.img bs=512 skip=2048 count=65536 status=none
dd if=/w/disk.img of=/tmp/p3.img bs=512 skip=198656 count=131072 status=none
rm -rf /w/card; mkdir -p /w/card/p1 /w/card/p3
MTOOLS_SKIP_CHECK=1 mcopy -s -n -i /tmp/p1.img ::/nibe-trial /w/card/p1/ 2>/dev/null || true
debugfs -R "rdump /system/boot-logs /w/card/p3" /tmp/p3.img >/dev/null 2>&1 || true
chmod -R a+rX /w/card
