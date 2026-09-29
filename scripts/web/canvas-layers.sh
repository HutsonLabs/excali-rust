#!/bin/sh
# The layered canvases in the browser (ex-503): excali_ui::layers mounted in
# Chromium at several device pixel ratios, their backing sizes, order,
# stylesheet rules and pixels checked by Playwright.
#
#   scripts/web/canvas-layers.sh
#
# 1. Builds the wasm harness tools/canvas-layers (CanvasLayers, the static
#    and new-element scenes) for wasm32-unknown-unknown and runs
#    wasm-bindgen --target web into CANVAS_LAYERS_HARNESS.
# 2. Runs the Playwright suite tests/web/layers
#    (playwright.layers.config.mjs).
#
# CI runs this on every PR (rust.yml, job canvas-layers).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: CANVAS_LAYERS_HARNESS (default target/canvas-layers-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${CANVAS_LAYERS_HARNESS:-$root/target/canvas-layers-harness}"
manifest="$root/tools/canvas-layers/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/canvas-layers/target}"

die() {
  echo "canvas layers: $*" >&2
  exit 1
}

want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
[ -n "$want" ] || die "wasm-bindgen is not in Cargo.lock"
command -v wasm-bindgen >/dev/null 2>&1 ||
  die "wasm-bindgen not found; cargo install --locked wasm-bindgen-cli --version $want"
have="$(wasm-bindgen --version | awk '{ print $2 }')"
[ "$have" = "$want" ] ||
  die "wasm-bindgen $have, Cargo.lock pins $want; cargo install --locked wasm-bindgen-cli --version $want"
[ -x "$root/tests/web/node_modules/.bin/playwright" ] ||
  die "Playwright missing; npm ci --prefix tests/web && (cd tests/web && npx playwright install chromium)"

cargo build --locked --release --target wasm32-unknown-unknown --manifest-path "$manifest"
rm -rf "$harness"
mkdir -p "$harness"
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name canvas_layers \
  "$target_dir/wasm32-unknown-unknown/release/canvas_layers.wasm"

(
  cd "$root/tests/web"
  CANVAS_LAYERS_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.layers.config.mjs
)
echo "canvas layers: backing sizes, stacking and snapped painting hold in Chromium"
