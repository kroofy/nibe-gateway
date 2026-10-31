#!/usr/bin/env bash
# Strip the pinned Tailscale ARM binaries in place (ADR 0116; used by scripts/build-alpine-os-bundle.sh): they ship with DWARF debug info and symbol tables
# (66 MiB → 46 MiB for 1.104.2). Go keeps what it needs at runtime (pclntab, build info) outside those sections.
# Usage: strip-tailscale.sh DIR   (DIR holds `tailscale` and `tailscaled`; writes DIR/SHA256SUMS)
# Strip tool: $NIBE_STRIP, else llvm-strip, else arm-linux-gnueabihf-strip. Needs readelf.
set -euo pipefail
[[ $# == 1 && -d $1 ]] || { echo 'usage: strip-tailscale.sh DIR' >&2; exit 2; }
dir=$1
tool=${NIBE_STRIP:-}
if [[ -z $tool ]]; then
    for candidate in llvm-strip arm-linux-gnueabihf-strip; do
        command -v "$candidate" >/dev/null && { tool=$candidate; break; }
    done
fi
[[ -n $tool ]] || { echo 'Need llvm-strip or arm-linux-gnueabihf-strip (or set NIBE_STRIP) to strip Tailscale' >&2; exit 1; }
for name in tailscale tailscaled; do
    file=$dir/$name
    [[ -f $file ]] || { echo "missing $file" >&2; exit 1; }
    header=$(LC_ALL=C readelf -h "$file") || { echo "$file is not an ELF file" >&2; exit 1; }
    grep -Eq 'Machine:[[:space:]]+ARM[[:space:]]*$' <<< "$header" || { echo "$file is not an ARM binary" >&2; exit 1; }
    before=$(stat -c %s "$file")
    "$tool" --strip-all -o "$file.stripped" "$file"
    if LC_ALL=C readelf -S -W "$file.stripped" | grep -Eq '\.(debug_|zdebug_|symtab)'; then
        rm -f "$file.stripped"; echo "$file still has debug or symbol sections after $tool" >&2; exit 1
    fi
    chmod --reference="$file" "$file.stripped"
    mv "$file.stripped" "$file"
    echo "stripped $name: $before -> $(stat -c %s "$file") bytes" >&2
done
(cd "$dir" && sha256sum tailscale tailscaled > SHA256SUMS)
cat "$dir/SHA256SUMS" >&2
