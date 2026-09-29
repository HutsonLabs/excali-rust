#!/usr/bin/env bash
# Chrome PNG export references (ex-g402): run upstream's own exportToCanvas
# at the pinned commit in Playwright's Chromium, with the vendored fonts, on
# every fixture scene, and keep the PNGs in
# crates/excali-cli/tests/fixtures/chrome-export/ with a manifest.json
# (scripts/fixtures/chrome_png_export.mjs has the details).
#
#   scripts/fixtures/chrome-png-export.sh           regenerate the references
#   scripts/fixtures/chrome-png-export.sh --check   re-draw them with this
#                                                   machine's Chromium into a
#                                                   temporary directory, require
#                                                   the written scene files to be
#                                                   the committed ones, and run
#                                                   the Rust comparison against
#                                                   those PNGs
#
# The comparison is crates/excali-cli/tests/chrome_export.rs: the port's
# full PNG export of each scene (text drawn by the CLI's GlyphText, images
# decoded) must have Chrome's canvas size and be within the scene's
# tolerance in tolerances.json. --check does not compare PNG bytes: the
# question is whether the port still matches the browser within the
# tolerances. A failing scene writes the port's PNG, Chrome's and a diff
# image to target/chrome-export-diff/.
#
# Needs the upstream checkout at the pin (scripts/upstream/checkout.sh),
# tools/goldens installed (esbuild bundles upstream's sources) and tests/web
# installed with Playwright's Chromium (npx playwright install chromium in
# tests/web). The tolerances were measured on macOS arm64, where CI runs
# --check (rust.yml, job chrome-png-export).
#
# Environment:
#   NODE    node binary; default node
#   CARGO   cargo binary; default cargo
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
node="${NODE:-node}"
cargo="${CARGO:-cargo}"
refs="$root/crates/excali-cli/tests/fixtures/chrome-export"

die() { printf 'chrome png export: %s\n' "$*" >&2; exit 1; }

mode=write
case "${1:-}" in
  "") ;;
  --check) mode=check ;;
  -h | --help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

[ -d "$root/tools/goldens/node_modules/esbuild" ] ||
  die "tools/goldens is not installed (npm ci --prefix tools/goldens)"
[ -d "$root/tests/web/node_modules/@playwright/test" ] ||
  die "tests/web is not installed (npm ci --prefix tests/web, then npx playwright install chromium there)"

if [ "$mode" = write ]; then
  # tolerances.json and README.md are written by hand: keep them
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  "$node" "$here/chrome_png_export.mjs" --out "$tmp/out"
  for kept in tolerances.json README.md; do
    if [ -f "$refs/$kept" ]; then cp "$refs/$kept" "$tmp/out/"; fi
  done
  rm -rf "$refs"
  mkdir -p "$(dirname "$refs")"
  cp -R "$tmp/out" "$refs"
  echo "wrote ${refs#"$root"/}" >&2
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$node" "$here/chrome_png_export.mjs" --out "$tmp/out"
diff -r "$refs/scenes" "$tmp/out/scenes" > /dev/null ||
  die "the element fixture scene files are stale: run scripts/fixtures/chrome-png-export.sh"
EXCALI_CHROME_EXPORT_REFERENCES="$tmp/out" "$cargo" test --locked \
  --manifest-path "$root/Cargo.toml" -p excali-cli --test chrome_export -- --nocapture
