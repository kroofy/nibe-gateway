# Alpine appliance image — contents, sizes and differences from Raspberry Pi OS (2026-10-25)

Decision: [ADR 0117](../decisions/0117-alpine-base.md) (base), [ADR 0118](../decisions/0118-alpine-boot-and-openrc-services.md)
(boot and services). Build: `scripts/build-alpine-os-bundle.sh`. Hardware steps: HP-38 in
[HARDWARE_PENDING.md](../progress/HARDWARE_PENDING.md).

Labels: **VERIFIED** = measured on the build host from the built bundle `nibe-os-alpine-2026.10.25-alpine.1.tar`
(Alpine 3.24.3 packages of 2026-10-25, kernel 6.18.58-0-rpi), or observed booting that bundle's kernel, initramfs and
squashfs under QEMU `raspi0` (Pi Zero model) in the builder container. **HYPOTHESIS** = needs the Pi (HP-38).

## 0. Update after the first hardware trial (2026-10-25 evening)

Bundle `2026.10.25-alpine.2` (Alpine linux-rpi 6.18) hung on the Pi Zero W before userspace: no answer, nothing on
p3, no return by itself; power-cycled after ~11 min → Raspberry Pi OS (os-state `failed`). Without traces the stage
is unknown; the firmware → kernel step is the prime suspect because QEMU skips it. Changes (ADR 0118 amendment):
- **Kernel:** the Raspberry Pi OS `6.12.109+rpt-rpi-v6` that slot A runs (package
  `linux-image-6.12.109+rpt-rpi-v6_6.12.109-1+rpt1_armhf.deb`, SHA-256 `f4f39b33…f705d090`, pinned in
  `packaging/alpine/kernel.lock`). `linux-rpi` left `world`, and with it mkinitfs/cryptsetup/device-mapper (124 →
  116 packages). This kernel has no `CONFIG_FW_LOADER_COMPRESS`: the Alpine `.zst` Wi-Fi firmware would not have
  loaded, so the build now decompresses the kept chain (guarded).
- **Traces:** initramfs stages on `p1:/nibe-trial/`, userspace snapshots on `p3:system/boot-logs/`,
  `scripts/alpine-trial-report.sh` to read both; `quiet` removed.
- **Smoke bundle** (`NIBE_SMOKE=1`): never commits, reboots back by itself; tested before any real trial.

Sizes `2026.10.25-alpine.3` (VERIFIED): root content 98.4 MiB, squashfs 38.1 MiB, slot boot files 7.9 MiB
(kernel.img 7.1 MiB, initramfs 0.7 MiB), bundle 46.0 MiB; 272 of 1802 kernel modules kept (2.9 MiB); smoke bundle
the same size. With `NIBE_TAILSCALE_DIR` add ~4.4 MiB. QEMU (raspi0, this kernel) boots to the default runlevel in
~45 s of emulated time (6.18: ~150 s); smoke and trial-failure paths verified there (§4, HP-38).
Sections 1–6 below describe the first build (kernel 6.18) where they differ; the package list in §2 lost linux-rpi
(33 MiB installed) and mkinitfs, lddtree, cryptsetup-libs, device-mapper(-libs/-udev), json-c, scanelf, mdev-conf.

### Smoke boot alpine.4 (Pi, 20:48) and alpine.5-smoke

alpine.4-smoke on the Pi: initramfs hand-over at 4.6 s, default runlevel reached, smoke snapshot and self-reboot at
150 s uptime (Pi unreachable ~3–4 min in total). brcmfmac loaded the decompressed BCM43430/1 firmware
(7.45.98). Faults: disable-bt not applied by the firmware (PL011 = ttyAMA1, Bluetooth serdev on it), so the gateway
could not open ttyAMA0 and never activated Wi-Fi (NetworkManager never autoconnects the gateway's user-permission
profile — ADR 0118 amendment 2, point 13). Fixes in alpine.5-smoke: full overlays directory, disable-bt pre-applied
to the dtb (idempotence proven at build time), UART/NM evidence and event uptimes in the traces, report timing.
Sizes alpine.5-smoke (with the pinned Tailscale 1.104.2, VERIFIED): root content 109.7 MiB, squashfs 42.6 MiB, slot
boot files 9.0 MiB (overlays 2.0 MiB), bundle 51.8 MiB.

## 1. How it is built

