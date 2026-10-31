# WS03 — Tooling, CI, cross-compile, packaging

- **Milestone:** M4
- **Status:** done-hw (Alpine packaging live on the owner's Pi); pi-gen / Raspberry Pi OS / systemd parts retired 2026-10-25; fresh-card first-boot layout (ADR 0129) done-sim 2026-10-31, HP-47
- **Owns:** justfile, .github/workflows/, scripts/, packaging/alpine/, packaging/common/, docs/dev/ (packaging/systemd/ and packaging/raspberry-pi/ were removed 2026-10-25)
- **Brief:** [03-stack-and-architecture.md](../brief/03-stack-and-architecture.md), [12-deploy-and-diagnostics.md](../brief/12-deploy-and-diagnostics.md), [13-image-and-filesystem.md](../brief/13-image-and-filesystem.md), [15-milestones.md](../brief/15-milestones.md)

## Checklist
- [x] justfile with recipes: dev, test, build-pi, deploy, logs, debug, rollback, image (deploy/rollback may call scripts that WS07 fills in)
- [x] ARMv6 cross-compile path (arm-unknown-linux-gnueabihf) that works on this machine (system rust, no rustup: try cargo-zigbuild/zig, cross, or mise rust) — prove with a build of a hello binary or the gateway
- [x] GitHub Actions: fmt, clippy, test, frontend build, ARMv6 build artifact, image build (manual dispatch)
- [x] systemd units: nibe-gateway.service (hardening, watchdog/readiness), tmpfs logs
- [x] pi-gen custom stage: 32-bit Lite, NetworkManager, avahi, UART enable (disable BT on PL011), overlay/read-only root, /var/lib/nibe-gateway persistent partition, ship binary+UI+units
- [x] docs/dev/: setup, cross-compile, image build instructions

## Verify
Run from the repository root:

```bash
./scripts/bootstrap-tools.sh
export PATH="$HOME/.local/share/nibe-build-tools/bin:$PATH"
just --list
cargo test --workspace
./scripts/with-build-tools.sh cargo test --workspace --all-features --locked
./scripts/with-build-tools.sh cargo fmt --all -- --check
./scripts/with-build-tools.sh cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/check-tooling.sh
systemd-analyze verify packaging/systemd/nibe-gateway.service
mise exec aqua:rhysd/actionlint@1.7.12 shellcheck@0.10.0 -- actionlint
mise exec shellcheck@0.10.0 -- shellcheck -x scripts/*.sh packaging/raspberry-pi/stage-nibe/prerun.sh packaging/raspberry-pi/stage-nibe/00-install/00-run.sh packaging/raspberry-pi/export-image/prerun.sh packaging/raspberry-pi/export-image/04-data-partuuid/00-run.sh packaging/raspberry-pi/stage-nibe/00-install/files/nibe-overlay
./scripts/build-pi.sh
./scripts/check-pi-elf.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway
# New directory; the already-tested local sysroot is /tmp/nibe-ws03-sysroot.
./scripts/fetch-pi-sysroot.sh /tmp/nibe-bookworm-sysroot
./scripts/smoke-pi.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway /tmp/nibe-bookworm-sysroot
./scripts/build-frontend.sh
NIBE_ALLOW_MISSING_UI=1 ./scripts/build-image.sh --prepare-only
# Expected failure here until rootful Docker is available:
./scripts/build-image.sh --check
```

`cargo run -p nibe-gateway -- --device simulated --bind 127.0.0.1:18093`
was also smoke-tested through `/health` and `/api/v1/status`, then terminated
cleanly. See `docs/dev/` for the full hello proof and image build procedure.
Actionlint/ShellCheck are optional additional developer checks installed here
through mise, not production dependencies.

## Open issues / hardware-pending
- **Actual image generation is blocked on this host:** Docker 29.9.1 has `name=rootless`; pi-gen needs rootful privileged containers and loop access. `--check` reports that exact missing prerequisite before building. The prepared pi-gen checkout is `~/.cache/nibe-gateway/pi-gen`; no flashable image/boot success is claimed.
- pi-gen file-install smoke, shell validation, and stage preparation pass. Package installation/chroot, initramfs generation, three-partition image export, actual systemd supervision, boot, memory, persistent state/power loss, and UART need the rootful builder / physical Pi checks appended to `HARDWARE_PENDING.md`.
- Frontend absent until WS05: CI explicitly skips frontend when no package exists; normal images require built UI; development placeholder requires `NIBE_ALLOW_MISSING_UI=1`.
- Factory service intentionally runs simulator on loopback, no dev endpoints, controls disabled. WS04 adds hardware, WS06 adds authenticated radio/LAN provisioning and identity persistence; enable HTTP mDNS advertisement only then. SSH is disabled and initial user password locked.
- WS07 implements `scripts/deploy-release.sh` / `scripts/rollback-release.sh` and authorized expiring debug/capture. Current recipes fail explicitly for absent deployment implementations; diagnostic recipes are read-only.
- GitHub workflows were linted locally but not dispatched remotely. `done-sim` means tooling/stage implemented and locally verified; M4 acceptance remains pending image generation and physical boot.

## Log
- 2026-10-22: Implemented just recipes, isolated user-level build bootstrap, pinned ARMv6 GNU/Zig path, ELF gate, Raspbian sysroot fetch and ARM1176/QEMU HTTP smoke. First proved a throwaway hello crate in `/tmp/nibe-ws03-hello`, then built the real gateway (about 2.9 MiB stripped); both report `Tag_CPU_arch: v6`, EABI5 hard-float and highest required glibc 2.28, below Bookworm 2.36. The real gateway's QEMU health/status, disabled controls and SIGTERM checks pass.
- 2026-10-22: Added CI fmt/clippy/default-and-dev tests, conditional frontend build, ARMv6 artifact/runtime smoke, and manual pi-gen image workflow. Added hardened notify/watchdog service with Python standard-library HTTP supervisor, volatile bounded journals/logs, pinned Bookworm Lite stage, PL011/BT/console setup, capped RAM root overlay, persistent third partition with root-owned releases/network/SSH state and a service-owned state directory. Prepared the real binary and explicit placeholder UI in isolated pi-gen. ADRs 0030/0031 record decisions.
- 2026-10-22: Verification passed: system `cargo test --workspace`; pinned-toolchain all-feature tests, fmt, strict clippy; three tooling tests (stage installation, readiness/watchdog/signal forwarding, exit propagation); systemd unit verification; actionlint and ShellCheck; native simulation HTTP smoke; real ARMv6 release build and ARM1176/QEMU HTTP smoke; pi-gen `--prepare-only`. Initial concurrent WS02 compiler error resolved by WS02 before final successful builds. Frontend step explicitly skipped because WS05 has not created it. Full image preflight fails on rootless Docker; exact build/boot/UART/storage/watchdog/power-loss steps appended to hardware pending.
- 2026-10-22: Stayed within WS03 implementation paths. Required cross-workstream metadata exceptions: added ADRs, appended hardware checklist, updated this file and only the WS03 row of PROGRESS.md. Cargo verification generated/used the workspace lockfile as part of the concurrent WS02 workspace; no Rust source or root Cargo.toml edits, no git commit, no physical device changes. User-level installs use mise/Python venv/Cargo without sudo; system Rust and global defaults remain unchanged.
- 2026-10-25: ADR 0117 steps 1–4 (image slimming, accepted by the owner): `packaging/raspberry-pi/strip-tailscale.sh` (called by `build-image.sh`; stripped hashes installed as `/usr/share/nibe-gateway/tailscale.sha256sums`), `export-image/nibe-prune.sh` (inserted after `nibe-slots.sh`; deletes root boot/firmware/hwdb-source copies, then a 73-path runtime guard), extended `nibe-slim.sh` purge list and required-package check (37 packages, dependency closure checked against bundle 2026.10.25-3 — see docs/research/image-size-2026-10-25.md §7), `nibe_os_install` (zstd -19 stream) in `scripts/ssh-maintenance.sh`, operator commands in docs/dev/image-build.md. Verify: `python3 -m unittest discover -s scripts/tests` (new PruneTest, StripTailscaleTest, OsInstallPipeTest). Not built yet; hardware checks HP-35 step 2. Open: isc-dhcp-client purge waits for HP-35; squashfs (step 5) proposed.
- 2026-10-25: ADR 0117 step 5 (squashfs root, accepted by the owner): `export-image/nibe-squash.sh` (build + verify before pi-gen unmounts the root; write into p2 after zerofree), `squashfs-tools` added to pi-gen's Dockerfile/depends by `build-image.sh` (which also inserts the hooks; `NIBE_ROOT_FS=ext4` keeps the old layout), slot cmdline `rootfstype=squashfs panic=10` (nibe-slots.sh), `squashfs` initramfs module and forced overlay for squashfs roots (00-run.sh, nibe-overlay), `nibe-prune.sh` check-only mode, `packaging/raspberry-pi/rootfs-image.py` + squashfs path in `build-os-bundle.sh`; crates/updater os.rs refuses rootfstype/image mismatches in os-sign and os-install. Verify: `python3 -m unittest discover -s scripts/tests`, `cargo test -p nibe-updater`. Not built yet; hardware HP-37. Cross-workstream: crates/updater/src/os.rs (WS07).
- 2026-10-25 (bookkeeping): pi-gen, `packaging/raspberry-pi/`, `packaging/systemd/`, `scripts/build-image.sh`, `scripts/build-os-bundle.sh` and the image workflow were removed (commit c2caee1; ADR 0117/0118). Packaging is now `packaging/alpine/` plus `packaging/common/`: `scripts/build-alpine-os-bundle.sh`, `scripts/build-alpine-image.sh` (fresh card), `scripts/pi-diagnostics.sh` (OpenRC, `/var/log/messages`), and static musl is the default build. The ext4/squashfs steps were verified before the switch (HP-35/36/37), then Alpine (HP-38). The 2026-10-22 and earlier 2026-10-25 entries above describe the retired pi-gen path. Open: HP-40 (fresh card, bench).
- 2026-10-31: Fresh-card image without p3/p4; the first boot completes the layout (ADR 0129, owner decision; ADR 0106
  amendment). `packaging/alpine/initramfs/init`: guarded first-boot step (marker `nibe-first-boot` on p1 + MBR entries
  3/4 empty + entry 2 short, or resume: full table + marker + p3 without boot logs), busybox-only table write (disk id
  kept, first MiB of p3/p4 wiped, `blockdev --rereadpt`, reboot-and-resume fallback), p3 seeded in a tmpfs and created
  with the root's `mke2fs -d` via chroot, marker deleted last; without the marker a missing p3 stops the boot.
  `card_image.py`: `p2`, short `mbr`, `inspect` for the image, `card` for a first-booted card; `build-card.sh` no
  longer builds p3; `alpine_image.py` requires `sbin/mke2fs`; QEMU check boots twice on an 8 GiB card with an old
  ext4 where p3 lands. Tests: `test_alpine_card.py` (short layout, marker, card checks), `InitramfsTest` (plan table,
  layout states, byte-exact table write vs `write_mbr`, wipe, seed tree, ordering).
  Result: `nibe-gateway-alpine-0.1.31-firstboot.1.img.xz` (bundle signed with the release key, disk id 1c566130):
  590348288 bytes raw (was 4.9 GiB), 59340092 bytes xz. QEMU: 18/18 PASS (layout completed and re-read in ~1 s, p3
  created and seeded in ~10 s, marker removed, p4 wiped, second boot plan none with identical MBR and p3 UUID, no new
  p1 trace). Seed comparison (same bundle root, HEAD's build-time seed vs initramfs `seed_data` under busybox):
  identical files, hashes, owners and modes except `system/connections` and `system/NetworkManager` 0700 instead of
  0755 (the old `cp -a src/.` copied the root's mode; `nibe-persist` sets 0700 on every boot anyway). Setup AP: under
  QEMU the gateway opens its store on the new p3 (`state/config.json`, `setup-secrets.json`, 995:992 0600), NM runs
  with no saved profiles and the Wi-Fi fallback finds none; the gateway is not healthy under QEMU (no wlan0/UART,
  same as the 0.1.31 QEMU baseline), so the AP itself is hardware-only (HP-47 step 4; the setup AP has never run on
  hardware, HP-40 step 3 / HP-13).
  Verify: `python3 -B -m unittest discover -s scripts/tests`; `cargo test --workspace`;
  `scripts/build-alpine-os-bundle.sh ~/.config/nibe-gateway/release.key VERSION`,
  `scripts/build-alpine-image.sh target/alpine/nibe-os-alpine-VERSION.tar`,
  `scripts/inspect-alpine-image.sh target/alpine/nibe-gateway-alpine-VERSION.img.xz`,
  `scripts/qemu-boot-alpine-image.sh target/alpine/nibe-gateway-alpine-VERSION.img.xz`.
  Open: HP-47 on the owner's spare card (table re-read on the Pi's SD controller, flash and first-boot time, setup
  AP). No commits.
