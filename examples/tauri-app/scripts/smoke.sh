#!/bin/sh
# The example app end to end in the platform webview (ex-606): builds it as
# `cargo tauri build --no-bundle` does (beforeBuildCommand, then cargo with
# tauri's custom-protocol feature: the embedded ui/ with the web runtime,
# served under the CSP), without needing tauri-cli, then runs it in smoke mode
# (EXCALI_EXAMPLE_SMOKE, src-tauri/src/lib.rs): ui/app.js mounts
# <excali-editor>, opens smoke/open.excalidraw, saves it back, saves it as,
# and exports PNG and SVG through tauri-plugin-excali, then reports;
# check-smoke.py checks the report and the files written.
#
#   scripts/smoke.sh [--release]     EXCALI_WEB_DIST as for web-runtime.sh
#
# Linux needs a display (CI runs it under xvfb-run).
set -eu
app="$(cd "$(dirname "$0")/.." && pwd)"
profile=debug
if [ "${1:-}" = "--release" ]; then
  profile=release
fi
target="${CARGO_TARGET_DIR:-$app/src-tauri/target}"
sh "$app/scripts/web-runtime.sh"
cargo build --locked --manifest-path "$app/src-tauri/Cargo.toml" --features tauri/custom-protocol \
  $([ "$profile" = release ] && echo --release)
dir="$(mktemp -d "${TMPDIR:-/tmp}/excali-smoke.XXXXXX")"
trap 'rm -rf "$dir"' EXIT
cp "$app/smoke/open.excalidraw" "$dir/open.excalidraw"
EXCALI_EXAMPLE_SMOKE="$dir" "$target/$profile/excali-tauri-example"
python3 "$app/scripts/check-smoke.py" "$dir" "$app/smoke/open.excalidraw"
