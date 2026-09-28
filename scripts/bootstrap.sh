#!/usr/bin/env bash
# One-shot, idempotent developer/agent setup for this repository.
# Re-run any time; every step checks before it changes anything.
#
#   1. git identity   -> the responsible human, never a tool
#   2. hooks path     -> .githooks (authorship gate + beads export)
#   3. beads (bd)     -> task tracker, metrics off, JSONL export on
#   4. zola           -> pinned static-site generator for site/
#
# Overrides: EXCALI_GIT_NAME, EXCALI_GIT_EMAIL, EXCALI_SKIP_ZOLA=1
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
cd "$root"

name="${EXCALI_GIT_NAME:-Dr. Hutson}"
email="${EXCALI_GIT_EMAIL:-hutsona@me.com}"

step() { printf '\n== %s\n' "$*"; }

step "git identity (repo-local)"
git config user.name "$name"
git config user.email "$email"
# Commit signing in hosted agent containers uses a key registered to the tool
# vendor; a signature is an identity marker, so it is disabled here.
git config commit.gpgsign false
git config --local --get-regexp '^user\.' | sed 's/^/   /'

step "hooks"
git config core.hooksPath .githooks
chmod +x .githooks/* scripts/gates/attribution.py
echo "   core.hooksPath = $(git config core.hooksPath)"

step "beads"
if ! command -v bd >/dev/null 2>&1; then
  if command -v npm >/dev/null 2>&1; then
    npm install -g @beads/bd >/dev/null
  else
    echo "   bd not found and npm unavailable; install per https://github.com/steveyegge/beads" >&2
  fi
fi
if command -v bd >/dev/null 2>&1; then
  bd metrics off >/dev/null 2>&1 || true
  export BEADS_ACTOR="$name"
  if [ ! -d .beads/embeddeddolt ] && [ ! -d .beads/dolt ]; then
    # Fresh clone: rebuild the local database from the tracked JSONL.
    bd bootstrap --yes >/dev/null
  fi
  bd config set export.auto true >/dev/null 2>&1 || true
  echo "   $(bd version | head -1)"
  bd status 2>/dev/null | head -5 | sed 's/^/   /' || true
fi

step "zola"
if [ -z "${EXCALI_SKIP_ZOLA:-}" ]; then
  "$root/scripts/site/zola.sh" ensure
fi

step "gate self-check"
python3 scripts/gates/attribution.py files --all
echo
echo "bootstrap complete"
