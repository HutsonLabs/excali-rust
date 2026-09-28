#!/usr/bin/env bash
# Text width causes (ex-g303): run upstream's own restore at the pinned commit,
# and upstream's getLineWidth of 62228e0b, in Chrome on every known width
# deviation of crates/excali-text/tests/text_width_corpus.rs, and write
# crates/excali-text/tests/fixtures/text-width-causes.json
# (scripts/fixtures/text_width_causes.mjs has the details).
#
#   scripts/fixtures/text-width-causes.sh           regenerate the fixture
#   scripts/fixtures/text-width-causes.sh --check   exit 1 if this machine's
#                                                   Chrome moves a number by
#                                                   more than 0.001 px or
#                                                   changes anything else
#
# Needs the upstream checkout at the pin (scripts/upstream/checkout.sh),
# tools/goldens installed (esbuild, which bundles upstream's sources) and
# tests/web installed with Playwright's Chromium (npx playwright install
# chromium in tests/web). The committed fixture was measured on macOS arm64:
# Chrome on Linux rounds advances to whole pixels, so --check is meant for
# macOS.
#
# Environment:
#   NODE   node binary; default node
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
node="${NODE:-node}"

die() { printf 'text width causes: %s\n' "$*" >&2; exit 1; }

case "${1:-}" in
  "" | --check) ;;
  -h | --help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

[ -d "$root/tools/goldens/node_modules/esbuild" ] ||
  die "tools/goldens is not installed (npm ci --prefix tools/goldens)"
[ -d "$root/tests/web/node_modules/@playwright/test" ] ||
  die "tests/web is not installed (npm ci --prefix tests/web, then npx playwright install chromium there)"

exec "$node" "$here/text_width_causes.mjs" "$@"
