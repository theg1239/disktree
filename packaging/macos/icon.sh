#!/bin/sh
# Convert the existing SVG mark with macOS tools; no graphics dependencies.
set -eu
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
mkdir "$scratch/disktree.iconset"
sips -s format png -z 1024 1024 assets/disktree.svg --out "$scratch/icon.png" >/dev/null
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$scratch/icon.png" --out "$scratch/disktree.iconset/icon_${size}x${size}.png" >/dev/null
    doubled=$((size * 2))
    sips -z "$doubled" "$doubled" "$scratch/icon.png" --out "$scratch/disktree.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$scratch/disktree.iconset" -o assets/disktree.icns
