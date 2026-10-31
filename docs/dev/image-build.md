# Appliance OS: Alpine bundle and fresh-card image

The appliance runs Alpine Linux (ADR 0117/0118) with the Raspberry Pi OS ARMv6 kernel, a squashfs root under a
96 MiB RAM overlay and A/B OS slots (ADR 0106). One build produces a **signed OS bundle** (what `nibe-updater
os-install` installs into the inactive slot) and, from that bundle, a **fresh-card image** for new SD cards. The
Raspberry Pi OS (pi-gen) build path was retired on 2026-10-25 after both slots of the owner's card ran Alpine.

Everything runs rootless: Docker (rootless is fine) with the host's `qemu-arm` binfmt handler registered with the
`F` flag, plus `llvm-strip` (or `NIBE_STRIP`) for the pinned Tailscale. No loop devices, no sudo.

## 1. Binaries and UI

```bash
./scripts/bootstrap-tools.sh
./scripts/build-frontend.sh
./scripts/build-pi.sh          # static musl ARMv6 (default NIBE_PI_LIBC=musl), serial + network-manager
```

`build-pi.sh` writes `target/pi-musl/arm-unknown-linux-musleabihf/release/{nibe-gateway,nibe-updater,nibe-system}`
and a SHA-bound `nibe-gateway.features.json`; the image build refuses a hardware bundle without matching
`serial,network-manager` evidence. `NIBE_PI_LIBC=gnu` still builds the old glibc variant (not used by the appliance).

## 2. OS bundle (OTA) and smoke bundle

```bash
scripts/build-alpine-os-bundle.sh ~/.config/nibe-gateway/release.key 2026.10.26-alpine.1
# -> target/alpine/nibe-os-alpine-2026.10.26-alpine.1.tar (+ .txt with sizes)
NIBE_SMOKE=1 scripts/build-alpine-os-bundle.sh ~/.config/nibe-gateway/release.key 2026.10.26-alpine.1-smoke
```

