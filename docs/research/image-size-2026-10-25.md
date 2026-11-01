# Appliance OS image and OS bundle size — what is left, and how to shrink it further (2026-10-25)

Scope: analysis and proposal. Steps 1–4 of §5
were accepted and implemented the same day (see §7); squashfs (step 5) is still proposed. Hardware steps: HP-35 in
[HARDWARE_PENDING.md](../progress/HARDWARE_PENDING.md). Baseline:.

Labels: **VERIFIED** = measured on the built bundle `~/nibe-img/os-bundles/nibe-os-2026.10.25-3.tar` or on the running
Pi (read-only) today. **SIM** = measured by simulation on the build host (for example block-wise compression of the
real files as a stand-in for `mksquashfs`, which is not installed). **ESTIMATE** = reasoned, not measured.

## 1. Method

- `tar -xf nibe-os-2026.10.25-3.tar rootfs.img boot.tar`, then `debugfs -R "rdump / root" rootfs.img` (no root, no
  mount) into `/var/tmp`, and `du`, `dumpe2fs`, per-package sizes from `var/lib/dpkg/info/*.list` (paths normalised
  for merged `/usr`), reverse dependencies from `var/lib/dpkg/status`, `zstd -3/-9/-19` on images and file groups.
- Squashfs/EROFS stand-in: every file cut into fixed blocks (16 KiB / 128 KiB / 1 MiB), tails packed into fragment
  blocks like squashfs does, each block compressed independently with LZMA2 (xz) and zstd. Real `mksquashfs` adds
  ~1–3 % metadata.
- Pi (read-only): `df -h`, `lsblk`, `/proc/meminfo`, `systemctl list-units --type=service`, `dpkg-query` sizes. The
  Pi's root is an overlay whose lower layer is not visible after boot, so on-device `du` shows only the 8.8 MiB
  tmpfs upper; the bundle dump is the authoritative root measurement (same build, slot B is running it).
- The scratch copies were deleted afterwards. To repeat: the commands above, against any bundle.

## 2. What the bundle and root consist of today

### 2.1 Bundle and image (VERIFIED)

| Item | Size |
|---|---|
| Bundle tar (uncompressed, A/B OS bundle format) | 814.0 MB (776.3 MiB) |
| `rootfs.img` (ext4, shrunk by `resize2fs -M` + 32 MiB) | 757.0 MiB = 193 759 × 4 KiB blocks |
| of which used blocks | 588.9 MiB |
| of which free blocks | 168.0 MiB — **all zeros** (2 of 43 008 free blocks non-zero) |
| file content (apparent size, 17 440 regular files) | 515 MiB |
| ext4 overhead (16 MiB journal, 11 MiB inode tables for 45 216 inodes of which 22 311 used, 4 KiB tail rounding, dirs) | ~74 MiB |
| `boot.tar` (kernel.img 7.1, initramfs 9.5, 30 dtbs 1.3, overlays 1.1) | 19.4 MiB |
| Bundle through `zstd -3` (default level; matches the "224 MiB over the wire" in HP log) | 224.8 MiB |
| `rootfs.img` through `zstd -3 / -9 / -19` | 207.8 / 189.3 / 168.7 MiB |
| `boot.tar` through `zstd -19` | 16.8 MiB (kernel and initramfs are already compressed) |

Observations:
- The bundle format itself is **not compressed**; compression only happens in the operator's SSH pipe
  (`zstd -dc | sudo nibe-updater os-install /dev/stdin`). `os-install` (crates/updater/src/os.rs) writes and SHA-256-hashes
  **every byte of the 757 MiB image**, including the 168 MiB of zeros, so the Pi decompresses, hashes and writes
  757 MiB to the SD card per OS update although only 589 MiB is used and 515 MiB is content.
