#!/bin/sh
# Builds the Alpine appliance root, slot boot files and squashfs (ADR 0117). Runs inside a native (amd64) Alpine
# container started by scripts/build-alpine-os-bundle.sh; the armhf root is populated with `apk --arch armhf` and
# its package scripts, chroot steps and depmod run through the host's qemu-arm binfmt handler.
# Mounts: /src = packaging (read-only), /work = payload in, out/ results, /cache = apk cache,
#         /kernel/kernel.deb = the pinned Raspberry Pi OS kernel package (packaging/alpine/kernel.lock, hash checked).
# Environment: NIBE_ALPINE_BRANCH (v3.24), NIBE_WIFI_COUNTRY, NIBE_GATEWAY_UID/GID, NIBE_IMAGE_PROFILE, NIBE_SMOKE.
set -eu
src=/src/alpine
shared=/src/common
payload=/work/payload
out=/work/out
root=/work/root
branch=${NIBE_ALPINE_BRANCH:-v3.24}
country=${NIBE_WIFI_COUNTRY:-SE}
uid=${NIBE_GATEWAY_UID:-995}
gid=${NIBE_GATEWAY_GID:-992}
profile=${NIBE_IMAGE_PROFILE:-hardware}
mirror=${NIBE_ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}
smoke=${NIBE_SMOKE:-0}
# shellcheck disable=SC1091
. "$src/kernel.lock"
kver=$RPI_KERNEL_VERSION
step() { echo "== $*"; }

step "builder tools"
apk add -q --no-progress python3 squashfs-tools xz zstd binutils dtc >/dev/null
py() { python3 "$src/alpine_image.py" "$@"; }

