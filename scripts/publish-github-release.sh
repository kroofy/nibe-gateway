#!/usr/bin/env bash
# Build, sign and publish the current workspace version as a GitHub Release (ADR 0124).
# The gateway's Updates page offers the attached nibe-gateway-<version>.tar; the updater verifies its signature.
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

git tag -a "$tag" -m "nibe-gateway $version"
git push origin "$tag"
gh release create "$tag" "${draft[@]}" --verify-tag --title "nibe-gateway $version" --generate-notes \
    "$artifact" "$artifact.sha256"
printf 'Published %s with %s\n' "$tag" "$artifact"