- `resize2fs -M` is conservative: it left 168 MiB free, not the intended ~32 MiB.
- Install timing (VERIFIED from `~/nibe-img/os-install*.log`): 2026.10.25-1 (2.15 GB raw) 15 min 28 s = 2.3 MB/s raw;
  -2 / -3 (814 MB raw, 225 MiB on the wire) 4 min 17 s / 4 min 21 s = 3.2 MB/s raw, 0.87 MiB/s compressed.
  HYPOTHESIS: the limit is the single ARM11 core doing sshd decryption + `zstd -d` + Rust SHA-256 + block writes, not
  Wi-Fi, because raw throughput stayed in the same 2.3–3.2 MB/s band while the compressed rate varied. HP-35 step 1
  measures each stage separately to settle this.

### 2.2 Root content by area (VERIFIED, raw = apparent bytes; z = bytes through `zstd -3`)

| Area | Raw MiB | z MiB | Needed on a Pi Zero W appliance? |
|---|---:|---:|---|
| **Tailscale** `tailscaled` 35.8 + `tailscale` 30.5 (pinned 1.104.2 static tgz) | 66.3 | 34.0 | yes — but both binaries ship **unstripped with DWARF** (`file`: "with debug_info, not stripped") |
| **Python 3.11** (stdlib, interpreter, 15 MiB `__pycache__`, pip/setuptools wheels, python3-* libs) | 40.1 | 13.8 | only for `supervise.py` (89 lines) and `nibe-provision.py` (96 lines) |
| **ICU + libxml2 + shared-mime-info** (+ `/usr/share/mime`) | 40.6 | 13.9 | no — only chain: libglib2.0 *Recommends* shared-mime-info → libxml2 → libicu72 |
| **Perl** (`perl`, `libperl5.36`, `perl-modules-5.36`; `perl-base` 9.7 stays, it is Essential) | 38.3 | 8.7 | no — nothing installed Depends on `perl` |
| Kernel package `linux-image-…-rpi-v6` (modules 20.7 xz-compressed, 1 802 `.ko.xz`; `/boot/vmlinuz` 7.1) | 27.7 | ~25 | modules: a few dozen; `/boot/vmlinuz` is a duplicate of `slot-x/kernel.img` |
| `/boot/initrd.img-…` (unowned) | 9.5 | 9.5 | no — duplicate of `slot-x/initramfs` in boot.tar |
| `/usr/lib/raspi-firmware` (start*.elf/fixup* for all boards) | 21.6 | 12.5 | no — the boot partition's shared firmware is never touched by OS updates |
| `/usr/lib/firmware` (firmware-brcm80211 etc., all chipsets) | 19.8 | 10.0 | only `brcmfmac43430-sdio.*` → `cypress/cyfmac43430-sdio.*` + regulatory.db, ~0.5 MiB |
| `locales` (`/usr/share/i18n` sources) + generated `locale-archive` | 18.3 | 5.6 | no — `C.UTF-8` is built into libc-bin |
| udev `hwdb.bin` 12.4 + `hwdb.d` sources 8.6 | 21.0 | 3.5 | sources no (only for `systemd-hwdb update`); hwdb.bin cosmetic (vendor names) |
| ModemManager, libqmi, libmbim, libqrtr (NM *Recommends*; **running** on the Pi) | 10.7 | 2.5 | no — no modem |
| file + libmagic (`magic.mgc` 7.9) | 8.2 | 0.4 | no |
| console-setup, keyboard-configuration, xkb-data, kbd (headless; services run at boot) | 7.2 | 2.5 | no |
| gnupg stack except gpgv | 5.3 | 1.7 | no |
| X11 libs via xauth | 2.9 | 0.8 | no |
| misc (groff-base, ncurses-term, strace, bash-completion, pci.ids, pciutils, usbutils, htop, wireless-tools, net-tools, isc-dhcp-client, ppp, parted, wget, python wheels/venv, bluez-firmware, …) | 16.3 | 6.7 | no (isc-dhcp-client: NM uses its internal DHCP client — HYPOTHESIS, no `dhcp=` set in NetworkManager.conf; verify) |
| Gateway factory copy `/opt/nibe-gateway/factory` (stripped, 8.0) | 8.2 | 3.3 | only used by `export-image/prerun.sh` to seed p3 at image build |
| dpkg database `/var/lib/dpkg` | 9.9 | ~1.5 | keep (audit, CVE checks) |
| rest: libc, systemd, NetworkManager core (16.5), OpenSSH (7.7), coreutils, util-linux, glib, OpenSSL, … | ~130 | — | yes |