rm -rf "$root" "$out"
mkdir -p "$root/etc/apk" "$out/boot/overlays"
printf '%s/%s/main\n%s/%s/community\n' "$mirror" "$branch" "$mirror" "$branch" > "$root/etc/apk/repositories"
mkdir -p "$root/etc/apk/keys"
cp /usr/share/apk/keys/armhf/* "$root/etc/apk/keys/"

step "install world ($(py world "$src/world" | wc -l) packages listed)"
# shellcheck disable=SC2046
apk add --root "$root" --initdb --arch armhf --no-progress --cache-dir /cache \
    --keys-dir "$root/etc/apk/keys" --repositories-file "$root/etc/apk/repositories" \
    $(py world "$src/world") > "$out/apk.log" 2>&1 || { tail -40 "$out/apk.log"; exit 1; }
grep -iE 'error|warning' "$out/apk.log" | grep -v 'tailscaled runs as root' || true
[ ! -d "$root/lib/modules" ] || [ -z "$(ls "$root/lib/modules")" ] \
    || { echo "an Alpine kernel was pulled in: $(ls "$root/lib/modules")" >&2; exit 1; }
apk --root "$root" info -q | sort > "$out/packages.txt"
# shellcheck disable=SC2046
apk --root "$root" info -s $(cat "$out/packages.txt") 2>/dev/null \
    | awk '/ installed size:$/ { name=$1; getline; print $1, $2, name }' \
    | sed -E 's/-[0-9][^ ]*-r[0-9]+$//' | awk 'NR == FNR { installed[$1] = 1; next } installed[$3]' \
        "$out/packages.txt" - | sort -t' ' -k3 > "$out/package-sizes.txt"
apk --root "$root" info -v 2>/dev/null | sort > "$out/package-versions.txt"

step "accounts (nibe-gateway $uid:$gid as on Raspberry Pi OS, nibe-admin)"
chroot "$root" /bin/sh -e <<EOF
addgroup -g $gid nibe-gateway
adduser -S -D -H -u $uid -G nibe-gateway -h /var/lib/nibe-gateway/state -s /sbin/nologin nibe-gateway
addgroup nibe-gateway dialout
adduser -D -u 1000 -s /bin/sh nibe-admin
EOF
# '*' = no password but not locked: OpenSSH without PAM refuses '!'-locked accounts even for key logins.
sed -i -E 's/^(nibe-admin|root):[^:]*:/\1:*:/' "$root/etc/shadow"

step "configuration"
cp -a "$src/rootfs/." "$root/"
install -d -m 0755 "$root/usr/lib/nibe-gateway" "$root/etc/nibe-gateway" "$root/opt/nibe-gateway/factory" \
    "$root/etc/polkit-1/rules.d" "$root/etc/avahi/services" "$root/etc/sudoers.d" "$root/boot/firmware" \
    "$root/var/lib/nibe-gateway" "$root/var/lib/tailscale" "$root/var/lib/NetworkManager" \
    "$root/etc/NetworkManager/system-connections" "$root/usr/share/nibe-gateway" "$root/var/lib/misc"
# packaging/common: the same files and paths the Raspberry Pi OS image had (ADR 0107/0060/0111).
install -m 0644 "$shared/49-nibe-gateway-network.rules" "$root/etc/polkit-1/rules.d/"
install -m 0644 "$shared/hardware.json" "$root/etc/nibe-gateway/hardware.json"
install -m 0644 "$shared/nibe.service" "$root/etc/avahi/services/nibe.service"
# Alpine's avahi package advertises SSH/SFTP; Raspberry Pi OS does not, and sshd only runs when provisioned.
rm -f "$root/etc/avahi/services/ssh.service" "$root/etc/avahi/services/sftp-ssh.service"
# mDNS on wlan0 only (station and setup AP). Otherwise avahi also joins lo and tailscale0, hears its own probes and
# renames itself nibe-gateway-2.local (seen on the Pi, alpine.5). Everything else stays Alpine's default, which
# equals the unmodified Debian file Raspberry Pi OS used (publish-hinfo=no, publish-workstation=no, IPv4 + IPv6).
sed -i 's/^#allow-interfaces=.*/allow-interfaces=wlan0/' "$root/etc/avahi/avahi-daemon.conf"
install -m 0644 "$shared/persistent-state.conf" "$root/usr/lib/nibe-gateway/persistent-state.conf"
install -m 0755 "$shared/nibe-persist" "$root/usr/lib/nibe-gateway/nibe-persist"
install -m 0440 "$shared/sudoers-nibe-admin" "$root/etc/sudoers.d/nibe-admin"
sed -i "s/ieee80211_regdom=SE/ieee80211_regdom=$country/" "$root/etc/modprobe.d/nibe.conf"
if [ "$profile" = simulated ]; then
    sed -i 's#^nibe_gateway_args=.*#nibe_gateway_args="--data-dir /var/lib/nibe-gateway/state --device simulated --network simulated --bind 127.0.0.1:8080 --admin-dir /var/lib/nibe-gateway/current/admin"#' \
        "$root/etc/conf.d/nibe-gateway"
    mv "$root/etc/avahi/services/nibe.service" "$root/etc/avahi/services/nibe.service.disabled"
fi
# swclock floor: the image build time (the default file lives in the read-only root).
touch "$root/var/lib/misc/openrc-shutdowntime"
# OpenRC boot log in RAM (/var/log is tmpfs).
sed -i -E 's/^#?rc_logger=.*/rc_logger="YES"/' "$root/etc/rc.conf"
cp "$out/package-sizes.txt" "$root/usr/share/nibe-gateway/alpine-packages.txt"
printf '%s\n' "${NIBE_IMAGE_VERSION:-unknown}" > "$root/usr/share/nibe-gateway/image-version"

step "NIBE payload"
install -m 0755 "$payload/nibe-updater" "$root/usr/lib/nibe-gateway/nibe-updater"
install -m 0755 "$payload/nibe-system" "$root/usr/lib/nibe-gateway/nibe-system"
install -m 0644 "$payload/release.pub" "$root/etc/nibe-gateway/release.pub"
install -m 0755 "$payload/nibe-gateway" "$root/opt/nibe-gateway/factory/nibe-gateway"
install -m 0644 "$payload/manifest.json" "$root/opt/nibe-gateway/factory/manifest.json"
cp -a "$payload/admin" "$root/opt/nibe-gateway/factory/admin"
if [ -d "$payload/tailscale" ]; then
    # The pinned, stripped static pair (packaging/alpine/tailscale) instead of Alpine's package build (same paths).
    rm -f "$root/usr/bin/tailscale" "$root/usr/sbin/tailscaled"
    install -m 0755 "$payload/tailscale/tailscaled" "$root/usr/sbin/tailscaled"
    install -m 0755 "$payload/tailscale/tailscale" "$root/usr/bin/tailscale"
    (cd "$payload/tailscale" && sha256sum tailscale tailscaled) \
        | sed -E 's#  tailscaled$#  /usr/sbin/tailscaled#; s#  tailscale$#  /usr/bin/tailscale#' \
        > "$root/usr/share/nibe-gateway/tailscale.sha256sums"
