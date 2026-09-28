#!/usr/bin/env python3
"""Generate excali-text's font face table from upstream's font registry (ex-302).

Upstream registers every family's faces in Fonts.init()
(packages/excalidraw/fonts/Fonts.ts:375-416) from the descriptor arrays in
packages/excalidraw/fonts/<Dir>/index.ts, and the UI family Assistant from
packages/excalidraw/fonts/fonts.css. Each face is a file (or `local:`) with an
optional CSS unicode-range and weight. This script reads those sources at the
pinned commit and writes crates/excali-text/src/font_faces_table.rs, the
`FONT_FACES` table the measurement code resolves characters through.

Every file face maps to the file the port vendors under crates/excali-text/assets/fonts/<Dir>/ (ex-307): the
same file name, except upstream's Liberation Sans 1.05 file, which ADR-004
lists as a licence gap and replaces with the OFL build 2.1.5.

Subcommands:
  write   regenerate the table from the upstream checkout
  check   exit 1 if the committed table differs from a fresh generation, or
          a vendored file the table names is missing from crates/excali-text/assets/fonts/

The upstream checkout is $UPSTREAM_DIR, else scripts/upstream/checkout.sh
--print-dir.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TABLE = ROOT / "crates" / "excali-text" / "src" / "font_faces_table.rs"
FONTS = ROOT / "crates" / "excali-text" / "assets" / "fonts"

# Fonts.init() order (Fonts.ts:399-412): CSS family name, directory, the
# export name of its descriptor array. Assistant (fonts.css) comes last.
FAMILIES = (
    ("Cascadia", "Cascadia", "CascadiaFontFaces"),
    ("Comic Shanns", "ComicShanns", "ComicShannsFontFaces"),
    ("Excalifont", "Excalifont", "ExcalifontFontFaces"),
    ("Helvetica", "Helvetica", "HelveticaFontFaces"),
    ("Liberation Sans", "Liberation", "LiberationFontFaces"),
    ("Lilita One", "Lilita", "LilitaFontFaces"),
    ("Nunito", "Nunito", "NunitoFontFaces"),
    ("Virgil", "Virgil", "VirgilFontFaces"),
    ("Xiaolai", "Xiaolai", "XiaolaiFontFaces"),
    ("Segoe UI Emoji", "Emoji", "EmojiFontFaces"),
)

# ADR-004, Licence gaps / Vendored builds: upstream file -> vendored file.
REPLACED = {
    "Liberation/LiberationSans-Regular.woff2": "Liberation/LiberationSans-Regular.ttf",
}

IMPORT = re.compile(r'^import\s+(\w+)\s+from\s+"\./([^"]+)";', re.M)
FACE = re.compile(r"\{\s*uri:\s*(\w+)\s*,?(.*?)\}\s*,?\s*(?=\{\s*uri:|\]\s*;)", re.S)
RANGE_LITERAL = re.compile(r'unicodeRange:\s*"([^"]*)"')
RANGE_CONST = re.compile(r"unicodeRange:\s*GOOGLE_FONTS_RANGES\.(\w+)")
WEIGHT = re.compile(r'weight:\s*"([^"]*)"')
GOOGLE = re.compile(r'(\w+):\s*"([^"]*)"')
CSS_FACE = re.compile(r"@font-face\s*\{(.*?)\}", re.S)


def upstream_dir() -> Path:
    env = os.environ.get("UPSTREAM_DIR")
    if env:
        return Path(env)
    out = subprocess.run(
        [str(ROOT / "scripts" / "upstream" / "checkout.sh"), "--print-dir"],
        capture_output=True, text=True, check=True,
    )
    return Path(out.stdout.strip())


def google_ranges(upstream: Path) -> dict[str, str]:
    text = (upstream / "packages" / "common" / "src" / "font-metadata.ts").read_text()
    block = text[text.index("GOOGLE_FONTS_RANGES"):]
    block = block[: block.index("};")]
    return {m.group(1): m.group(2) for m in GOOGLE.finditer(block)}


def faces(upstream: Path) -> list[dict]:
    fonts = upstream / "packages" / "excalidraw" / "fonts"
    google = google_ranges(upstream)
    out: list[dict] = []
    for family, directory, export in FAMILIES:
        text = (fonts / directory / "index.ts").read_text()
        imports = dict(IMPORT.findall(text))
        start = text.index(f"export const {export}")
        body = text[text.index("[", start):]
        body = body[: body.index("];") + 2]
        found = FACE.findall(body)
        if not found:
            raise SystemExit(f"{directory}/index.ts: no face descriptors in {export}")
        for uri, rest in found:
            face = {"family": family, "range": None, "weight": None}
            if uri == "LOCAL_FONT_PROTOCOL":
                face["upstream"] = None
                face["vendored"] = None
            else:
                name = f"{directory}/{imports[uri]}"
                face["upstream"] = name
                face["vendored"] = REPLACED.get(name, name)
            if m := RANGE_LITERAL.search(rest):
                face["range"] = m.group(1)
            elif m := RANGE_CONST.search(rest):
                face["range"] = google[m.group(1)]
            if m := WEIGHT.search(rest):
                face["weight"] = m.group(1)
            out.append(face)
    css = (fonts / "fonts.css").read_text()
    for block in CSS_FACE.findall(css):
        family = re.search(r'font-family:\s*"([^"]+)"', block).group(1)
        url = re.search(r"url\(\.\./fonts/([^)]+)\)", block).group(1)
        weight = re.search(r"font-weight:\s*(\d+)", block).group(1)
        out.append({"family": family, "upstream": url, "vendored": REPLACED.get(url, url),
                    "range": None, "weight": weight})
    return out


def _str(value: str | None) -> str:
    if value is None:
        return "None"
    return 'Some("' + value.replace("\\", "\\\\").replace('"', '\\"') + '")'


def render(items: list[dict]) -> str:
    lines = [
        "// Generated by scripts/fonts/font_faces.py from upstream's",
        "// packages/excalidraw/fonts/*/index.ts and fonts/fonts.css at the pinned",
        "// commit. Do not edit; run `python3 scripts/fonts/font_faces.py write`.",
        "",
        "use crate::font_faces::FontFaceDescriptor;",
        "",
        "/// Every face upstream registers, in registration order: `Fonts.init()`",
        "/// (`packages/excalidraw/fonts/Fonts.ts:399-412`) and then the UI family",
        "/// Assistant from `fonts/fonts.css`.",
        "#[rustfmt::skip]",
        f"pub static FONT_FACES: [FontFaceDescriptor; {len(items)}] = [",
    ]
    for f in items:
        lines.append(
            "    FontFaceDescriptor { "
            f'family: "{f["family"]}", '
            f"upstream: {_str(f['upstream'])}, "
            f"vendored: {_str(f['vendored'])}, "
            f"unicode_range: {_str(f['range'])}, "
            f"weight: {_str(f['weight'])} }},"
        )
    lines.append("];")
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[1] not in ("write", "check"):
        print(__doc__, file=sys.stderr)
        return 2
    items = faces(upstream_dir())
    text = render(items)
    if argv[1] == "write":
        TABLE.write_text(text)
        print(f"wrote {TABLE.relative_to(ROOT)} ({len(items)} faces)")
        return 0
    errs = []
    if not TABLE.is_file() or TABLE.read_text() != text:
        errs.append(f"{TABLE.relative_to(ROOT)} is stale; run scripts/fonts/font_faces.py write")
    for f in items:
        if f["vendored"] and not (FONTS / f["vendored"]).is_file():
            errs.append(f"crates/excali-text/assets/fonts/{f['vendored']}: named by the face table but not vendored")
    for e in errs:
        print(e, file=sys.stderr)
    if not errs:
        print(f"font face table OK ({len(items)} faces)")
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
