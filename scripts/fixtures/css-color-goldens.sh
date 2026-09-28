#!/usr/bin/env bash
# CSS colour goldens (ex-216): load scripts/fixtures/css_color_goldens.html in
# headless Chrome and write what its canvas made of every colour string to
# crates/excali-scene/tests/fixtures/css-colors.json.
#
#   scripts/fixtures/css-color-goldens.sh           regenerate the goldens file
#   scripts/fixtures/css-color-goldens.sh --check   exit 1 if regenerating would
#                                                   change the cases or results
#
# Upstream assigns element colours to fillStyle/strokeStyle verbatim, so the
# browser's CSS parser is the specification the port's display-list colour
# parser (crates/excali-scene/src/display/css_color.rs) is tested against.
# --check compares everything but the user-agent string, so another Chrome
# release that paints the same passes.
#
# Environment:
#   CHROME   Chrome or Chromium binary; default the macOS application, then
#            google-chrome, chromium on PATH
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
out="$root/crates/excali-scene/tests/fixtures/css-colors.json"

die() { printf 'css colour goldens: %s\n' "$*" >&2; exit 1; }

mode=write
case "${1:-}" in
  "") ;;
  --check) mode=check ;;
  -h | --help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

chrome="${CHROME:-}"
if [ -z "$chrome" ]; then
  for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" google-chrome chromium; do
    if command -v "$c" > /dev/null 2>&1 || [ -x "$c" ]; then chrome="$c"; break; fi
  done
fi
[ -n "$chrome" ] || die "no Chrome found (set CHROME)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$chrome" --headless=new --disable-gpu \
  --dump-dom "file://$here/css_color_goldens.html" 2> /dev/null > "$tmp/dom.html"

python3 - "$tmp/dom.html" "$tmp/goldens.json" <<'PY'
import html, json, re, sys
dom = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r'<pre id="out">(.*?)</pre>', dom, re.S)
if not m or not m.group(1).strip():
    sys.exit("css colour goldens: the page wrote no results")
data = json.loads(html.unescape(m.group(1)))
with open(sys.argv[2], "w", encoding="utf-8") as f:
    json.dump(data, f, indent=1, ensure_ascii=True)
    f.write("\n")
PY

if [ "$mode" = write ]; then
  mkdir -p "$(dirname "$out")"
  mv "$tmp/goldens.json" "$out"
  echo "wrote ${out#"$root"/}" >&2
  exit 0
fi

python3 - "$out" "$tmp/goldens.json" <<'PY'
import json, sys
old, new = (json.load(open(p, encoding="utf-8")) for p in sys.argv[1:])
for d in (old, new):
    d.pop("userAgent", None)
if old != new:
    sys.exit("css colour goldens: crates/excali-scene/tests/fixtures/css-colors.json is stale: run scripts/fixtures/css-color-goldens.sh")
print("css colour goldens match Chrome", file=sys.stderr)
PY
