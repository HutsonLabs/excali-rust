#!/bin/sh
# Build every crate whose Targets cell in site/content/architecture/overview.md
# includes wasm32 for wasm32-unknown-unknown (ADR-008). Run by rust.yml and
# locally; the crate list comes from scripts/gates/workspace.py so the page
# stays the single source of truth.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
set --
for crate in $(python3 scripts/gates/workspace.py wasm-crates); do
  set -- "$@" -p "$crate"
done
if [ "$#" -eq 0 ]; then
  echo "wasm-build: no wasm32 crates listed in the architecture overview" >&2
  exit 1
fi
exec cargo build --locked --target wasm32-unknown-unknown "$@"