`scripts/build-alpine-os-bundle.sh PRIVATE_KEY VERSION [OUTDIR]` (host, no root):
1. Checks the binaries (default `target/pi-musl/arm-unknown-linux-musleabihf/release/`) are static 32-bit ARM ELF
   (no `PT_INTERP`: Alpine has no glibc) and the gateway's `features.json` says serial + network-manager; assembles
   the payload (binaries, admin UI, release.pub, factory manifest exactly as `build-image.sh` writes it).
2. Runs `packaging/alpine/build-root.sh` in a native `alpine:3.24` (amd64) container: `apk --arch armhf --root`
   installs `packaging/alpine/world`; package scripts and chroot steps (adduser, depmod, visudo, sshd -t) run
   through the host's qemu-arm binfmt. Then: accounts, `packaging/alpine/rootfs/` (OpenRC scripts and configs),
   the shared Raspberry Pi OS files (polkit rule, hardware.json, Avahi service, persistent-state.conf, nibe-persist,
   sudoers), runlevels, slot boot files, the initramfs, module/firmware pruning, the guard
   (`alpine_image.py check`, 90 paths, 34 runlevel services, accounts, fstab, inittab), `mksquashfs` (zstd 19,
   128 KiB blocks), an `unsquashfs` round trip (same file/byte/link counts) and the guard again on the unpacked image.
3. `nibe-release os-sign` (same tool, key and format as `build-os-bundle.sh`) → `nibe-os-alpine-VERSION.tar`.
A warm build takes ~15 s (apk cache in `~/.cache/nibe-gateway/alpine-apk-armhf`).

## 2. Sizes (VERIFIED)

| | Alpine bundle 2026.10.25-alpine.1 | Raspberry Pi OS bundle 2026.10.25-5 (ext4) / squashfs estimate |
|---|---|---|
| Installed packages before pruning | 124 packages, 128.8 MiB | 68 purged from ~400 |
| Root content (after pruning) | **99.3 MiB** (1232 files, 726 links) | 340 MiB / ~280 MiB |
| Root image (`rootfs.img`, squashfs zstd-19) | **38.6 MiB** | 373 MiB ext4 / ~95–100 MiB squashfs (SIM) |
| Slot boot files | 8.1 MiB (kernel 7.3 MiB, initramfs 0.7 MiB, dtb, 4 overlays) | ~18 MiB (all dtbs, 375 overlays, 10 MiB initramfs) |
| Bundle tar | **46.7 MiB** | 378 MB |

Biggest contributors of the 99.3 MiB root: tailscale 34.0 MiB (one combined `tailscaled` binary, stripped by Alpine,
`tailscale` is a symlink), the gateway factory copy 10.6 MiB, glib/NSS/libcrypto/wpa_supplicant/NetworkManager
(~19 MiB together), kernel modules 2.9 MiB (258 of 1799 kept), firmware 0.6 MiB (11 of 224 files kept).
Measured RAM under QEMU with the gateway not running: 17 MiB used (NetworkManager 12 MiB RSS) — the Pi will differ.

Installed sizes per package (apk, before pruning; kernel and firmware shrink to 2.9 + 0.6 MiB):
tailscale 34.0M, linux-rpi 33.0M, linux-firmware-brcm 7.8M, linux-firmware-cypress 4.9M, glib 4.7M,
wpa_supplicant 3.9M, libcrypto3 3.5M, networkmanager 3.5M, nss 3.3M, libstdc++ 2.4M (zstd CLI), openssh-server 1.8M,
libunistring 1.8M, sudo 1.8M, sqlite-libs 1.4M, libnm 1.1M, busybox 1.0M, brotli-libs 0.8M, openrc 0.8M, libssl3 0.8M,
libnl3 0.8M, libcurl 0.7M, pcre2 0.7M, networkmanager-cli 0.7M, zstd-libs 0.7M, musl 0.6M, libevent 0.6M, eudev 0.6M,
libelogind 0.6M, linux-pam 0.6M, cryptsetup-libs 0.5M, e2fsprogs(-libs) 0.8M, avahi 0.4M, dnsmasq 0.4M,
openssh-keygen 0.3M, device-mapper-libs 0.3M, libncursesw 0.3M, chrony 0.3M, libmount 0.3M, dbus 0.3M, curl 0.3M,
and 85 packages under 0.3 MiB each. The full list is in the image at `/usr/share/nibe-gateway/alpine-packages.txt`.
Dead weight kept because a listed package depends on it: mkinitfs + cryptsetup-libs + device-mapper + lddtree
(~1 MiB; linux-rpi depends on an initramfs generator), linux-pam (sudo/polkit), ifupdown-ng (alpine-base).

## 3. What is in the image and why