fi
[ ! -e "$payload/STUB" ] || cp "$payload/STUB" "$root/usr/share/nibe-gateway/STUB-BINARIES"

step "runlevels"
rm -rf "$root/etc/runlevels"
mkdir -p "$root/etc/runlevels/sysinit" "$root/etc/runlevels/boot" "$root/etc/runlevels/default" \
    "$root/etc/runlevels/shutdown" "$root/etc/runlevels/nonetwork"
python3 - "$src/alpine_image.py" "$root" <<'PY'
import importlib.util, os, sys
spec = importlib.util.spec_from_file_location("alpine_image", sys.argv[1])
image = importlib.util.module_from_spec(spec); spec.loader.exec_module(image)
for level, services in image.RUNLEVELS.items():
    for service in services:
        os.symlink(f"/etc/init.d/{service}", os.path.join(sys.argv[2], "etc/runlevels", level, service))
PY

step "kernel $kver (Raspberry Pi OS package, the kernel the other slot runs)"
kdeb=/work/kernel-deb
rm -rf "$kdeb"; mkdir -p "$kdeb/data"
(cd "$kdeb" && ar x /kernel/kernel.deb && tar -xJf data.tar.xz -C data)
[ -f "$kdeb/data/boot/vmlinuz-$kver" ] || { echo "kernel package lacks vmlinuz-$kver" >&2; exit 1; }
mkdir -p "$root/lib/modules"
cp -a "$kdeb/data/lib/modules/$kver" "$root/lib/modules/"
chroot "$root" /sbin/depmod -a "$kver"

step "boot files (slot A template)"
boot=$out/boot
dtbs=$kdeb/data/usr/lib/linux-image-$kver
cp "$kdeb/data/boot/vmlinuz-$kver" "$boot/kernel.img"
# The complete overlays directory of the kernel package, identical to the Raspberry Pi OS slot: with only the four
# files config.txt names, the firmware did not apply dtoverlay=disable-bt on the Alpine slot (smoke boot alpine.4).
cp -a "$dtbs/overlays/." "$boot/overlays/"
# Belt and braces: disable-bt pre-applied to the dtb, so the PL011 is ttyAMA0 even if the firmware skips overlays.
# Every fragment of disable-bt.dtbo assigns fixed values to existing nodes (status, pinctrl, empty pin lists,
# /aliases serial0/serial1): no new nodes, no __overrides__, no __local_fixups__, so the firmware applying it again is
# harmless. Proven here: applying it to the result changes nothing (decompiled trees must be identical).
fdtoverlay -i "$dtbs/bcm2708-rpi-zero-w.dtb" -o /work/dtb-once "$dtbs/overlays/disable-bt.dtbo"
fdtoverlay -i /work/dtb-once -o /work/dtb-twice "$dtbs/overlays/disable-bt.dtbo"
dtc -q -s -I dtb -O dts /work/dtb-once > /work/dtb-once.dts
dtc -q -s -I dtb -O dts /work/dtb-twice > /work/dtb-twice.dts
cmp -s /work/dtb-once.dts /work/dtb-twice.dts || { echo "disable-bt is not idempotent on this dtb" >&2; exit 1; }
py check-dtb /work/dtb-once
cp /work/dtb-once "$boot/bcm2708-rpi-zero-w.dtb"
if [ "$smoke" = 1 ]; then py cmdline "$country" --smoke; else py cmdline "$country"; fi > "$boot/cmdline.txt"
cp "$kdeb/data/boot/config-$kver" "$out/kernel-config"

step "kernel modules and firmware"
py prune-modules "$root" "$kver"
chroot "$root" /sbin/depmod -a "$kver"
py prune-firmware "$root"