- Packages: `packaging/alpine/world` (explicit allowlist, each entry justified). Kernel, modules, dtb and
  overlays: the Raspberry Pi OS kernel pinned in `packaging/alpine/kernel.lock`; Tailscale: the release pinned in
  `packaging/alpine/tailscale/tailscale.lock`, stripped (`NIBE_TAILSCALE=alpine` uses Alpine's package instead).
- Image files: `packaging/alpine/rootfs/` (OpenRC services, configs) and `packaging/common/` (polkit rule,
  hardware.json, Avahi service, persistent-state.conf, nibe-persist, sudoers).
- Guards (`packaging/alpine/alpine_image.py check`): required paths, runlevels, accounts, fstab, inittab, avahi
  config, polkit D-Bus activation, no pidfiles in one-shots, kernel modules, uncompressed firmware, the dtb with
  disable-bt applied; then an `unsquashfs` round trip.
- Options: `NIBE_IMAGE_PROFILE=simulated` (bench/no-UART profile), `NIBE_WIFI_COUNTRY` (default SE),
  `NIBE_GATEWAY_UID/GID` (995/992), `NIBE_RELEASE_PUBLIC_KEY` (default `release.pub` next to the private key).
- A smoke bundle (`nibe.smoke=1`) never commits: it boots once, records traces and reboots back by itself.

Install over SSH (the Pi's updater verifies signature and checksums, writes the inactive slot, tryboots it;
`nibe-os-confirm` commits after the probation):

```bash
NIBE_OS_ZSTD_LEVEL=1 NIBE_SSH_HOST=nibe-admin@nibe-gateway.local \
  bash -c 'source scripts/ssh-maintenance.sh && nibe_os_install target/alpine/nibe-os-alpine-VERSION.tar'
NIBE_SSH_HOST=nibe-admin@nibe-gateway.local scripts/alpine-trial-report.sh [--fetch ~/nibe-trials]
```

## 3. Fresh-card image

Every GitHub release carries the fresh-card image of its version (`nibe-gateway-alpine-<version>.img.xz` and
`.sha256`), built and inspected by `scripts/publish-github-release.sh` from an OS bundle of the same version, so new
users can flash it without building anything. The steps below are what that script runs.

```bash
scripts/build-alpine-image.sh target/alpine/nibe-os-alpine-VERSION.tar
# -> target/alpine/nibe-gateway-alpine-VERSION.img.xz + .img.xz.sha256
scripts/inspect-alpine-image.sh target/alpine/nibe-gateway-alpine-VERSION.img.xz     # mount-free checks
scripts/qemu-boot-alpine-image.sh target/alpine/nibe-gateway-alpine-VERSION.img.xz   # boots the image itself
```

The image is built from the bundle (checksums checked against its signed manifest; smoke bundles are refused), so
a fresh card runs exactly what an OTA update installs. Since ADR 0129 the image holds only p1 and slot A cut to its
squashfs; the **first boot** completes the layout of the owner's card (512-byte sectors):

| Part | Start | Size | In the image | After the first boot |
|---|---|---|---|---|
| p1 FAT32 `bootfs` | 8 MiB | 512 MiB | firmware (`raspi-firmware` pinned in `packaging/alpine/firmware.lock`, start.elf identical to the card's), `config.txt` (`packaging/alpine/boot/config.txt`, managed `os_prefix=slot-a/`), `slot-a/` (bundle boot files, cmdline with this image's PARTUUID), empty `slot-b/`, `nibe-provision.json.template`, `README.txt`, the marker `nibe-first-boot` | same, marker deleted, `nibe-trial/a-1.log` (first-boot trace) |
| p2 slot A | 520 MiB | the squashfs rounded up to 1 MiB (~43 MiB) | the bundle's squashfs | grown to 1976 MiB |
| p3 ext4 `nibe-data` | 2496 MiB | 512 MiB | absent | created and seeded: `releases/slot-a` = factory release, `current` → it, idle `update-state/state.json`, `state/` (995:992, 0700), `system/{ssh,connections,NetworkManager}` |
| p4 slot B | 3008 MiB | 1976 MiB | absent | first MiB wiped, otherwise untouched until the first OS update |

The raw image ends right after the squashfs: ~563 MiB raw, ~60 MB as `.img.xz` (the full layout was 4.9 GiB raw,
about 25 minutes to write at the owner's card's 3–5 MB/s; now about 2–3 minutes). The card needs at least 4.9 GiB
(an 8 GB card); capacity beyond the layout stays unused.

What the first boot does (`packaging/alpine/initramfs/init`, ADR 0129), before anything on the card is mounted
read-write: it checks that the marker is on p1, MBR entries 3 and 4 are empty and entry 2 is shorter than the
layout, wipes the first MiB of p3 and p4 (a reused card may hold an older OS there), writes MBR entries 2–4 (disk
id kept) and re-reads the table (if the kernel refuses, it reboots and resumes). After the squashfs root is mounted
it seeds p3 in a tmpfs from the root, runs the root's `mke2fs -t ext4 -L nibe-data -O ^64bit,^huge_file -d`
through `chroot`, and deletes the marker last. An interrupted first boot redoes it on the next boot (full table +
marker + a p3 without boot logs). Without the marker a missing p3 stops the boot ("not a fresh card; rewrite the
card"); an installed card (full table, no marker) is never touched. Recovery from any first-boot failure: write
the image again.

The MBR disk id is random per image (`NIBE_DISK_ID` fixes it for tests); slot A's cmdline says
`root=PARTUUID=<disk id>-02 nibe.slot=a`, exactly as `nibe-updater os-install` would write it.
`scripts/inspect-alpine-image.sh` checks the image (`card_image.py inspect`); `scripts/qemu-boot-alpine-image.sh`
boots it twice on an 8 GiB virtual card with an old filesystem where p3/p4 land, checks the completed card
(`card_image.py card`: full layout, p4 wiped, no marker, p3 seed) and that the second boot leaves table and p3 alone.

### Flash

```bash
cd target/alpine && sha256sum -c nibe-gateway-alpine-VERSION.img.xz.sha256
xzcat nibe-gateway-alpine-VERSION.img.xz | sudo dd of=/dev/sdX bs=4M conv=fsync status=progress
```

(or Raspberry Pi Imager → "Use custom" with the `.img.xz`, without its OS customisation). Re-plug the card: the
`bootfs` partition appears on the laptop (only `bootfs`: p3 and p4 appear after the first boot). Do not create or
copy `nibe-first-boot` by hand.

### Provision (optional, ADR 0108)

Copy `nibe-provision.json.template` to `nibe-provision.json` on `bootfs`, delete the keys you do not need and fill
in the rest (`wifi` ssid/password, `admin_password` ≥ 12 characters, `ssh_authorized_keys`, `tailscale_auth_key`).
On first boot `nibe-provision` validates it (an invalid file is left untouched and nothing is applied), installs the
SSH keys and Tailscale key, hands Wi-Fi and the admin password to the gateway, and deletes the file. Without the
file the gateway starts its setup access point: an **open** Wi-Fi network named `NIBE-Gateway-XXXX` (XXXX: the last four
hex digits of the Pi's serial number, upper case; `nibe-gateway-setup` is only NetworkManager's connection id). Join it, open
http://192.168.4.1:8080/ and create the administrator straight away: over the AP no setup token is needed while no
admin exists, so anyone in range could claim the gateway first (ADR 0130).

### Verify on the Pi

`ssh nibe-admin@nibe-gateway.local` (provisioned key): `cat /proc/cmdline` (`nibe.slot=a`, the image's PARTUUID),
`rc-status -a`, `curl -s localhost:8080/health`, `ls /boot/firmware` (no `nibe-provision.json` any more),
`sudo nibe-updater os-status`. The first OS update then installs into slot B.

## 4. Operating notes

- Logs: busybox syslogd → `/var/log/messages` (RAM, 4 MiB + 1 rotation); OpenRC boot log `/var/log/rc.log`; boot
  traces on p3 `system/boot-logs/` (last 5 boots) and, for trial/smoke boots, initramfs traces on
  `bootfs/nibe-trial/`. `scripts/pi-diagnostics.sh logs|debug|traces` reads them over SSH.
- Services: `rc-status -a`, `rc-service nibe-gateway restart`. sshd and tailscale start only when provisioned.
- Simulated bench profile: build the bundle with `NIBE_IMAGE_PROFILE=simulated` (the root is read-only, so a
  per-boot change would not persist).
- Root edits land in the RAM overlay and disappear on reboot; persistent state is declared in
  `persistent-state.conf` (ADR 0107).