The allowlist is `packaging/alpine/world`, every entry with its reason. Beyond the owner's starting list:
- **dnsmasq** — the setup AP profile uses `ipv4.method=shared` (crates/network/src/network_manager.rs), for which
  NetworkManager spawns dnsmasq (Raspberry Pi OS: dnsmasq-base). NetworkManager gets `firewall-backend=none`: no
  NAT is needed and neither nft nor iptables is installed (`NetworkManager --print-config` accepts it).
- **eudev** — NetworkManager manages devices through libudev and the udev database, and eudev's
  `60-persistent-storage.rules` create `/dev/disk/by-partuuid/*`, which `nibe-updater os-install` uses (VERIFIED in
  QEMU: `15be6592-01…04`). mdev does neither; its init scripts are removed so `need dev` can only mean udev.
- **kmod** — modprobe for xz-compressed modules. **wireless-regdb** — regulatory.db(.p7s) for cfg80211 (a module,
  signed regdb required). **networkmanager-cli** — `nmcli` for diagnosis/recovery over SSH.
- **Not installed:** raspberrypi-bootloader (p1 firmware is shared and never updated, ADR 0106), iptables/nftables,
  bash, Python, util-linux mount (busybox suffices), openssh-sftp-server (`internal-sftp`), a getty on the UART.

Runtime layout (VERIFIED in QEMU unless noted): `/` overlay (squashfs lower, 96 MiB tmpfs upper); `/boot/firmware`
vfat `LABEL=bootfs` read-only; `/var/lib/nibe-gateway` ext4 `LABEL=nibe-data`; tmpfs `/var/log` 16M, `/tmp` 32M,
`/var/tmp` 16M; nibe-persist bind mounts `/etc/ssh`, `/var/lib/NetworkManager`, `/etc/NetworkManager/system-connections`,
`/var/lib/tailscale` from p3. Accounts: `nibe-gateway` 995:992 + dialout (owner of p3 `state/` on Raspberry Pi
OS), `nibe-admin` 1000 with `*` password (key-only SSH, sudo NOPASSWD).

OpenRC runlevels (port of the systemd units, ordering by `depend()`):
- sysinit: devfs dmesg udev udev-trigger udev-settle
- boot: modules sysctl hostname bootmisc syslog klogd swclock localmount(+fsck) loopback seedrng watchdog
  nibe-persist nibe-boot-guard
- default: dbus polkit networkmanager avahi-daemon chronyd nibe-updater nibe-system nibe-provision nibe-gateway
  sshd tailscale nibe-tailscale-up nibe-os-confirm
- Observed start order (QEMU): … localmount → nibe-persist → dbus → networkmanager → avahi → chronyd →
  nibe-updater → nibe-system → nibe-gateway → sshd/tailscale (gated) → nibe-os-confirm → polkit (D-Bus activated
  earlier if NetworkManager needs it).

## 4. Boot

Slot files: `kernel.img` (`vmlinuz-rpi`, ARMv6 zImage), `initramfs` (ours, zstd), `bcm2708-rpi-zero-w.dtb`,
`overlays/{disable-bt.dtbo, vc4-kms-v3d.dtbo, overlay_map.dtb, hat_map.dtb}`, `cmdline.txt`:
`console=tty1 root=PARTUUID=00000000-02 rootwait quiet cfg80211.ieee80211_regdom=SE nibe.overlay=1 rootfstype=squashfs panic=10 nibe.slot=a`
(os-install substitutes the disk id, partition and slot). The shared `config.txt` (stock bookworm +
`enable_uart=1`, `dtoverlay=disable-bt`, `auto_initramfs=1`, managed `os_prefix`) needs **no change**: it loads
`kernel.img` and `initramfs` from the prefix, and the two overlays it names are in the slot.

Alpine's mkinitfs init was checked first: it supports `rootfstype=squashfs` and `overlaytmpfs`/`overlaytmpfsflags`,
but mounts `root=` with busybox mount, which cannot resolve `PARTUUID=` (ADR 0118). Our init resolves the PARTUUID
from the MBR signature, insmods squashfs + overlay, mounts the overlay, moves /dev /proc /sys, attaches
`/dev/console` and `switch_root`s to busybox init. Found and fixed by booting it under QEMU: busybox installs
`od`/`tr` into `/usr/bin` (not on the first PATH) and the cpio has no `/dev/console` (no mknod in rootless Docker).

### QEMU boot check (VERIFIED, repeatable)

