#!/usr/bin/env bash
set -euo pipefail
# Raspbian armhf is ARMv6; Debian armhf requires ARMv7 and is not a substitute.
sysroot=${1:?usage: fetch-pi-sysroot.sh EMPTY_DIRECTORY}
[[ ! -e $sysroot ]] || { echo 'Choose a new directory for the smoke-test sysroot.' >&2; exit 1; }
package=$(mktemp)
trap 'rm -f "$package"' EXIT
curl -fL --retry 2 --max-time 120 \
    'https://archive.raspbian.org/raspbian/pool/main/g/glibc/libc6_2.36-9+rpi1+deb12u14_armhf.deb' -o "$package"
printf '%s  %s\n' '16b0a9a9111592161fb76746750d0de7112723a762bd7c66f333ce1b2435e39b' "$package" | sha256sum --check
mkdir -p "$sysroot"
ar p "$package" data.tar.xz | tar -xJ -C "$sysroot"
printf 'Raspbian Bookworm glibc 2.36 sysroot: %s\n' "$sysroot"
