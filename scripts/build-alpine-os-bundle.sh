#!/usr/bin/env bash
# Build a signed Alpine OS bundle: squashfs root + slot boot files, signed with `nibe-release os-sign`
# (format 1), installed by the existing updater (`nibe_os_install`).
# Usage: scripts/build-alpine-os-bundle.sh PRIVATE_KEY VERSION [OUTPUT_DIR]   -> OUTPUT_DIR/nibe-os-alpine-VERSION.tar
# Needs Docker (rootless is fine) and qemu-arm binfmt with the F flag; no root on the host.
# Environment:
#   NIBE_RELEASE_PUBLIC_KEY  32-byte public key (default: release.pub next to PRIVATE_KEY)
#   NIBE_PI_BINARY / NIBE_PI_UPDATER / NIBE_PI_SYSTEM   static musl ARM binaries
#                            (default target/pi-musl/arm-unknown-linux-musleabihf/release/{nibe-gateway,nibe-updater,nibe-system})
#   NIBE_ALPINE_STUBS=1      build with placeholder binaries when those are missing (image is NOT usable)
#   NIBE_ADMIN_DIST          admin UI build (default apps/admin/dist)
#   NIBE_IMAGE_PROFILE       hardware (default) | simulated
#   NIBE_WIFI_COUNTRY        regulatory domain (default SE)
#   NIBE_GATEWAY_UID/GID     numeric ids of nibe-gateway; must match the owner of p3's state/ (default 995/992,
#                            the Raspberry Pi OS image's)
#   NIBE_ALPINE_IMAGE        builder image (default alpine:3.24), NIBE_ALPINE_BRANCH (default v3.24)
#   NIBE_ALPINE_WORKDIR      work directory (default ~/.cache/nibe-gateway/alpine-work)
#   NIBE_RELEASE             nibe-release binary (default: cargo build)
#   NIBE_SMOKE=1             smoke bundle: nibe.smoke=1 on the cmdline; the slot records traces, never commits and
#                            reboots back by itself (HP-38 step 1). VERSION must contain "smoke".
#   NIBE_TAILSCALE           pinned (default): the Tailscale release in packaging/alpine/tailscale/tailscale.lock,
#                            fetched, checked and stripped (needs llvm-strip or NIBE_STRIP); alpine: Alpine's package
#   NIBE_TAILSCALE_DIR       directory with static `tailscale` + `tailscaled` to use instead (overrides NIBE_TAILSCALE)
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
usage() { echo 'usage: build-alpine-os-bundle.sh PRIVATE_KEY VERSION [OUTPUT_DIR]' >&2; exit 2; }
[[ $# == 2 || $# == 3 ]] || usage
key=$1 version=$2 outdir=${3:-$repo/target/alpine}
[[ -f $key ]] || { echo "private key $key not found" >&2; exit 1; }
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo 'VERSION must be semver (e.g. 2026.10.25-alpine.1)' >&2; exit 2; }
smoke=${NIBE_SMOKE:-0}
[[ $smoke == 0 || $smoke == 1 ]] || { echo 'NIBE_SMOKE must be 0 or 1' >&2; exit 2; }
if [[ $smoke == 1 && $version != *smoke* ]] || [[ $smoke == 0 && $version == *smoke* ]]; then
    echo 'a smoke bundle (NIBE_SMOKE=1) must have "smoke" in VERSION, and only a smoke bundle may' >&2; exit 2
fi
out=$outdir/nibe-os-alpine-$version.tar
[[ ! -e $out ]] || { echo "refusing to overwrite $out" >&2; exit 1; }
public=${NIBE_RELEASE_PUBLIC_KEY:-$(dirname -- "$key")/release.pub}
[[ -f $public && $(wc -c < "$public") == 32 ]] || { echo "public key $public must exist and be 32 bytes (NIBE_RELEASE_PUBLIC_KEY)" >&2; exit 1; }
profile=${NIBE_IMAGE_PROFILE:-hardware}
[[ $profile == hardware || $profile == simulated ]] || { echo 'NIBE_IMAGE_PROFILE must be hardware or simulated' >&2; exit 2; }
country=${NIBE_WIFI_COUNTRY:-SE}
[[ $country =~ ^[A-Z]{2}$ ]] || { echo 'NIBE_WIFI_COUNTRY must be two uppercase letters' >&2; exit 2; }
uid=${NIBE_GATEWAY_UID:-995} gid=${NIBE_GATEWAY_GID:-992}
[[ $uid =~ ^[0-9]+$ && $gid =~ ^[0-9]+$ ]] || { echo 'NIBE_GATEWAY_UID/GID must be numeric' >&2; exit 2; }
image=${NIBE_ALPINE_IMAGE:-alpine:3.24}
branch=${NIBE_ALPINE_BRANCH:-v3.24}
command -v docker >/dev/null || { echo 'docker not found' >&2; exit 1; }
[[ -f /proc/sys/fs/binfmt_misc/qemu-arm ]] && grep -q '^flags:.*F' /proc/sys/fs/binfmt_misc/qemu-arm \
    || { echo 'qemu-arm binfmt (with the F flag) is required to run armhf package scripts' >&2; exit 1; }

work=${NIBE_ALPINE_WORKDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway/alpine-work}
cache=${XDG_CACHE_HOME:-$HOME/.cache}/nibe-gateway/alpine-apk-armhf
mkdir -p "$work" "$cache" "$outdir"
payload=$work/payload
rm -rf "$payload" "$work/out"
mkdir -p "$payload/admin"

# Binaries: static musl ARM (Alpine has no glibc; the same files run on the Raspberry Pi OS slot).
musl=$repo/target/pi-musl/arm-unknown-linux-musleabihf/release
gateway=${NIBE_PI_BINARY:-$musl/nibe-gateway}
updater=${NIBE_PI_UPDATER:-$musl/nibe-updater}
system=${NIBE_PI_SYSTEM:-$musl/nibe-system}
stubs=0
for binary in "$gateway" "$updater" "$system"; do
    [[ -f $binary ]] || stubs=1
done
if (( stubs )); then
    [[ ${NIBE_ALPINE_STUBS:-0} == 1 ]] || { echo 'musl binaries missing: run scripts/build-pi.sh (musl) or set NIBE_ALPINE_STUBS=1' >&2; exit 1; }
    echo 'WARNING: building with STUB binaries; this bundle must not be installed' >&2
    for name in nibe-gateway nibe-updater nibe-system; do
        printf '#!/bin/sh\necho "%s: stub binary (NIBE_ALPINE_STUBS=1 build)" >&2\nexit 1\n' "$name" > "$payload/$name"
        chmod 755 "$payload/$name"
    done
    echo "built with stub binaries on $(date -u +%FT%TZ)" > "$payload/STUB"
else
    python3 "$repo/packaging/alpine/alpine_image.py" check-elf "$gateway" "$updater" "$system"
    if [[ $profile == hardware ]]; then
        python3 - "$gateway" <<'PYFEATURES'
import hashlib, json, pathlib, sys
binary = pathlib.Path(sys.argv[1])
meta = pathlib.Path(str(binary) + ".features.json")
if meta.exists():
    data = json.loads(meta.read_text())
    if data.get("sha256") != hashlib.sha256(binary.read_bytes()).hexdigest() or sorted(data.get("features", [])) != ["network-manager", "serial"]:
        sys.exit("hardware bundles need the matching serial,network-manager build (features.json)")
else:
    print("note: no features.json next to the gateway binary; features not verified", file=sys.stderr)
PYFEATURES
    fi
    install -m 755 "$gateway" "$payload/nibe-gateway"
    install -m 755 "$updater" "$payload/nibe-updater"
    install -m 755 "$system" "$payload/nibe-system"
fi
install -m 644 "$public" "$payload/release.pub"
tailscale_dir=${NIBE_TAILSCALE_DIR:-}
case ${NIBE_TAILSCALE:-pinned} in
pinned)
    if [[ -z $tailscale_dir ]]; then
        source "$repo/packaging/alpine/tailscale/tailscale.lock"
        tgz=$cache/../tailscale_${TAILSCALE_VERSION}_arm.tgz
        if [[ ! -f $tgz ]]; then
            curl -fsSL -o "$tgz.part" "https://pkgs.tailscale.com/stable/tailscale_${TAILSCALE_VERSION}_arm.tgz"
            mv "$tgz.part" "$tgz"
        fi
        echo "$TAILSCALE_ARM_SHA256  $tgz" | sha256sum -c --quiet - || { echo 'Tailscale tarball checksum mismatch' >&2; exit 1; }
        tailscale_dir=$work/tailscale-pinned
        rm -rf "$tailscale_dir"; mkdir -p "$tailscale_dir"
        tar -xzf "$tgz" -C "$tailscale_dir" --strip-components=1 \
            "tailscale_${TAILSCALE_VERSION}_arm/tailscale" "tailscale_${TAILSCALE_VERSION}_arm/tailscaled"
        "$repo/packaging/alpine/tailscale/strip-tailscale.sh" "$tailscale_dir"
    fi ;;
