#!/bin/sh
# The Canvas 2D backend against the fixture display lists (ex-502):
# excali-canvas2d paints every list of
# crates/excali-raster/tests/fixtures/display-lists/ in Chromium, and
# excali-raster must render each within the fixture's tolerance of those
# pixels.
#
#   scripts/web/canvas2d-fixtures.sh
#
# 1. Builds the wasm harness tools/canvas2d-fixtures (the Rust fixture
#    loader, excali_canvas2d::paint_scaled, WebCanvas) for
#    wasm32-unknown-unknown and runs wasm-bindgen --target web into
#    CANVAS2D_HARNESS, with scripts/fixtures/raster_references.js beside it.
# 2. Runs the Playwright suite tests/web/canvas2d
#    (playwright.canvas2d.config.mjs): each list painted by the backend is
#    held to its tolerance against the independent canvas reading in the
#    same page, and kept as CANVAS2D_OUT/<name>.png with a manifest.json.
# 3. Runs excali-raster's fixture test with
#    EXCALI_RASTER_REFERENCES=CANVAS2D_OUT: the tiny-skia render of every
#    list against excali-canvas2d's pixels, under each fixture's tolerance
#    (a failure writes the rendered and diff images to target/raster-diff/).
#
# Tolerances were measured on macOS arm64 (see the raster-chrome CI job);
# CI runs this there (rust.yml, job canvas2d-fixtures).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: CANVAS2D_HARNESS (default target/canvas2d-harness),
# CANVAS2D_OUT (default target/canvas2d-fixtures), CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${CANVAS2D_HARNESS:-$root/target/canvas2d-harness}"
out="${CANVAS2D_OUT:-$root/target/canvas2d-fixtures}"
manifest="$root/tools/canvas2d-fixtures/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/canvas2d-fixtures/target}"

die() {
  echo "canvas2d fixtures: $*" >&2
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
rm -rf "$harness" "$out"
mkdir -p "$harness" "$out"
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name excali_canvas2d_fixtures \
  "$target_dir/wasm32-unknown-unknown/release/canvas2d_fixtures.wasm"
cp "$root/scripts/fixtures/raster_references.js" "$harness/raster_references.js"

(
  cd "$root/tests/web"
  CANVAS2D_HARNESS="$harness" CANVAS2D_OUT="$out" \
    ./node_modules/.bin/playwright test --config playwright.canvas2d.config.mjs
)

EXCALI_RASTER_REFERENCES="$out" cargo test --locked -p excali-raster --test fixtures -- --nocapture
echo "canvas2d fixtures: excali-canvas2d within tolerance of the independent reading and of excali-raster ($out)"