Top individual files (VERIFIED): tailscaled 35.8, tailscale 30.5, libicudata 29.8, hwdb.bin 12.4, initrd copy 9.5,
gateway factory 8.0, magic.mgc 7.9, vmlinuz copy 7.1, python3.11 5.4.

Package-level ranking (VERIFIED, on-disk after the first slimming's excludes): libicu72 34.2, linux-image 27.7, raspi-firmware 21.6,
libperl5.36 21.1, firmware-brcm80211 19.7, perl-modules 16.6, locales 15.1, udev 10.3, perl-base 9.7, coreutils 8.4,
libc6 8.0, libmagic-mgc 7.9, libpython3.11-stdlib 7.3, systemd 6.0, openssh-client 5.6, network-manager 5.5.
362 packages installed. The Pi's own `dpkg-query` Installed-Size list agrees (it over-counts docs that were excluded).

Not an issue any more (VERIFIED): docs/man/info/translations (5 MiB of copyright files left), apt lists, apt caches,
other boards' kernels and headers, compilers — the slimming removed them.

### 2.3 RAM side effect (VERIFIED + ESTIMATE)

MemTotal 427 MiB, MemAvailable 339 MiB. Running services that the slimming would remove: `ModemManager`,
`console-setup`/`keyboard-setup` (oneshot), `cron`, and the Python `supervise.py` wrapper (resident for the
gateway's lifetime). ESTIMATE: 10–15 MiB RSS back (Python ~8–10, ModemManager ~3–5).

## 3. Bundle format options

### 3.1 Keep ext4, fix the obvious

- **Compress harder in the pipe** (no code): `zstd -19` gives 168.7 MiB instead of 207.8 MiB for the root image
  (VERIFIED, −19 %), window stays 8 MiB so the Pi's decompression memory and speed are unchanged. Host-side cost:
  ~1–2 min with `-T0`. Only helps if the wire is the bottleneck.
- **Size the image exactly**: build the ext4 with `mke2fs -d <dir>` (or iterate `resize2fs -M`) to content + ~10 %
  instead of 757 MiB. Saves ~150 MiB of zeros that the Pi currently hashes and writes (ESTIMATE). No updater change.
- Do **not** skip writing zero blocks: ext4 needs zeroed metadata blocks (inode tables, directory blocks) to read
  as zero; stale data in the inactive slot would corrupt it. SD discard does not guarantee zeros.

### 3.2 Read-only compressed root (squashfs) — recommended

The root is already read-only at runtime (overlay with a 96 MiB tmpfs upper, `nibe-overlay` in the initramfs), and
the updater writes the root image as opaque bytes into the inactive partition. A squashfs image therefore fits the
existing design almost unchanged.

Size of today's content as a compressed image (SIM, block-wise compression of the real 17 440 files):

| Block size | xz | zstd -19 | zstd -3 |
|---|---:|---:|---:|
| 16 KiB (EROFS-like small clusters) | 203.3 | 217.0 | 236.4 |
| 128 KiB (squashfs default) | 183.8 | 200.1 | 226.7 |
| 1 MiB | 172.0 | 187.6 | 213.3 |

For comparison: whole-tree `tar | xz -9` 148.5 MiB, `tar | zstd -19 --long` 160.7 MiB.

So with no trimming at all, a squashfs root (zstd, 128 KiB) is ~200 MiB on the card and on the wire, against
757 MiB written today; with the trims in §5 it is ~100–120 MiB (ESTIMATE).

What it changes:
- SD writes per OS update −74 % or more; Pi CPU: no `zstd -d` (the image is incompressible, send it raw or with
  `zstd -1`), SHA-256 over 200 MiB instead of 757. Install time ESTIMATE: 60–230 s (raw-side-bound vs wire-bound;
  HP-35 step 1 decides), against 257 s today; with §5 trims, 40–140 s.
- Slot partitions could be much smaller on *new* cards (512 MiB instead of 1.9 GiB, giving the space to p3 for
  history). Existing cards keep their 1.9 GiB slots; `os-install` already accepts images smaller than
  the partition. Never repartition a deployed card.
- Kernel support (VERIFIED, `/boot/config-6.12.109+rpt-rpi-v6`): `CONFIG_SQUASHFS=m` with XZ and ZSTD,
  `CONFIG_SQUASHFS_DECOMP_SINGLE=y` (fine on one core); `CONFIG_EROFS_FS=m` with LZ4 and ZSTD (no LZMA).
  The current initramfs has **no squashfs module** (VERIFIED: 253 `.ko` in it, only `overlayfs` among file
  systems) — `squashfs` must be added to `/etc/initramfs-tools/modules` like `overlay`.
- Per-slot switch is natural: the slot's `cmdline.txt` comes from the bundle's `boot.tar`, so a squashfs bundle
  carries `rootfstype=squashfs` while the old slot keeps `rootfstype=ext4`; rollback works in both directions.
- Needed changes (for the implementer, not done here): build root with `mksquashfs … -comp zstd -Xcompression-level
  19 -b 131072 -reproducible -all-time $SOURCE_DATE_EPOCH -noappend` from the export rootfs (pi-gen container needs
  `squashfs-tools`); `rootfstype=squashfs` in `nibe-slots.sh`; `squashfs` in initramfs modules; `build-os-bundle.sh`
  reads fstab with `unsquashfs -cat` instead of `debugfs`; ext4-specific shrink step dropped; first image's p2
  written from the squashfs file instead of rsync into ext4 (prerun.sh). `os.rs` needs no format change (manifest
  field names stay; optionally record `rootfs_type` in the manifest for display).
- Risks: initramfs fsck on an unknown type — `_checkfs_once` runs `fsck -t squashfs`, which finds no helper and
  should return 8 (not treated as failure, which is bit 4); HYPOTHESIS, HP-35 checks the boot log. Random-read
  latency: zstd squashfs on ARM11 is slower than ext4 for cold reads (service start-up, first page-in of the
  9 MB gateway binary — though the gateway lives on p3, not in the root). ESTIMATE: boot +5–15 s; HP-35 measures.
  A failed boot of the new slot falls back through tryboot exactly like today (one-shot `tryboot.txt`).
- EROFS instead: faster random reads, but in this kernel only LZ4/LZ4HC or zstd, and Debian bookworm's
  `erofs-utils` 1.5 has no zstd; LZ4HC with small clusters lands at ~250–300 MiB (ESTIMATE from the 16 KiB row
  and LZ4's ~15 % worse ratio). Squashfs wins on size and tooling availability today.

### 3.3 Delta updates — not now

- File-level rsync into the inactive slot: breaks the "signed image hash = what boots" guarantee unless the slot
  is re-hashed after writing (reading 200–750 MiB on the Pi), needs a writable ext4 slot (incompatible with
  squashfs) and a known base. Rejected.
- Binary diff of the image (`zstd --patch-from`, bsdiff): the reference must be in memory; a 200–750 MiB
  reference does not fit 427 MiB RAM. Rejected.
- Content-defined chunking (casync/desync-style) over a reproducible squashfs: realistic reuse 50–80 % when only a
  few packages change (ESTIMATE), but needs a chunk index format, signed index, a chunk store on the host, copying
  unchanged chunks from the active slot, and a final hash of the slot. Several days of work and new failure modes,
  to save perhaps 50–80 MiB per OS update — and OS updates are rare: the gateway app has its own small signed
  update channel (app and OS stay separate). Revisit only if OS updates become frequent or move to a
  metered link. A ~100–120 MiB squashfs bundle makes this moot for a LAN/Wi-Fi install.

## 4. Alternative base systems

All Rust binaries target `arm-unknown-linux-gnueabihf` (ARMv6 hard-float, glibc). Note: **Debian's own `armhf` is
ARMv7** and does not run on a Pi Zero W; Debian `armel` is soft-float. "Debian minimal" here therefore means
Raspbian/Raspberry Pi OS packages bootstrapped minimally.

| Option | Root raw | Bundle (squashfs) | Effort | Risk to Wi-Fi/NM, Tailscale, SSH, tryboot A/B | Maintenance |
|---|---:|---:|---|---|---|
| Today | 515 MiB content, 757 MiB image | 225 MiB wire (zstd-3), 757 MiB written | — | — | Raspberry Pi OS security updates via rebuild |
| **A. Extend nibe-slim (denylist) + post-export cleanup + strip Tailscale** (§5 steps 1–4) | ~280 | ~125 wire as ext4+zstd; ~110–120 squashfs | 0.5–1 day | low: same packages for NM, wpa, sshd, polkit, avahi; build check list guards them | same as today |
| **B. A + squashfs root** (§5 step 5) | ~280 | ~110–120 | +1–2 days | low–medium: initramfs/fsck/rootfstype; tryboot fallback covers a bad boot | same |
| C. A + B + drop Python, Tailscale combined build, module allowlist (§5 steps 6–8) | ~200–210 | ~85–100 | +3–5 days | medium: watchdog/provisioning rewritten in Rust; Tailscale built from source; missing module = lost feature | Tailscale build pin (Go + tag) |
| D. pi-gen stage0+stage1+own stage (allowlist instead of stage2 + purge) | ~180–210 | ~80–95 | 3–5 days | medium: must re-create what stage2 does (WPA country/rfkill unblock, user, ssh setup, NM) | similar; smaller surprise surface on upstream changes |
| E. mmdebstrap of Raspbian bookworm `--variant=minbase` + RPi archive (kernel/firmware) | ~170–200 | ~75–90 | 1–2 weeks (replace pi-gen, re-create its config, image assembly) | medium | own build tooling instead of pi-gen; same Debian security stream |
| F. Buildroot (raspberrypi0w defconfig + NM, polkit, systemd, OpenSSH, avahi, Tailscale pkg) | ~60–90 (Tailscale ~25–45 of it) | ~30–45 | 3–6 weeks to parity | high initially: every service re-validated on hardware; tryboot/os_prefix layout is ours anyway | **high**: you own every package's CVE tracking and bumps (Buildroot LTS every quarter) |
| G. Yocto (meta-raspberrypi) | ~50–90 | ~25–45 | 1–3 months; 50+ GB build host, hours per build | high | highest; steep tooling |
| H. Alpine armhf diskless (musl, OpenRC, apkovl + modloop) | ~60–100 loaded **into RAM** | ~40–60 | 3–6 weeks: retarget to musl, rewrite all systemd units, sd_notify watchdog, journald, tmpfiles, polkit; new A/B scheme | high: no systemd; 50–100 MiB of 427 MiB RAM spent on the root | Alpine releases every 6 months |

All sizes in this table except "Today" are ESTIMATES.

Assessment: A and B take the bundle from 225 MiB wire / 757 MiB written to ~110–120 MiB for both, in about two days,
with the same OS, the same package set for every service we depend on, and the existing build/A-B/rollback path.
C/D reach roughly the size of a hand-built Debian (E) without changing distribution. F–H could save another
50–80 MiB but replace a maintained distribution with one the owner has to maintain alone; on a Pi Zero W whose SD
card is 15 GB that is not worth it.

## 5. Ranked recommendations

Savings are relative to today's content (raw) and to today's 224.8 MiB zstd-3 wire size. Raw savings also shrink the
ext4 image the Pi writes, once the image is sized to its content (step 3) or replaced by squashfs (step 5).

| # | Step | Raw MiB | Wire MiB | Effort | Re-verify on hardware |
|---|---|---:|---:|---|---|
| 1 | **Strip the Tailscale binaries** (`llvm-strip`/`arm-linux-gnueabihf-strip` after the checksum check; record the stripped hashes) | −20.1 (VERIFIED) | −16.8 (VERIFIED) | 1 h | `tailscale status`, `tailscale up` with auth key (nibe-tailscale-up), os-confirm's tailscaled check |
| 2 | **Delete build leftovers from the root at the end of export** (after `nibe-slots.sh`): `/boot/vmlinuz-*`, `/boot/initrd.img-*`, `/boot/System.map-*`, `/usr/lib/raspi-firmware/*`, all of `/usr/lib/firmware` except `brcm/brcmfmac43430-sdio.*` + their `cypress/cyfmac43430-sdio.*` targets + `regulatory.db*`, `/usr/lib/udev/hwdb.d/*` (keep hwdb.bin) | −66 (VERIFIED) | −40 (VERIFIED) | 2 h | Wi-Fi association and regdom (SE), boot, `cat /sys/module/cfg80211/parameters/ieee80211_regdom` |
| 3 | **Extend the nibe-slim purge list**: `perl libperl5.36 perl-modules-5.36`, `shared-mime-info libxml2 libicu72`, `modemmanager libqmi-* libmbim-* libqrtr-glib0 libmm-glib0`, `locales` (+ `LOCALE_DEFAULT=C.UTF-8`), `file libmagic1 libmagic-mgc`, `console-setup* keyboard-configuration xkb-data kbd`, gnupg stack but `gpgv`, `xauth` (+ X11 libs via autoremove), `groff-base ncurses-term strace bash-completion pci.ids pciutils usbutils htop wireless-tools net-tools python3-pip-whl python3-setuptools-whl python3.11-venv ppp bluez-firmware`, and `isc-dhcp-client` only after HP-35 confirms NM's internal DHCP. Keep the existing required-package check; add `perl-base` and `gpgv` to it. Also size the ext4 to content (iterate `resize2fs -M` or `mke2fs -d`) if step 5 is not done | −148 (VERIFIED for the files) | −43 (VERIFIED) | 0.5 day | full HP-29-style OS update: NM station + setup AP, avahi, polkit rules, SSH, Tailscale, os-confirm; `locale`/journal output readable |
| 4 | **Use `zstd -19` in the SSH pipe** (operator command / docs only) — only while the root is still ext4 | 0 | −39 on today's image (VERIFIED) | 5 min | none (same decompressor); compare install time |
| 5 | **Squashfs root (zstd, 128 KiB blocks)** — see §3.2 for the change list; new cards get 512 MiB slots, old cards keep theirs | written image 757 → ~200 (SIM) today, ~110–120 after 1–3 | bundle = image, ~110–120 after 1–3 | 1–2 days | HP-35 steps 2–5: boot from squashfs slot, fsck log, boot time, rollback to the ext4 slot, update back again |
| 6 | **Replace Python**: sd_notify readiness/watchdog inside the gateway (or a small Rust supervisor binary next to nibe-updater) and `nibe-provision` as a `nibe-system` subcommand; then purge python3* | −40.1 (VERIFIED), ~8–10 MiB RAM (ESTIMATE) | −13.8 (VERIFIED) | 1–2 days incl. tests | watchdog restart on hung health, degraded-but-200 does not restart (HP-11 style), provisioning file consumed (HP-30) |
| 7 | **Kernel module allowlist** + initramfs `MODULES=list`: keep brcmfmac/cfg80211/mac80211/rfkill, mmc, overlay, squashfs, vfat/nls, ext4 deps, tun, nf_tables/xt modules Tailscale uses, ipv6, USB-serial only if ever needed | ~−16 root, ~−4 initramfs (ESTIMATE) | ~−14 (ESTIMATE from VERIFIED 18.5 for all modules) | 0.5–1 day | Wi-Fi, Tailscale (netfilter mode), UART, `lsmod` diff, dmesg for request_module failures |
| 8 | **Tailscale combined binary** built from the pinned tag (`build_dist.sh --extra-small --box ./cmd/tailscaled`, `tailscale` as symlink), Go version pinned | ~−15…−25 beyond step 1 (ESTIMATE) | ~−8…−12 (ESTIMATE) | 1 day + per-release rebuild | everything Tailscale; confirm `--extra-small` keeps the features used (auth key, `status`, netfilter) |
| 9 | Small: trim `slot/` dtbs to `bcm2708-rpi-zero-w.dtb` and overlays to the ones `config.txt` uses (`disable-bt`); drop `/opt/nibe-gateway/factory` from OS bundles if the seed is moved into prerun's payload | −2.3 boot, −8.2 root | −1.5, −3.3 | 1–2 h | boot, UART (disable-bt overlay) |
| — | Not recommended now: delta updates (§3.3), EROFS, Buildroot/Yocto/Alpine (§4) | | | | |

Cumulative (ESTIMATE): steps 1–3 → content ~280 MiB, wire ~125 MiB as ext4+zstd-3 (~100 MiB with step 4);
with step 5 the image itself is ~110–120 MiB and is what is written; steps 6–8 bring it to ~85–100 MiB.
SD writes per OS update: 757 MiB → ~110 MiB. RAM: ~10–15 MiB back.

Order rationale: 1–2 are deletions of files nothing at runtime reads, so they carry almost no risk and give
~57 MiB of the wire saving. Step 3 needs a full OS update trial because package purges can take something
NetworkManager/polkit/sshd use at runtime (the existing required-package check and the os-confirm SSH/Tailscale
health gate are the safety net). Step 5 is the biggest structural win (written bytes, install time, card wear) and
is independent of 1–3; doing 1–3 first keeps the squashfs trial focused on the filesystem change alone.

## 6. Open questions / hypotheses

- Install-time bottleneck (CPU vs Wi-Fi) — HP-35 step 1.
- NM DHCP client: internal vs dhclient on this image — HP-35 step 1 (`NetworkManager --print-config`).
- initramfs fsck behaviour with `rootfstype=squashfs` and boot-time cost — HP-35 steps 3–4.
- Whether anything on the appliance shells out to `perl` (none found in repo scripts; debconf uses perl-base only).

## 7. Implementation of steps 1–4 (2026-10-25) and the dependency check behind it

Purge safety was checked against the measured package database of bundle 2026.10.25-3 (`/var/lib/dpkg/status`
and `/var/lib/apt/extended_states` read with `debugfs`, 362 installed packages) with a small closure checker (`docs/research/tools/purge-closure.py`):
(1) everything `apt-get purge` must also remove — installed packages whose `Depends`/`Pre-Depends` group (with
`Provides`) can only be satisfied by removed packages, iterated to a fixpoint; (2) what `apt-get autoremove`
then removes — auto-installed packages no longer reachable from manual/Essential packages via
Depends/Pre-Depends/Recommends/Suggests (apt's defaults); (3) intersection with a protected list (network-manager,
libnm0, wpasupplicant, openssh-server/-client/-sftp-server, avahi-daemon, libnss-mdns, dnsmasq-base, polkitd,
pkexec, sudo, zstd, python3/python3.11/python3-minimal, systemd, systemd-sysv, systemd-timesyncd, fake-hwclock,
udev, dbus, dbus-daemon, e2fsprogs, dosfstools, ca-certificates, firmware-brcm80211, raspi-firmware,
linux-image-rpi-v6, initramfs-tools, curl, iproute2, kmod, util-linux, mount, apt, gpgv, gnupg, gpg, perl-base,
debconf, isc-dhcp-client, libc-bin, libc6, coreutils, bash, dash, both archive keyrings, wireless-regdb, nftables,
libglib2.0-0, sed, grep, tar, findutils, procps, ncurses-term, less, nano, cron).

Findings:
- The first draft list (§5 step 3) **would have removed network-manager**: it *Depends* on `libmm-glib0`. Fixed
  by keeping libmm-glib0 (0.9 MiB); ModemManager itself (only *Recommended* by NM) still goes.
- `gnupg`/`gpg` kept: `raspbian-archive-keyring` depends on them, and pi-gen's `export-image/02-set-sources` runs
  `apt-get update` + `dist-upgrade` *after* stage-nibe, so a missing keyring would fail the build.
- `ncurses-term` kept (terminfo for alacritty/kitty/ghostty-type SSH clients); `isc-dhcp-client` kept until HP-35.
- Final result: 43 packages purged (2 more by reverse Depends: `libpci3`, `ssh-import-id`), 25 autoremoved
  (`libgdbm-compat4 libgudev-1.0-0 libiw30 libpcap0.8 libusb-1.0-0` X11 libs `lsb-release make` and the
  python3-requests family); **protected removed: none**. `iw` and `rfkill` are not installed (so on-device
  regdom checks use `/sys/module/cfg80211/parameters/ieee80211_regdom` or `dmesg`, not `iw reg get`).
- Files owned by those 68 packages in the measured root: 138.8 MiB raw, 40.4 MiB zstd-3 (VERIFIED).
- `nibe-prune.sh` dry run on the dumped 2026.10.25-3 root: 516 → 449 MiB apparent, guard "73 required runtime
  files present" (VERIFIED). Firmware left: the Zero W 43430 brcm names, their two `cypress/cyfmac43430-sdio.*`
  targets and `regulatory.db*`.
- Tailscale 1.104.2 stripped with llvm-strip: `tailscale` 22 392 576 B (sha256 `5c99834d…0a39dc`),
  `tailscaled` 26 020 952 B (sha256 `8cbcbf5c…71a038`); both print `1.104.2` under `qemu-arm-static` (VERIFIED).

Expected for the next bundle (ESTIMATE from the measured parts): content ~290 MiB, ~128 MiB on the wire at
zstd-3 or ~105 MiB at zstd-19, ext4 image ~400 MiB written (step 5 would make that ~110–120 MiB).

## 8. Step 5: squashfs root (implemented 2026-10-25)

Design and compatibility reasoning were in the former decision record for step 5. Numbers behind it:
- SIM (block-wise zstd-19, 128 KiB, real files of the shipped bundle 2026.10.25-4 root, 280 MiB, 11 320 files):
  102.3 MiB of compressed data; leaving out the Python files (bundle -5) 93.6 MiB. Squashfs metadata adds ~1–3 %,
  so expect a ~95–100 MiB root image instead of the 373 MiB ext4 image -4 wrote.
- VERIFIED on the build host (util-linux 2.42): `fsck -a -T -t squashfs <image>` prints
  "fsck.squashfs… not found; ignore" and exits 0, so the initramfs root check passes; the Pi's util-linux 2.38
  is checked in HP-37.
- VERIFIED (kernel config of 6.12.109+rpt-rpi-v6): `CONFIG_SQUASHFS=m` with ZSTD and XATTR; the module is now
  listed in the initramfs (`/etc/initramfs-tools/modules`) because the previous initramfs did not carry it.
- Install time: unknown which of Wi-Fi or the Pi's CPU limits it (HP-35 step 1 not run); with ~100 MiB written
  instead of 373 MiB the ESTIMATE is 40 s–2 min against 2 min 30 s for bundle -4.
