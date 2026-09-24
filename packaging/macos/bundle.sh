#!/bin/sh
# Run from the repository root, after building for the desired architecture.
set -eu
binary=${1:-target/release/disktree}
bundle=${2:-target/release/Disktree.app}
identity=${CODESIGN_IDENTITY:--}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ "$(uname -s)" = Darwin ] || { echo 'Bundling requires macOS.' >&2; exit 1; }
[ -f "$binary" ] || { echo "Missing binary: $binary" >&2; exit 1; }
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
install -m755 "$binary" "$bundle/Contents/MacOS/disktree"
install -m644 assets/disktree.icns "$bundle/Contents/Resources/disktree.icns"
install -m644 LICENSE "$bundle/Contents/Resources/LICENSE"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$bundle/Contents/Info.plist"
printf 'APPL????' > "$bundle/Contents/PkgInfo"
plutil -lint "$bundle/Contents/Info.plist"
# '-' is a local ad-hoc signature. A Developer ID may be supplied for a
# distribution build; notarization remains a separate credentialed step.
if [ "$identity" = - ]; then
    codesign --force --sign - "$bundle"
else
    codesign --force --options runtime --timestamp --sign "$identity" "$bundle"
fi
codesign --verify --deep --strict "$bundle"
echo "Built $bundle"