step "initramfs"
ir=/work/initramfs-root
rm -rf "$ir" /work/busybox-static
mkdir -p "$ir/bin" "$ir/lib/modules" /work/busybox-static
apk add -q --root /work/busybox-static --initdb --arch armhf --no-progress --cache-dir /cache \
    --keys-dir "$root/etc/apk/keys" --repositories-file "$root/etc/apk/repositories" busybox-static >/dev/null
cp /work/busybox-static/bin/busybox.static "$ir/bin/busybox"
py initramfs-modules "$root" "$kver" | while read -r module; do
    name=$(basename "$module"); name=${name%.xz}; name=${name%.gz}; name=${name%.zst}
    case $module in
    *.xz) xz -dc "$root/lib/modules/$kver/$module" > "$ir/lib/modules/$name" ;;
    *.gz) gzip -dc "$root/lib/modules/$kver/$module" > "$ir/lib/modules/$name" ;;
    *.zst) zstd -qdc "$root/lib/modules/$kver/$module" > "$ir/lib/modules/$name" ;;
    *) cp "$root/lib/modules/$kver/$module" "$ir/lib/modules/$name" ;;
    esac
    echo "$name" >> "$ir/lib/modules/order"
done
install -m 0755 "$src/initramfs/init" "$ir/init"
(cd "$ir" && find . -mindepth 1 | sort | cpio -o -H newc 2>/dev/null) | zstd -q -19 > "$boot/initramfs"

step "prune the root"
# Slot boot files live on the boot partition; the root keeps only the mount point.
find "$root/boot" -mindepth 1 -maxdepth 1 ! -name firmware -exec rm -rf {} +
# udev-trigger is the only provider of "dev": without these, `need dev` can never pull in mdev next to udev.
rm -f "$root/etc/init.d/mdev" "$root/etc/init.d/hwdrivers"
rm -rf "$root/var/cache/apk"/* "$root/usr/share/man" "$root/usr/share/doc"

step "guard"
py check "$root" --uid "$uid" --gid "$gid" --profile "$profile"
chroot "$root" /usr/sbin/visudo -cf /etc/sudoers.d/nibe-admin >/dev/null
chroot "$root" /usr/bin/ssh-keygen -q -t ed25519 -N '' -f /tmp/hostkey
chroot "$root" /usr/sbin/sshd -t -f /usr/lib/nibe-gateway/sshd_config -h /tmp/hostkey
rm -f "$root/tmp/hostkey" "$root/tmp/hostkey.pub"
for helper in "$root"/usr/lib/nibe-gateway/nibe-boot-guard "$root"/usr/lib/nibe-gateway/nibe-trace \
    "$root"/usr/lib/nibe-gateway/nibe-reboot "$root"/usr/lib/nibe-gateway/nibe-os-confirm; do
    sh -n "$helper"
done
for script in "$root"/etc/init.d/nibe-* "$root/etc/init.d/sshd" "$root/etc/init.d/tailscale"; do
    sh -n "$script"
done
[ ! -e "$root/run/systemd/system" ] || { echo "/run/systemd/system must not exist (binaries would pick systemd)" >&2; exit 1; }

step "squashfs"
# busybox could not mknod in the container and left a regular /dev/null; /dev is devtmpfs at runtime.
find "$root/dev" -mindepth 1 ! -type d -delete
du -sb "$root" | cut -f1 > "$out/root-bytes"
mksquashfs "$root" "$out/rootfs.img" -noappend -comp zstd -Xcompression-level 19 -b 131072 -no-progress -quiet
check=/work/sqfs-check
rm -rf "$check"
unsquashfs -no-progress -q -d "$check" "$out/rootfs.img" >/dev/null
a=$(py summary "$root"); b=$(py summary "$check")
[ "$a" = "$b" ] || { echo "squashfs ($b) differs from the root ($a)" >&2; exit 1; }
py check "$check" --uid "$uid" --gid "$gid" --profile "$profile" >/dev/null
rm -rf "$check"
echo "squashfs: $(stat -c %s "$out/rootfs.img") bytes, $a"
echo "$kver" > "$out/kernel-version"