`scripts/qemu-boot-alpine.sh [--trial]` boots the last build under QEMU `raspi0` (in the builder container, no root)
from a 256 MiB SD image with the real layout and labels (`packaging/alpine/qemu/`), with a probe added to a *copy* of
the squashfs. The dtb copy has the BCM2835 PM/watchdog node disabled (QEMU lacks it; the kernel oopses otherwise), and
QEMU applies no overlays, so the PL011 is `ttyAMA1` there. Results on 2026-10-25: boot to the default runlevel in
~3 min of emulated time; root via PARTUUID, overlay `/`, `/boot/firmware` ro, p3 + four bind mounts, by-partuuid
links, nibe-gateway 995:992 + dialout, nibe-updater/-system at nice 10, modules ipv6/af_packet/tun/overlay/squashfs,
17–18 MiB RAM used. Simulated profile: the static musl gateway answers `/health` `ready:true` under
`nibe-system supervise`. `--trial`: os-confirm in probation, boot guard armed. Hardware profile: the gateway fails
(no ttyAMA0/wlan0 in QEMU) and supervise-daemon gives up after 5 respawns in 120 s, as systemd's StartLimitBurst.

## 5. Differences from the Raspberry Pi OS image

| Area | Raspberry Pi OS | Alpine |
|---|---|---|
| libc / binaries | glibc; gnueabihf builds | musl; **static** musleabihf builds only (an app release in `current/` must be static too) |
| init | systemd units, journald (volatile 16M) | busybox init + OpenRC, supervise-daemon; busybox syslogd → `/var/log/messages` (4 MiB + 1) |
| logs | `journalctl -u nibe-gateway` | `grep nibe-gateway /var/log/messages`, `logread` is not used; OpenRC boot log `/var/log/rc.log` |
| watchdog | systemd WatchdogSec via sd_notify | `nibe-system supervise` own watchdog (NIBE_WATCHDOG_SEC 30, START_TIMEOUT 60) + respawn 5/120 s |
| hardware watchdog | (systemd) | busybox `watchdog -T 15 -t 5 /dev/watchdog` from the boot runlevel |
| emergency | emergency.target → reboot after 180 s | nibe-boot-guard: p3 missing → reboot after 180 s; uncommitted trial → reboot after 30 min |
| time | timesyncd + fake-hwclock (both in RAM upper) | chrony + swclock (build time floor) + forward-only floor on p3 (`system/clock`) |
| sshd config | `/etc/ssh/sshd_config.d/nibe.conf` (persisted with /etc/ssh) | image file `/usr/lib/nibe-gateway/sshd_config`; host keys/authorized_keys still from p3 |
| ssh/tailscale gating | `ConditionPathExists=` drop-ins | marker check in `start_pre` (services listed in default runlevel; `rc-update` is per boot) |
| Tailscale | pinned 1.104.2 static, stripped (46 MiB) | Alpine `tailscale` 1.98.10 (34 MiB), nftables mode — **older**, see risks |
| sandboxing | ProtectSystem, empty capability sets, DeviceAllow… | not available; see ADR 0118 for what is lost |
| `.local` names | resolvable via libnss-mdns | not resolvable from our musl binaries (no NSS); Avahi still publishes nibe-gateway.local |
| UART | `/dev/ttyAMA0` (+ `/dev/serial0` link from raspberrypi-sys-mods) | `/dev/ttyAMA0` expected with disable-bt (no `/dev/serial0` link); under QEMU without overlays the PL011 was ttyAMA1 |
| kernel | 6.12.109+rpt-rpi-v6 | the same package since alpine.3 (alpine.1/.2: Alpine 6.18 rpi); Wi-Fi firmware stored uncompressed |
| boot traces | journald in RAM only | p1 `nibe-trial/` (trial/smoke boots), p3 `system/boot-logs/` (last 5 boots) |

## 6. Open risks (HYPOTHESIS until HP-38)

1. **App release must be static musl.** `/var/lib/nibe-gateway/current/nibe-gateway` comes from p3, not from the
   bundle. A glibc build there cannot exec on Alpine (no `ld-linux-armhf.so.3`) → gateway down → os-confirm fails →
   rollback. Deploy a musl app release on Raspberry Pi OS first (HP-38 step 0). Also `PI_TARGET` in
   `crates/updater/src/artifact.rs` is still `arm-unknown-linux-gnueabihf` (manifest label only).
2. **Tailscale 1.104.2 → 1.98.10 downgrade** of the persisted state on p3. Mitigation built in: build with
   `NIBE_TAILSCALE_DIR=<dir with the stripped pinned tailscale + tailscaled>` to ship the same 1.104.2 pair as
   Raspberry Pi OS. Measured (VERIFIED): root 110.6 MiB, squashfs 43.0 MiB, bundle 51.1 MiB (+4.4 MiB on the wire). os-confirm requires tailscaled when enabled, so a
   broken Tailscale fails the trial instead of committing.
