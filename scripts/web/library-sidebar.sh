#!/bin/sh
# The library sidebar in the browser (ex-526): excali_ui::library_sidebar's
# default sidebar and LayerUI's library trigger mounted with web-sys in
# Chromium, with the sidebar's stylesheets: its size docked (302 px column),
# its tabs, search, selection, insertion, drag data, header menu, docking
# and Escape, each event run through excali_ui::library_sidebar::update.
#
#   scripts/web/library-sidebar.sh
#
# 1. Builds the wasm harness tools/library-sidebar for wasm32-unknown-unknown
#    and runs wasm-bindgen --target web into LIBRARY_SIDEBAR_HARNESS.
# 2. Runs the Playwright suite tests/web/library-sidebar
#    (playwright.library-sidebar.config.mjs).
#
# CI runs this on every PR (rust.yml, job library-sidebar).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: LIBRARY_SIDEBAR_HARNESS (default target/library-sidebar-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${LIBRARY_SIDEBAR_HARNESS:-$root/target/library-sidebar-harness}"
manifest="$root/tools/library-sidebar/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/library-sidebar/target}"

die() {
  echo "library-sidebar: $*" >&2
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
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name library_sidebar \
  "$target_dir/wasm32-unknown-unknown/release/library_sidebar.wasm"

(
  cd "$root/tests/web"
  LIBRARY_SIDEBAR_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.library-sidebar.config.mjs
)
echo "library-sidebar: the sidebar's size and behaviour hold in Chromium"