alpine) [[ -z $tailscale_dir ]] || { echo 'NIBE_TAILSCALE=alpine and NIBE_TAILSCALE_DIR exclude each other' >&2; exit 2; } ;;
*) echo 'NIBE_TAILSCALE must be pinned or alpine' >&2; exit 2 ;;
esac
if [[ -n $tailscale_dir ]]; then
    NIBE_TAILSCALE_DIR=$tailscale_dir
    mkdir -p "$payload/tailscale"
    for name in tailscale tailscaled; do
        python3 "$repo/packaging/alpine/alpine_image.py" check-elf "$NIBE_TAILSCALE_DIR/$name"
        install -m 755 "$NIBE_TAILSCALE_DIR/$name" "$payload/tailscale/$name"
    done
fi
admin=${NIBE_ADMIN_DIST:-$repo/apps/admin/dist}
if [[ -f $admin/index.html ]]; then
    cp -a "$admin/." "$payload/admin/"
elif (( stubs )) || [[ $profile == simulated ]]; then
    echo '<!doctype html><title>NIBE Gateway</title><p>Admin UI not included in this build.</p>' > "$payload/admin/index.html"
else
    echo "admin UI missing ($admin/index.html): build the frontend" >&2; exit 1
fi
# Factory release manifest, as the retired Raspberry Pi OS image wrote it (the initial trust anchor of a fresh card).
target=$(sed -n 's/^pub const PI_TARGET: &str = "\(.*\)";/\1/p' "$repo/crates/updater/src/artifact.rs")
python3 - "$payload" "$repo/Cargo.toml" "${target:-arm-unknown-linux-gnueabihf}" <<'PYFACTORY'
import hashlib, io, json, pathlib, sys, tarfile, tomllib
payload, cargo, target = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
version = tomllib.loads(cargo.read_text())["workspace"]["package"]["version"]
buffer = io.BytesIO()
with tarfile.open(fileobj=buffer, mode="w") as archive:
    for path in [payload / "nibe-gateway", *sorted((payload / "admin").rglob("*"))]:
        if path.is_file():
            data = path.read_bytes()
            entry = tarfile.TarInfo(str(path.relative_to(payload)))
            entry.size = len(data)
            archive.addfile(entry, io.BytesIO(data))
