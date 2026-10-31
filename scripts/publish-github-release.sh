#!/usr/bin/env bash
# Build, sign and publish the current workspace version as a GitHub Release (ADR 0124).
# The gateway's Updates page offers the attached nibe-gateway-<version>.tar; the updater verifies its signature.
# New installations flash the attached fresh-card image nibe-gateway-alpine-<version>.img.xz (docs/dev/image-build.md),
# built from an OS bundle of the same version. The release notes are the version's section in CHANGELOG.md.
# Needs what build-pi.sh, build-alpine-os-bundle.sh and inspect-alpine-image.sh need (Docker, qemu-arm binfmt).
# Usage: NIBE_SIGNING_KEY=~/.config/nibe-gateway/release.key scripts/publish-github-release.sh [--draft]
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo"
: "${NIBE_SIGNING_KEY:?Set NIBE_SIGNING_KEY to a local 0600 release signing key file}"
draft=()
case "${1:-}" in
    '') ;;
    --draft) draft=(--draft) ;;
    *) echo 'usage: publish-github-release.sh [--draft]' >&2; exit 2 ;;
esac
version=$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("Cargo.toml").read_text())["workspace"]["package"]["version"])')
tag="v$version"
[[ -z $(git status --porcelain) ]] || { echo 'Commit or discard local changes first: a release must match a commit.' >&2; exit 1; }
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && { echo "Tag $tag already exists; bump the workspace version." >&2; exit 1; }
gh release view "$tag" >/dev/null 2>&1 && { echo "GitHub release $tag already exists." >&2; exit 1; }
notes=$(mktemp)
trap 'rm -f "$notes"' EXIT
python3 "$repo/scripts/changelog-notes.py" "$version" > "$notes"

./scripts/build-pi.sh
./scripts/build-frontend.sh
out="$repo/target/releases"
mkdir -p "$out"
artifact="$out/nibe-gateway-$version.tar"
binary="$repo/target/pi-musl/arm-unknown-linux-musleabihf/release/nibe-gateway"
# The target label stays arm-unknown-linux-gnueabihf: it is the updater's platform id; the binary is static musl.
cargo run --quiet -p nibe-updater --bin nibe-release -- sign "$NIBE_SIGNING_KEY" "$version" \
    arm-unknown-linux-gnueabihf "$binary" "$repo/apps/admin/dist" "$artifact"
(cd "$out" && sha256sum "nibe-gateway-$version.tar" > "nibe-gateway-$version.tar.sha256")
# Fresh-card image: an OS bundle of this version (the same binaries and UI) laid out as a whole SD card.
"$repo/scripts/build-alpine-os-bundle.sh" "$NIBE_SIGNING_KEY" "$version"
"$repo/scripts/build-alpine-image.sh" "$repo/target/alpine/nibe-os-alpine-$version.tar"
image="$repo/target/alpine/nibe-gateway-alpine-$version.img.xz"
"$repo/scripts/inspect-alpine-image.sh" "$image"

git tag -a "$tag" -m "nibe-gateway $version"
git push origin "$tag"
gh release create "$tag" "${draft[@]}" --verify-tag --title "nibe-gateway $version" --notes-file "$notes" \
    "$artifact" "$artifact.sha256" "$image" "$image.sha256"
printf 'Published %s with %s and %s\n' "$tag" "$artifact" "$image"
