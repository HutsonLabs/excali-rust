#!/bin/sh
# The UI primitives in the browser (ex-516): excali_ui::primitives mounted
# in Chromium with their stylesheet, their computed sizes measured and
# their behaviour (clicks, keys, hover, focus) driven by Playwright; the
# theme tokens (ex-532) computed light and dark, and overridden by a host.
#
#   scripts/web/ui-primitives.sh
#
# 1. Builds the wasm harness tools/ui-primitives (a gallery of the
#    primitives, a dialog and a popover) for wasm32-unknown-unknown and runs
#    wasm-bindgen --target web into UI_PRIMITIVES_HARNESS.
# 2. Runs the Playwright suite tests/web/primitives
#    (playwright.primitives.config.mjs).
#
# CI runs this on every PR (rust.yml, job ui-primitives).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: UI_PRIMITIVES_HARNESS (default target/ui-primitives-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${UI_PRIMITIVES_HARNESS:-$root/target/ui-primitives-harness}"
manifest="$root/tools/ui-primitives/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/ui-primitives/target}"

die() {
  echo "ui primitives: $*" >&2
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
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name ui_primitives \
  "$target_dir/wasm32-unknown-unknown/release/ui_primitives.wasm"

(
  cd "$root/tests/web"
  UI_PRIMITIVES_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.primitives.config.mjs
)
echo "ui primitives: sizes, classes and behaviour hold in Chromium"
