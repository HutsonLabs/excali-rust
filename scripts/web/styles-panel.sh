#!/bin/sh
# The full styles panel in the browser (ex-519): excali_ui::styles_panel
# (SelectedShapeActions inside LayerUI's section and island) mounted with
# web-sys in Chromium and held to upstream's tree for every case of
# crates/excali-ui/tests/fixtures/styles-panel.json.
#
#   scripts/web/styles-panel.sh
#
# 1. Builds the wasm harness tools/styles-panel for wasm32-unknown-unknown
#    and runs wasm-bindgen --target web into STYLES_PANEL_HARNESS.
# 2. Runs the Playwright suite tests/web/styles-panel
#    (playwright.styles-panel.config.mjs).
#
# CI runs this on every PR (rust.yml, job styles-panel).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: STYLES_PANEL_HARNESS (default target/styles-panel-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${STYLES_PANEL_HARNESS:-$root/target/styles-panel-harness}"
manifest="$root/tools/styles-panel/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/styles-panel/target}"

die() {
  echo "styles-panel: $*" >&2
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
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name styles_panel \
  "$target_dir/wasm32-unknown-unknown/release/styles_panel.wasm"

(
  cd "$root/tests/web"
  STYLES_PANEL_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.styles-panel.config.mjs
)
echo "styles-panel: every case mounts upstream's tree in Chromium"
