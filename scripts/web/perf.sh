#!/bin/sh
# The performance budgets of the web runtime (ex-710): first paint after
# module load and a pan frame at 1,000 elements, against the budgets on
# site/content/plan/phases.md (## Budgets).
#
#   scripts/web/perf.sh [--no-build]
#
# 1. Builds the release into dist/ (scripts/web/build.sh) unless --no-build.
# 2. Runs the Playwright suite tests/web/perf (playwright.perf.config.mjs),
#    which writes the measurements and their calibrations to PERF_RESULTS.
# 3. Holds them, normalised by the calibrations, to the budgets:
#    scripts/gates/perf_budget.py.
#
# CI runs this on the pinned self-hosted runner (rust.yml, job web-perf), with
# --no-build after a build step of its own.
#
# Needs: what scripts/web/build.sh needs, and `npm ci` plus
# `npx playwright install chromium` in tests/web.
#
# Environment: PERF_RESULTS (default
# tests/web/test-results-perf/perf.json).
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
results="${PERF_RESULTS:-$root/tests/web/test-results-perf/perf.json}"

die() {
  echo "perf: $*" >&2
  exit 1
}

case "${1:-}" in
  "") scripts/web/build.sh ;;
  --no-build) ;;
  *) die "usage: scripts/web/perf.sh [--no-build]" ;;
esac
[ -x "$root/tests/web/node_modules/.bin/playwright" ] ||
  die "Playwright missing; npm ci --prefix tests/web && (cd tests/web && npx playwright install chromium)"

rm -f "$results"
(
  cd tests/web
  PERF_RESULTS="$results" npx playwright test -c playwright.perf.config.mjs
)
python3 scripts/gates/perf_budget.py check "$results"
