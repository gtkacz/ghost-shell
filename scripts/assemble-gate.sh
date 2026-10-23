#!/usr/bin/env bash
# Writes <out_dir>/GhostGate/: the gate's own files plus copies of the GhostKit files it shares. Copies, not
# symlinks, because the lock and login greeters run as other users and must find a self-contained directory.
set -euo pipefail

# Relative to shell/kit/GhostKit. Shaders are the compiled files, so `make shaders` must have run.
SHARED_FILES=(
    Panel.qml Chip.qml Glyph.qml
    lib/theme.mjs lib/scale.mjs lib/views.mjs lib/attention.mjs
    shaders/dotgrid.frag.qsb shaders/scanlines.frag.qsb shaders/stripes.frag.qsb shaders/blink.frag.qsb
)

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
out=${1:?usage: assemble-gate.sh <out_dir>}
dest=$out/GhostGate

rm -rf -- "$dest"
mkdir -p -- "$dest"
# -L turns the gate's lib/attention.mjs link into a real file; the shared copy below overwrites it with the same bytes.
cp -rL -- "$root/shell/gate/GhostGate/." "$dest/"
for file in "${SHARED_FILES[@]}"; do
    mkdir -p -- "$dest/$(dirname -- "$file")"
    cp -- "$root/shell/kit/GhostKit/$file" "$dest/$file"
done
