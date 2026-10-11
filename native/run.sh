#!/bin/sh
set -eu

if [ "$(uname -s)" != Darwin ]; then
  echo "This iteration targets macOS. Linux/Windows packaging is not configured yet." >&2
  exit 1
fi

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
profile=debug
case "${1:-}" in
  "") cargo build --locked --manifest-path "$root/Cargo.toml" ;;
  --demo) cargo build --locked --manifest-path "$root/Cargo.toml" ;;
  --release) profile=release; cargo build --locked --release --manifest-path "$root/Cargo.toml" ;;
  *) echo "Usage: $0 [--release | --demo]" >&2; exit 1 ;;
esac

target=${CARGO_TARGET_DIR:-"$root/target"}
bundle="$target/$profile/SPEED Native.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target/$profile/speed-native" "$bundle/Contents/MacOS/speed-native"
cp "$root/Info.plist" "$bundle/Contents/Info.plist"
cp "$root/assets/Speed.icns" "$bundle/Contents/Resources/Speed.icns"
codesign --force --sign - "$bundle"
echo "Opening $bundle"
if [ "${1:-}" = --demo ]; then
  open -n "$bundle" --args --demo
else
  open "$bundle"
fi
