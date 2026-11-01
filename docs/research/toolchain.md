# Toolchain: building for the Pi Zero W (ARMv6) and pi-gen notes

WS01 research input for WS03 (which owns the actual justfile/CI/pi-gen). 2026-10-22.

## 1. Target facts
| Fact | Label | Evidence |
|---|---|---|
| BCM2835 = ARM1176JZF-S: **ARMv6 + VFPv2**, no NEON, no Thumb-2. Binaries built for ARMv7 (`armv7-unknown-linux-gnueabihf`, Debian `armhf` gcc defaults) die with SIGILL. | VERIFIED-SOURCE | `rustc --print target-cpus --target arm-unknown-linux-gnueabihf` lists `arm1176jzf-s`; Rust platform support (below) |
| Rust targets: `arm-unknown-linux-gnueabihf` = "Armv6 Linux, hardfloat (kernel 3.2+, glibc 2.17)"; `arm-unknown-linux-musleabihf` = "Armv6 Linux with musl 1.2.5, hardfloat". Both Tier 2 with prebuilt std. | VERIFIED-SOURCE | https://doc.rust-lang.org/rustc/platform-support.html (fetched 2026-10-22) |
| Owner's current OS (NibePi 1.1.1) is Raspberry Pi OS **Bullseye → glibc 2.31**; a new pi-gen image (Trixie) has a newer glibc. | VERIFIED-SOURCE | NibePi README.en.md |
| Local dev machine here: rustc 1.99.1 from Arch (no rustup), so ARM std is not installed system-wide — cross builds were **not** executed by WS01 (WS03 did). | observed | `rustc --version`, `rustup` absent |

## 2. Options
| Option | How | Pros | Cons |
|---|---|---|---|
| **A. `cargo-zigbuild`** | `rustup target add arm-unknown-linux-musleabihf` (or `…gnueabihf`), `pip install ziglang cargo-zigbuild` / `cargo install cargo-zigbuild`; `cargo zigbuild --release --target arm-unknown-linux-musleabihf` or `--target arm-unknown-linux-gnueabihf.2.31` | No Docker; zig provides ARMv6-correct libc/crt; glibc version pinning via suffix | Zig version churn; glibc static not supported (use musl for static) — cargo-zigbuild README |
| **B. `cross`** | `cargo install cross`; `cross build --release --target arm-unknown-linux-gnueabihf` | Proven images: crosstool-ng **armv6 + vfp, glibc 2.17** (`docker/crosstool-config/arm-unknown-linux-gnueabihf.config`: `CT_ARCH_ARCH="armv6"`, `CT_ARCH_FPU="vfp"`, `CT_GLIBC_VERSION="2.17"`); includes qemu runner so `cross test` works | Needs Docker/Podman; slower CI |
| C. Raw rustup target + distro gcc linker | `gcc-arm-linux-gnueabihf` as linker | Simple | Debian/Ubuntu armhf gcc targets **ARMv7**; crt/libgcc objects can SIGILL on ARMv6 → avoid |
| D. Native build on the Zero | `cargo build` on the Pi | Exact target | 512 MB RAM, hours per build, wears SD → only as last resort |

### Recommendation and reconciliation with WS03
WS03 has since decided: `cargo-zigbuild` with
`--target arm-unknown-linux-gnueabihf.2.28` (glibc baseline 2.28, ELF gate rejecting ARMv7). That is compatible
with this research:
1. glibc 2.28 ≤ 2.31, so the **same binary also runs on the owner's NibePi Bullseye** — required for the in-place
   trial in feasibility.md §5 (Phase 2/3). Keep the baseline ≤ 2.31 as long as that trial path matters.
2. `arm-unknown-linux-musleabihf` (fully static) remains a fallback if a glibc-version problem appears on the
   NibePi OS; it needs no other change (pure-Rust deps, rustls instead of OpenSSL).
3. Do not set `-C target-cpu` beyond the target default (ARMv6) unless measured.
4. Release profile for size/RAM: `opt-level="s"`, `lto=true`, `codegen-units=1`, `strip=true`. Avoid
   `panic="abort"` unless systemd restart semantics are relied upon (it would also kill the serial task).

### Verifying an ARMv6 binary without hardware
```
readelf -A target/arm-unknown-linux-musleabihf/release/nibe-gateway | grep -E 'Tag_CPU_arch|Tag_FP_arch'
#   expect: Tag_CPU_arch: v6 (or v6KZ), Tag_FP_arch: VFPv2 ; anything v7 = will SIGILL on the Zero
qemu-arm -cpu arm1176 target/arm-unknown-linux-musleabihf/release/nibe-gateway --version
cross test --target arm-unknown-linux-gnueabihf        # runs unit tests under qemu
```
The final proof is running on the Zero (HARDWARE_PENDING).

## 3. pi-gen notes (image build, M4)
Source: https://github.com/RPi-Distro/pi-gen README (master, fetched 2026-10-22).
* `master` branch builds **32-bit armhf** images; "64 bit images should be built from the `arm64` branch". The Zero W
  is ARMv6 → **use master / armhf** (Raspbian armhf is ARMv6-compatible; Debian armhf is not). [VERIFIED-SOURCE]
* Default `RELEASE=trixie`; `IMG_NAME` default `raspios-$RELEASE-$ARCH`.
* Lite image = stages 0–2: `touch stage3/SKIP stage4/SKIP stage5/SKIP stage4/SKIP_IMAGES stage5/SKIP_IMAGES`.
  Add our own stage via `STAGE_LIST="stage0 stage1 stage2 stage-nibe"` and an `EXPORT_IMAGE` file in it.
* Useful config: `FIRST_USER_NAME`, `FIRST_USER_PASS`, `DISABLE_FIRST_BOOT_USER_RENAME=1`, `ENABLE_SSH`,
  `PUBKEY_SSH_FIRST_USER`, `PUBKEY_ONLY_SSH`, `TARGET_HOSTNAME`, `LOCALE_DEFAULT`, `TIMEZONE_DEFAULT`.
* Build in Docker: `./build-docker.sh` (needs binfmt/loop devices; `CONTINUE=1`, `PRESERVE_CONTAINER=1` for reruns).
* Our stage must: install the static binary + systemd unit, add `enable_uart=1` and `dtoverlay=disable-bt` to
  `config.txt`, remove `console=serial0,115200` from `cmdline.txt`, disable `hciuart`, set up the read-only/overlay
  root and data partition per brief 13 (WS03/WS07).
* HYPOTHESIS to check on first boot: Raspberry Pi OS Trixie armhf still boots on the Zero W (kernel `kernel.img`
  for BCM2835). If not, fall back to `RELEASE=bookworm`.
