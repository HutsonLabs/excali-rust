#!/usr/bin/env bash
# Raster references (ex-401): draw every fixture display list in
# crates/excali-raster/tests/fixtures/display-lists/ on a real canvas in
# headless Chrome (scripts/fixtures/raster_references.html) and keep the
# pixels as crates/excali-raster/tests/fixtures/chrome/<name>.png, with a
# manifest.json naming the Chrome and each fixture's SHA-256.
#
#   scripts/fixtures/raster-references.sh           regenerate the references
#   scripts/fixtures/raster-references.sh --check   draw them with this machine's
#                                                   Chrome into a temporary
#                                                   directory and run the Rust
#                                                   pixel diff against those
#
# Upstream draws with the Canvas 2D API, so Chrome's canvas is what the
# tiny-skia backend (excali-raster) is compared with; the comparison and each
# fixture's tolerance live in crates/excali-raster/tests/fixtures.rs.
# --check does not compare PNG bytes: another Chrome release may round an
# edge differently, and the question is whether the port still matches the
# browser within the tolerances the fixtures state.
#
# Environment:
#   CHROME   Chrome or Chromium binary; default the macOS application, then
#            google-chrome, google-chrome-stable, chromium on PATH
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
lists="$root/crates/excali-raster/tests/fixtures/display-lists"
refs="$root/crates/excali-raster/tests/fixtures/chrome"

die() { printf 'raster references: %s\n' "$*" >&2; exit 1; }

mode=write
case "${1:-}" in
  "") ;;
  --check) mode=check ;;
  -h | --help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

chrome="${CHROME:-}"
if [ -z "$chrome" ]; then
  for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" google-chrome google-chrome-stable chromium; do
    if command -v "$c" > /dev/null 2>&1 || [ -x "$c" ]; then chrome="$c"; break; fi
  done
fi
[ -n "$chrome" ] || die "no Chrome found (set CHROME)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# The page with the fixtures inlined (file:// pages cannot fetch).
python3 - "$lists" "$here/raster_references.html" "$tmp/page.html" <<'PY'
import json, pathlib, sys
lists, template, page = map(pathlib.Path, sys.argv[1:])
fixtures = {p.stem: json.loads(p.read_text(encoding="utf-8")) for p in sorted(lists.glob("*.json"))}
if not fixtures:
    sys.exit(f"raster references: no fixtures in {lists}")
html = template.read_text(encoding="utf-8")
marker = "/*FIXTURES*/null"
if html.count(marker) != 1:
    sys.exit("raster references: the page has no single FIXTURES marker")
page.write_text(html.replace(marker, json.dumps(fixtures).replace("</", "<\\/")), encoding="utf-8")
PY
# The drawing functions the page loads from beside it.
cp "$here/raster_references.js" "$tmp/raster_references.js"

# Software rasterisation (no GPU canvas) in sRGB, as the export path draws.
# On Linux CI runners (Ubuntu 24.04 restricts the unprivileged user
# namespaces Chrome's sandbox needs) the page, which is ours and local, runs
# unsandboxed.
flags=(--headless=new --disable-gpu --force-color-profile=srgb
  --no-first-run --no-default-browser-check --disable-extensions)
if [ "$(uname -s)" = Linux ]; then flags+=(--no-sandbox); fi
"$chrome" "${flags[@]}" --dump-dom "file://$tmp/page.html" 2> /dev/null > "$tmp/dom.html"

out="$tmp/references"
mkdir -p "$out"
python3 - "$tmp/dom.html" "$lists" "$out" <<'PY'
import base64, hashlib, html, json, pathlib, re, sys
dom = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
lists, out = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
m = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
if not m or not m.group(1).strip():
    sys.exit("raster references: the page wrote no results")
data = json.loads(html.unescape(m.group(1)))
if data["errors"]:
    for name, err in data["errors"].items():
        print(f"raster references: {name}: {err}", file=sys.stderr)
    sys.exit(1)
names = sorted(p.stem for p in lists.glob("*.json"))
if sorted(data["png"]) != names:
    sys.exit("raster references: the page drew a different set of fixtures")
prefix = "data:image/png;base64,"
for name in names:
    url = data["png"][name]
    if not url.startswith(prefix):
        sys.exit(f"raster references: {name}: not a PNG data URL")
    (out / f"{name}.png").write_bytes(base64.b64decode(url[len(prefix):]))
manifest = {
    "userAgent": data["userAgent"],
    "fixtures": {
        n: hashlib.sha256((lists / f"{n}.json").read_bytes()).hexdigest() for n in names
    },
}
(out / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
print(f"raster references: {len(names)} fixtures drawn by {data['userAgent']}", file=sys.stderr)
PY

if [ "$mode" = write ]; then
  mkdir -p "$refs"
  rm -f "$refs"/*.png "$refs/manifest.json"
  cp "$out"/*.png "$out/manifest.json" "$refs/"
  echo "wrote ${refs#"$root"/}" >&2
  exit 0
fi

export PATH="$HOME/.cargo/bin:$PATH"
EXCALI_RASTER_REFERENCES="$out" cargo test --locked --manifest-path "$root/Cargo.toml" \
  -p excali-raster --test fixtures -- --nocapture