3. **Kernel/firmware stage**: alpine.2 with Alpine's 6.18 hung here. alpine.3 uses the exact kernel, dtb and
   overlays slot A boots with the same firmware, so this stage should now behave like Raspberry Pi OS — the smoke
   boot checks it before a real trial.
4. **PL011 name**: Alpine 6.18 numbers PL011 ports from DT aliases; with `disable-bt` serial0 = uart0 → ttyAMA0
   expected, observed ttyAMA1 only without overlays in QEMU.
5. **NetworkManager 1.52 shared mode with `firewall-backend=none`** (setup AP) and brcmfmac AP mode — Wi-Fi is not
   emulated by QEMU.
6. **Persisted state written by newer daemons** (NM 1.42 → 1.52, Tailscale) must still be readable by the
   Raspberry Pi OS slot after a rollback.
7. `sshd` reads the Debian-generated host keys from p3 — same fingerprints expected; the Debian `sshd_config` in
   `/etc/ssh` is ignored on Alpine.

## 7. Firmware watchdog for a tryboot trial (item 6 of the owner's plan) — evidence and proposal

Question: can the Pi Zero's `start.elf` start the hardware watchdog before the kernel, only for the trial boot?

Evidence (VERIFIED by reading the firmware actually on p1 — `start.elf` md5 `ae2745e49b925f11a353b78596f13a13`,
identical in the 2026-10-24 and -25 images — and the kernel package):
- `start.elf` contains the config keys `kernel_watchdog_timeout` and `kernel_watchdog_partition`, the messages
  "Enabling early watchdog", "Boot watchdog running", "Set watchdog partition %u timeout %us", "boot watchdog stop:
  remaining %u", the cmdline fragment ` watchdog.open_timeout=%u`, and the strings `tryboot`, `tryboot.txt` and
  `[tryboot]` (a conditional section).
- Kernel 6.12.109+rpt-rpi-v6: `CONFIG_BCM2835_WDT=y`, `CONFIG_WATCHDOG_CORE=y`,
  `CONFIG_WATCHDOG_HANDLE_BOOT_ENABLED=y` (the core keeps petting a watchdog the boot loader left running until
  userspace opens `/dev/watchdog`), `CONFIG_WATCHDOG_OPEN_TIMEOUT=0` (overridden by `watchdog.open_timeout=` on the
  cmdline), no NOWAYOUT.
Reading (HYPOTHESIS, consistent with the strings, not tested on BCM2835): with `kernel_watchdog_timeout=N` the
firmware arms the PM watchdog before loading the kernel and passes `watchdog.open_timeout=N`; the kernel pets it
until our busybox `watchdog` (sysinit runlevel) opens `/dev/watchdog`, or resets the board after N s if nothing
does. A hang in the firmware → kernel stage (what alpine.2 did) then resets instead of waiting for a power cycle,
and the reset boots `config.txt` = the committed slot. Unknown: whether the firmware build for BCM2835 actually
honours it (the strings are in the shared VideoCore source), whether it applies before or after loading the kernel
image, and how `kernel_watchdog_partition` interacts with `os_prefix` (leave it unset).

Proposal (needs a Rust change in `crates/updater/src/os.rs`, not made here):
1. The slot template opts in with a cmdline token, e.g. `nibe.trial_watchdog=120` (Alpine bundles only: their
   userspace opens `/dev/watchdog` within ~30 s; Raspberry Pi OS does not open it, so a Pi OS trial with this option
   would always reset — never add it there).
2. `os-install` writes `tryboot.txt` = `select_slot(config, to)` plus, only when the new slot's template has the
   token (10…600), a final line `kernel_watchdog_timeout=<N>` after the managed `os_prefix`. `config.txt` is never
   changed, so committed boots are unaffected; os-confirm already removes `tryboot.txt`.
   (Alternative without per-trial files: a `[tryboot]` section in config.txt — rejected, it changes the shared
   config.txt and would also apply to Raspberry Pi OS trials.)
3. Verify first on the bench Pi (HP-38 step 0.6): hand-edit `tryboot.txt` after `os-install --no-reboot` of the
   smoke bundle, tryboot, check "Enabling early watchdog" on HDMI and `watchdog.open_timeout=` in the Alpine
   `/proc/cmdline`; then a negative test with a slot whose `init=` sleeps forever (bench only) must reset within N s.