manifest = dict(format=1, version=version, compatibility="nibe-gateway-app-v1", target=target, config_schema_min=2,
                config_schema_max=2, sha256=hashlib.sha256(buffer.getvalue()).hexdigest())
(payload / "manifest.json").write_text(json.dumps(manifest))
PYFACTORY

# Kernel: the Raspberry Pi OS ARMv6 kernel, pinned by version and SHA-256 (packaging/alpine/kernel.lock).
source "$repo/packaging/alpine/kernel.lock"
kernel_deb=$cache/../$RPI_KERNEL_DEB
if [[ ! -f $kernel_deb ]]; then
    curl -fsSL -o "$kernel_deb.part" "$RPI_KERNEL_URL"
    mv "$kernel_deb.part" "$kernel_deb"
fi
echo "$RPI_KERNEL_SHA256  $kernel_deb" | sha256sum -c --quiet - || { echo 'kernel package checksum mismatch' >&2; exit 1; }

echo "== building the root in $image (armhf packages via qemu binfmt)"
start=$SECONDS
docker run --rm --platform linux/amd64 \
    -v "$repo/packaging:/src:ro" -v "$work:/work" -v "$cache:/cache" -v "$kernel_deb:/kernel/kernel.deb:ro" \
    -e NIBE_ALPINE_BRANCH="$branch" -e NIBE_WIFI_COUNTRY="$country" -e NIBE_GATEWAY_UID="$uid" \
    -e NIBE_GATEWAY_GID="$gid" -e NIBE_IMAGE_PROFILE="$profile" -e NIBE_SMOKE="$smoke" -e NIBE_IMAGE_VERSION="$version" \
    "$image" sh /src/alpine/build-root.sh
echo "== root built in $(( SECONDS - start )) s"

release=${NIBE_RELEASE:-}
if [[ -z $release ]]; then
    cargo build -q --release -p nibe-updater --bin nibe-release --manifest-path "$repo/Cargo.toml"
    release=$repo/target/release/nibe-release
fi
"$release" os-sign "$key" "$version" "$work/out/rootfs.img" "$work/out/boot" "$out"
mib() { awk -v b="$1" 'BEGIN { printf "%.1f MiB (%d bytes)", b / 1048576, b }'; }
root_bytes=$(cat "$work/out/root-bytes")
sqfs_bytes=$(stat -c %s "$work/out/rootfs.img")
boot_bytes=$(du -sb "$work/out/boot" | cut -f1)
bundle_bytes=$(stat -c %s "$out")
{
    echo "Alpine OS bundle $version ($profile, regdom $country, kernel $(cat "$work/out/kernel-version")$( (( smoke )) && echo ', SMOKE: never commits'))"
    echo "  root content:  $(mib "$root_bytes")"
    echo "  squashfs:      $(mib "$sqfs_bytes")"
    echo "  boot files:    $(mib "$boot_bytes")"
    echo "  bundle:        $(mib "$bundle_bytes")"
    echo "  packages:      $(wc -l < "$work/out/packages.txt") (sizes: $work/out/package-sizes.txt)"
    (( stubs == 0 )) || echo "  WARNING: STUB binaries; do not install"
} | tee "$outdir/nibe-os-alpine-$version.txt"
echo "OS bundle: $out"
echo "Install: NIBE_OS_ZSTD_LEVEL=1 NIBE_SSH_HOST=... bash -c 'source scripts/ssh-maintenance.sh && nibe_os_install $out'"
