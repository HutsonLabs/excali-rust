#!/bin/sh
# ex-408: can each candidate go where excali-svg goes (wasm32-unknown-unknown,
# site/content/architecture/overview.md), and at what size?
#
#   tools/font-subset-eval/wasm.sh          write tools/font-subset-eval/wasm.json
#   tools/font-subset-eval/wasm.sh --check  exit 1 if wasm.json differs
#
# Builds this package's cdylib (src/probe.rs) for wasm32-unknown-unknown in
# release with no candidate, then with each candidate's features alone and
# each Rust subsetter with the Rust encoder, and records whether it builds
# and the module's bytes, raw and gzipped (level 9). Paths are remapped so
# panic locations do not depend on the checkout or CARGO_HOME. --check
# requires the same builds to succeed and each size within 5 % of the
# recorded one (toolchain point releases move sizes slightly); a build
# failure's reason is printed, not recorded, since the C++ toolchain's
# message differs by platform.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"
check=0
case "${1:-}" in
  "") ;;
  --check) check=1 ;;
  *) echo "usage: wasm.sh [--check]" >&2; exit 2 ;;
esac
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$here=/eval"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
module="$here/target/wasm32-unknown-unknown/release/font_subset_eval.wasm"
for features in none hb-subset skera allsorts subsetter woofwoof ttf2woff2 skera,ttf2woff2 allsorts,ttf2woff2 subsetter,ttf2woff2; do
  if [ "$features" = none ]; then set -- ; else set -- --features "$features"; fi
  rm -f "$module"
  if cargo build --locked --release --lib --target wasm32-unknown-unknown --no-default-features "$@" \
    >"$out/log" 2>&1 && [ -f "$module" ]; then
    cp "$module" "$out/$features.wasm"
    echo "$features: builds, $(wc -c <"$module" | tr -d ' ') bytes"
  else
    : >"$out/$features.failed"
    reason="$(grep -E 'fatal error|^error' "$out/log" | head -n 1 || true)"
    echo "$features: does not build: $reason"
  fi
done
python3 - "$out" "$here/wasm.json" "$check" <<'PY'
import gzip, json, sys
from pathlib import Path

out, path, check = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3] == "1"
order = ["none", "hb-subset", "skera", "allsorts", "subsetter", "woofwoof", "ttf2woff2", "skera,ttf2woff2", "allsorts,ttf2woff2", "subsetter,ttf2woff2"]
builds = []
for features in order:
    wasm = out / f"{features}.wasm"
    if wasm.exists():
        data = wasm.read_bytes()
        builds.append({"features": features, "builds": True, "bytes": len(data),
                       "gzip": len(gzip.compress(data, compresslevel=9, mtime=0))})
    else:
        builds.append({"features": features, "builds": False, "bytes": None, "gzip": None})
report = {
    "description": "ex-408: tools/font-subset-eval's cdylib (src/probe.rs) built with cargo build --release --target wasm32-unknown-unknown --no-default-features --features <features> (none: the probe alone); builds, and the module's bytes raw and gzipped at level 9. Written by tools/font-subset-eval/wasm.sh.",
    "builds": builds,
}
text = json.dumps(report, indent=2) + "\n"
if not check:
    path.write_text(text)
    print(f"wrote {path.name}")
    sys.exit(0)
old = json.loads(path.read_text()) if path.exists() else {"builds": []}
bad = []
by = {b["features"]: b for b in old["builds"]}
for b in builds:
    o = by.get(b["features"])
    if o is None or o["builds"] != b["builds"]:
        bad.append(f"{b['features']}: builds {b['builds']}, recorded {o and o['builds']}")
        continue
    for key in ("bytes", "gzip"):
        if b[key] is not None and abs(b[key] - o[key]) > o[key] * 0.05:
            bad.append(f"{b['features']}: {key} {b[key]}, recorded {o[key]} (more than 5 % apart)")
if bad or len(builds) != len(old["builds"]):
    print("stale: tools/font-subset-eval/wasm.json; run tools/font-subset-eval/wasm.sh", *bad, sep="\n  ", file=sys.stderr)
    sys.exit(1)
print("wasm.json is current")
PY
