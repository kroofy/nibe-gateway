#!/bin/sh
# Runs in the builder container: boot a fresh-card image under QEMU raspi0 with the image itself as the SD card,
# twice. QEMU cannot run the Pi firmware, so the kernel, dtb, initramfs and cmdline are taken from the image's own
# p1:slot-a/ (what the firmware would load with os_prefix=slot-a/). /card/IMAGE (.img or .img.xz) is never modified:
# the run uses a sparse copy grown to an 8 GiB card (a real card is larger than the image; QEMU's SD model also needs
# a power-of-two size) with an older OS's leftovers where p3 and p4 land, as on the owner's card.
# Boot 1 is the first boot (ADR 0129: the initramfs completes the layout and seeds p3); boot 2 must leave the layout
# and p3 alone. Results in /w: serial-1.log, serial-2.log, card-1.txt (card_image.py card after boot 1), mbr-N.bin,
# p3-uuid-N.txt, p1-after-N.txt, and the p3 dumps after boot 2. /a = packaging/alpine (read-only).
set -eu
apk add -q --no-progress qemu-system-arm dtc mtools e2fsprogs-extra xz coreutils python3 >/dev/null 2>&1
src=$(ls /card/*.img.xz /card/*.img 2>/dev/null | head -n 1)
sd=/w/sd.img
rm -f "$sd"
# shellcheck disable=SC2216  # cp reads the pipe through /dev/stdin and writes a sparse copy
case $src in
*.xz) xz -dc "$src" | cp --sparse=always /dev/stdin "$sd" ;;
*) cp --sparse=always "$src" "$sd" ;;
esac
stat -c %s "$sd" > /w/image-bytes.txt
truncate -s 8G "$sd"
p1=$((16384 * 512)); p3=$((5111808 * 512))
# An older OS on the card: an ext4 superblock labelled rootfs where p3 starts, non-zero data where p4 starts.
python3 - "$sd" <<'PY'
import sys
with open(sys.argv[1], "r+b") as f:
    sb = bytearray(4096)
    sb[1024 + 56:1024 + 58] = b"\x53\xef"
    sb[1024 + 120:1024 + 126] = b"rootfs"
    f.seek(5111808 * 512); f.write(sb)
    f.seek(6160384 * 512); f.write(b"hsqs" + b"\xa5" * ((1 << 20) - 4))
PY
export MTOOLS_SKIP_CHECK=1
rm -rf /w/slot; mkdir -p /w/slot
for f in kernel.img initramfs bcm2708-rpi-zero-w.dtb cmdline.txt; do mcopy -i "$sd@@$p1" "::/slot-a/$f" /w/slot/; done
cp /w/slot/bcm2708-rpi-zero-w.dtb /w/q.dtb
# QEMU's raspi0 lacks the BCM2835 PM/power block (the kernel oopses probing it): disable it and the watchdog.
for node in $(fdtget -l /w/q.dtb /soc | grep -E '^(watchdog|power)'); do fdtput -t s /w/q.dtb "/soc/$node" status disabled; done
# Keep the image's console=tty1 so the UART stays free for the gateway; kernel messages via earlycon only.
cmdline="$(tr -d '\n' < /w/slot/cmdline.txt) earlycon=pl011,0x20201000 keep_bootcon"
echo "cmdline (from the image, console added for QEMU): $cmdline"
for n in 1 2; do
    echo "== boot $n"
    timeout "${T:-300}" qemu-system-arm -M raspi0 -m 512 -display none -no-reboot \
      -kernel /w/slot/kernel.img -dtb /w/q.dtb -initrd /w/slot/initramfs \
      -append "$cmdline" -drive file="$sd",if=sd,format=raw -serial stdio -serial null > "/w/serial-$n.log" 2>&1 || true
    dd if="$sd" of="/w/mbr-$n.bin" bs=512 count=1 status=none
    od -A n -t x1 -j $((p3 + 1024 + 104)) -N 16 "$sd" | tr -d ' \n' > "/w/p3-uuid-$n.txt"
    mdir -i "$sd@@$p1" -/ -b ::/ > "/w/p1-after-$n.txt" 2>&1 || true
    if [ "$n" = 1 ]; then python3 /a/card_image.py card "$sd" > /w/card-1.txt 2>&1 || true; fi
done
rm -rf /w/p3; mkdir -p /w/p3
for path in boot-logs NetworkManager connections ssh tailscale; do
    debugfs -R "rdump /system/$path /w/p3" "$sd?offset=$p3" >/dev/null 2>&1 || true
done
debugfs -R "ls -l /" "$sd?offset=$p3" > /w/p3-root.txt 2>/dev/null
debugfs -R "ls -l /state" "$sd?offset=$p3" > /w/p3-state.txt 2>/dev/null
debugfs -R "ls -l /system" "$sd?offset=$p3" > /w/p3-system.txt 2>/dev/null
chmod -R a+rX /w
rm -f "$sd"
