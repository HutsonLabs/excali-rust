#!/bin/sh
# The app icons (src-tauri/icons), drawn by the port itself: icon-scene.py
# writes icon.excalidraw, `excali render` exports it as a 1024 x 1024 PNG
# with a transparent background, and `cargo tauri icon` derives the sizes
# tauri.conf.json lists. Rerun after changing the scene.
set -eu
app="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$app/../.." && pwd)"
icons="$app/src-tauri/icons"
python3 "$app/scripts/icon-scene.py"
stage="$(mktemp -d "${TMPDIR:-/tmp}/excali-icons.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
cargo run --quiet --locked --manifest-path "$root/Cargo.toml" -p excali-cli -- \
  render "$icons/icon.excalidraw" --padding 12 --no-background -o "$stage/icon.png"
(cd "$app/src-tauri" && cargo tauri icon "$stage/icon.png" -o "$stage/out" >/dev/null 2>&1)
for f in 32x32.png 128x128.png 128x128@2x.png icon.icns icon.ico icon.png; do
  cp "$stage/out/$f" "$icons/$f"
done
echo "icons: $icons"
