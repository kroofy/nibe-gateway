# Pi Zero W / WH cross compilation

**Default since 2026-10-25: static musl** (`NIBE_PI_LIBC=musl`, target `arm-unknown-linux-musleabihf`, output
`target/pi-musl/arm-unknown-linux-musleabihf/release/`), which the Alpine appliance runs (ADR 0117). The glibc
build described below remains available with `NIBE_PI_LIBC=gnu` (output `target/pi/...`); `check-pi-elf.py --libc`
checks either.

The original Zero is ARM1176/ARMv6 with VFP hard float, not ARMv7 or AArch64.
The Rust target is exactly `arm-unknown-linux-gnueabihf`. See the
[official Rust ARM target documentation](https://doc.rust-lang.org/rustc/platform-support/arm-linux.html).
Debian's generic armhf compiler/sysroot normally assumes ARMv7; do not silently
substitute it for Raspbian ARMv6 libraries.

```bash
./scripts/bootstrap-tools.sh
./scripts/build-pi.sh
./scripts/check-pi-elf.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway
```

The build uses Rust 1.90.0, cargo-zigbuild 0.20.1, and Zig 0.14.1. The Zig target
suffix `.2.28` selects glibc 2.28 symbol versions while Cargo's directory retains
the plain Rust triple. Raspberry Pi OS Bookworm's glibc is 2.36. See
[cargo-zigbuild's glibc version documentation](https://github.com/rust-cross/cargo-zigbuild#specify-glibc-version).
Pinning tools alone does not prove the output: `check-pi-elf.py` requires ELF32
ARM, `Tag_CPU_arch: v6`, EABI5 hard-float, the armhf interpreter, and no required
glibc version newer than 2.36. It runs automatically after every Pi build and
before image staging. Host CPU Rust flags are rejected. Dependencies with native
code/system libraries need another explicit ARMv6 review and runtime smoke test.

Hardware bench builds add production adapters through an allowlist (the `dev`
feature is always rejected); use a separate target directory so the factory
binary is not overwritten:

```bash
NIBE_PI_FEATURES=serial NIBE_TARGET_DIR=target/pi-serial ./scripts/build-pi.sh
NIBE_PI_FEATURES=serial,network-manager NIBE_TARGET_DIR=target/pi-full ./scripts/build-pi.sh
```

Both pass the ELF gate (verified by WS08 on 2026-10-22; the serial binary also
starts under QEMU ARM1176 and attempts to open its UART).

Output: `target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway`. Set
`NIBE_TARGET_DIR` to move the Pi target directory; native tests retain their own
target files. No changes to root Cargo.toml or Rust sources are made by WS03.

WS09 makes `serial,network-manager` the default Pi feature set. To build an
explicit simulation-only development binary set `NIBE_PI_FEATURES=''`; it cannot
be assembled into the default hardware image. `build-pi.sh` emits a SHA-bound
`nibe-gateway.features.json` sidecar checked during image preparation.

Optional runtime check, requiring `qemu-arm-static`, `curl`, `ar`, `tar`, and
`sha256sum` on the host:

```bash
./scripts/fetch-pi-sysroot.sh /tmp/nibe-bookworm-sysroot
./scripts/smoke-pi.py target/pi/arm-unknown-linux-gnueabihf/release/nibe-gateway /tmp/nibe-bookworm-sysroot
```

Choose a fresh sysroot directory. The fetch script pins Raspbian libc6
`2.36-9+rpi1+deb12u14` and SHA-256
`16b0a9a9111592161fb76746750d0de7112723a762bd7c66f333ce1b2435e39b`.
QEMU uses `-cpu arm1176`, the actual Pi Zero CPU family; it checks real HTTP
health/status, disabled controls, and graceful SIGTERM. This validates userspace
instructions/linking, not Pi boot, UART timing, radio, kernel drivers, or RAM
behavior. CI performs this check on the release binary.

The first proof on 2026-10-22 used a throwaway crate at `/tmp/nibe-ws03-hello`
before building the concurrent WS02 workspace. Its ELF reports v6/hard float,
highest glibc requirement 2.28, and it runs with the same Raspbian sysroot under
QEMU ARM1176. Reproduce the initial hello proof independently:

```bash
mkdir -p /tmp/nibe-armv6-proof/src
cat > /tmp/nibe-armv6-proof/Cargo.toml <<'EOF'
[package]
name = "nibe-armv6-proof"
version = "0.1.0"
edition = "2021"
EOF
echo 'fn main() { println!("ARMv6 works"); }' > /tmp/nibe-armv6-proof/src/main.rs
./scripts/with-build-tools.sh cargo zigbuild --release --target arm-unknown-linux-gnueabihf.2.28 --manifest-path /tmp/nibe-armv6-proof/Cargo.toml
./scripts/check-pi-elf.py /tmp/nibe-armv6-proof/target/arm-unknown-linux-gnueabihf/release/nibe-armv6-proof
qemu-arm-static -cpu arm1176 -L /tmp/nibe-bookworm-sysroot /tmp/nibe-armv6-proof/target/arm-unknown-linux-gnueabihf/release/nibe-armv6-proof
```
