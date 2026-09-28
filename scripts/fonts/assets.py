#!/usr/bin/env python3
"""Font asset pipeline (task ex-307): vendor the range-split font files,
write the manifest that maps family and unicode range to file, and check
both against disk and upstream.

crates/excali-text/assets/fonts/ holds:

  <Dir>/<file>        every font file upstream's registry names
                      (Fonts.init, packages/excalidraw/fonts/Fonts.ts:375-416)
                      and the UI font Assistant (fonts/fonts.css:5-35), in
                      upstream's directory names (ADR-004 rule 4), copied
                      byte for byte from the pinned checkout. The one
                      exception is Liberation Sans: upstream's 1.05 woff2 is a
                      licence gap and is replaced by the OFL 2.1.5 ttf from
                      liberation-fonts (ADR-004, Licence gaps / Vendored
                      builds).
  <Dir>/<licence>     the family's licence text, from the source ADR-004
                      records (LICENCES below: a raw URL at a fixed commit,
                      the Liberation release tarball, or upstream's own copy
                      of the name table for Excalifont).
  manifest.json       families in upstream's registry order, each with its
                      font faces: file, CSS format, unicode-range (null for
                      the full range), weight, style, display, size, sha256
                      and the upstream file it stands for; the UI faces; the
                      licence files.

The registry itself is upstream's output: crates/excali-text/tests/fixtures/
font-assets.json, written by tools/goldens/font-assets.mjs from the pinned
checkout. This script only adds what is on disk. It also writes
crates/excali-text/src/font_assets/generated.rs, the same table as Rust
statics, so the crate needs no JSON parser at run time.

  assets.py vendor [--upstream DIR]   copy files and licences, then `write`
                                      (network for licences and the Liberation
                                      tarball unless already cached in .tools)
  assets.py write                     regenerate manifest.json and generated.rs
  assets.py check                     manifest.json and generated.rs are what
                                      `write` produces, every listed file is on
                                      disk with its size and sha256, and no
                                      other file is (offline; runs in CI)
  assets.py verify-upstream DIR       every vendored font equals its upstream
                                      file byte for byte (except the recorded
                                      substitution) and the UI faces match
                                      upstream's fonts.css

Exit status: 0 clean, 1 problems found, 2 usage error.
"""
from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "crates" / "excali-text"
ASSETS = CRATE / "assets" / "fonts"
MANIFEST = ASSETS / "manifest.json"
GOLDEN = CRATE / "tests" / "fixtures" / "font-assets.json"
GENERATED = CRATE / "src" / "font_assets" / "generated.rs"
TOOLS = ROOT / ".tools"

FULL_RANGE = (0, 0x10FFFF)

# ADR-004 Licence gaps / Vendored builds: upstream files replaced by another
# build of the same family. upstream file -> (vendored file, CSS format,
# where the vendored file comes from).
SUBSTITUTIONS = {
    "Liberation/LiberationSans-Regular.woff2": (
        "Liberation/LiberationSans-Regular.ttf",
        "truetype",
        "liberation-fonts release 2.1.5, liberation-fonts-ttf-2.1.5.tar.gz",
    ),
}

LIBERATION_TARBALL = {
    "url": "https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz",
    "sha256": "7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0",
    "cache": TOOLS / "fonts-check" / "lib.tgz",
    "members": {
        "liberation-fonts-ttf-2.1.5/LiberationSans-Regular.ttf": "Liberation/LiberationSans-Regular.ttf",
        "liberation-fonts-ttf-2.1.5/LICENSE": "Liberation/LICENSE",
    },
}

# UI font faces: upstream's fonts.css (packages/excalidraw/fonts/fonts.css:5-35),
# loaded by CSS before the editor starts, not through Fonts.registered.
UI_FAMILY = "Assistant"
UI_FACES = (
    ("Assistant/Assistant-Regular.woff2", "400"),
    ("Assistant/Assistant-Medium.woff2", "500"),
    ("Assistant/Assistant-SemiBold.woff2", "600"),
    ("Assistant/Assistant-Bold.woff2", "700"),
)


@dataclass(frozen=True)
class Licence:
    directory: str
    file: str
    licence: str
    source: str
    sha256: str


# One licence file per directory, from the source ADR-004's verification
# table records, fetched 2026-09-28. Raw URLs are at the commit that last
# changed the file, so the bytes cannot move.
LICENCES = (
    Licence(
        "Cascadia",
        "LICENSE",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/microsoft/cascadia-code/4e01cd4fd3758ae081848154b5eb01f04b447947/LICENSE",
        "51882cd3cdba4e16f220f44ddb08a635c38c44ea6e0975db2574f4be6f958238",
    ),
    Licence(
        "ComicShanns",
        "LICENSE.md",
        "MIT",
        "https://raw.githubusercontent.com/jesusmgg/comic-shanns-mono/0715f15bc47d32c8b1cf5bda4617d4335b924ac7/LICENSE.md",
        "d11d2829ba3f9dd21e8851a96c315694319f454a6de0fefc5d56495b09a758c1",
    ),
    Licence(
        "Excalifont",
        "OFL.txt",
        "SIL OFL 1.1",
        "upstream packages/excalidraw/fonts/Excalifont/index.ts:13-116 (the origin font's name table)",
        "1e92b41986476325f5b07f763cc876199f0eb2bb4e55058f95e484d93a40a756",
    ),
    Licence(
        "Liberation",
        "LICENSE",
        "SIL OFL 1.1",
        "liberation-fonts-ttf-2.1.5.tar.gz liberation-fonts-ttf-2.1.5/LICENSE",
        "93fed46019c38bbe566b479d22148e2e8a1e85ada614accb0211c37b2c61c19b",
    ),
    Licence(
        "Lilita",
        "OFL.txt",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/google/fonts/90abd17b4f97671435798b6147b698aa9087612f/ofl/lilitaone/OFL.txt",
        "255d5debbb80eb2ea762644311f266a279e8778f00156655a516e2b7781a63e1",
    ),
    Licence(
        "Nunito",
        "OFL.txt",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/google/fonts/a5bd0ea86b2576f86672aab557a6024d272187a5/ofl/nunito/OFL.txt",
        "580df76c95a1ec5ab878ceb25bb3d85c6a076804e9c970c8c6972aea775fdf65",
    ),
    Licence(
        "Virgil",
        "LICENSE.md",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/excalidraw/virgil/45fe1d8e3dba504adbd87a90b20449862fc0cb4d/LICENSE.md",
        "a3ac9cdc65ef304ffb095d8cc877ee93bf1bffa88269693cd42ddf001650ffd0",
    ),
    Licence(
        "Xiaolai",
        "OFL.txt",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/lxgw/kose-font/5359953d118976c7797ad3257d7cf4036ad09f3c/OFL.txt",
        "0df7e09be4c2c850a48bd8beb9cd64b343aad49cd5d3f6cfb2ad2e3d28a56ca4",
    ),
    Licence(
        "Assistant",
        "OFL.txt",
        "SIL OFL 1.1",
        "https://raw.githubusercontent.com/google/fonts/89c9db01508963eb8b48a171c8baf2ef750c5bd9/ofl/assistant/OFL.txt",
        "7eaf6282cfab122b99ffd62a122fc27b6c3c8f2c65b56ff9b3a25badbc64887d",
    ),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_unicode_range(text: str) -> list[tuple[int, int]]:
    """Ranges of a CSS unicode-range the way upstream's getUnicodeRangeRegex
    reads it (ExcalidrawFontFace.ts:114-131): split on /,\\s*/, drop the first
    "U+", split on "-", hex start and optional hex end."""
    out = []
    for part in re.split(r",\s*", text):
        bounds = part.replace("U+", "", 1).split("-")
        start = int(bounds[0], 16)
        end = int(bounds[1], 16) if len(bounds) > 1 else start
        out.append((start, end))
    return out


def _format_name(css: str | None) -> str | None:
    """`format('woff2')` (getFormat, ExcalidrawFontFace.ts:172-189) -> woff2."""
    if css is None:
        return None
    m = re.fullmatch(r"format\('([^']+)'\)", css)
    if not m:
        raise ValueError(f"unexpected CSS format {css!r}")
    return m.group(1)


def _face(assets: Path, upstream_file: str, fmt: str, unicode_range: str | None, descriptors: dict) -> dict:
    file, fmt_out = upstream_file, fmt
    if upstream_file in SUBSTITUTIONS:
        file, fmt_out, _ = SUBSTITUTIONS[upstream_file]
    path = assets / file
    data = path.read_bytes() if path.is_file() else b""
    return {
        "file": file,
        "format": fmt_out,
        "unicodeRange": unicode_range,
        "weight": descriptors["weight"],
        "style": descriptors["style"],
        "display": descriptors["display"],
        "size": len(data),
        "sha256": sha256(data),
        "upstreamFile": upstream_file,
    }


def build_manifest(golden: dict, assets: Path) -> dict:
    families = []
    for fam in golden["registered"]:
        faces = []
        if not fam["local"]:
            for f in fam["faces"]:
                faces.append(_face(assets, f["file"], _format_name(f["format"]), f["unicodeRange"], f["descriptors"]))
        families.append({"id": fam["id"], "family": fam["family"], "local": fam["local"], "faces": faces})
    ui = [
        _face(assets, file, "woff2", None, {"weight": weight, "style": "normal", "display": "swap"})
        for file, weight in UI_FACES
    ]
    licences = []
    for lic in LICENCES:
        path = assets / lic.directory / lic.file
        data = path.read_bytes() if path.is_file() else b""
        licences.append({
            "file": f"{lic.directory}/{lic.file}",
            "licence": lic.licence,
            "source": lic.source,
            "size": len(data),
            "sha256": sha256(data),
        })
    return {
        "description": (
            "Font assets of excali-text (ex-307): upstream's font registry (Fonts.init, "
            "packages/excalidraw/fonts/Fonts.ts:375-416) in its order, each family's range-split "
            "font faces with the file, CSS format, unicode-range (null: U+0-10FFFF) and descriptors "
            "upstream registers, the UI font faces of fonts/fonts.css, and the licence file of every "
            "directory (ADR-004). Written by scripts/fonts/assets.py; paths are relative to this file."
        ),
        "upstream": golden["upstream"],
        "families": families,
        "ui": {"family": UI_FAMILY, "faces": ui},
        "licences": licences,
        "substitutions": [
            {"upstreamFile": up, "file": file, "from": src}
            for up, (file, _fmt, src) in sorted(SUBSTITUTIONS.items())
        ],
    }


def manifest_text(manifest: dict) -> str:
    return json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"


def _rs_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def _rs_face(family: str, face: dict, indent: str) -> str:
    ranges = parse_unicode_range(face["unicodeRange"]) if face["unicodeRange"] else [FULL_RANGE]
    fmt = {"woff2": "FontFormat::Woff2", "truetype": "FontFormat::TrueType"}[face["format"]]
    unicode_range = f"Some({_rs_str(face['unicodeRange'])})" if face["unicodeRange"] else "None"
    range_items = ", ".join(f"(0x{a:x}, 0x{b:x})" for a, b in ranges)
    return (
        f"{indent}FontFaceAsset {{\n"
        f"{indent}    family: {_rs_str(family)},\n"
        f"{indent}    file: {_rs_str(face['file'])},\n"
        f"{indent}    format: {fmt},\n"
        f"{indent}    unicode_range: {unicode_range},\n"
        f"{indent}    ranges: &[{range_items}],\n"
        f"{indent}    weight: {_rs_str(face['weight'])},\n"
        f"{indent}    style: {_rs_str(face['style'])},\n"
        f"{indent}    display: {_rs_str(face['display'])},\n"
        f"{indent}    size: {face['size']},\n"
        f"{indent}    sha256: {_rs_str(face['sha256'])},\n"
        f"{indent}    upstream_file: {_rs_str(face['upstreamFile'])},\n"
        f"{indent}}},\n"
    )


def rust_text(manifest: dict) -> str:
    out = [
        "// @generated by scripts/fonts/assets.py from crates/excali-text/assets/fonts/manifest.json.\n",
        "// Do not edit: run `python3 scripts/fonts/assets.py write`.\n",
        "\n",
        "use excali_core::element::FontFamily;\n",
        "\n",
        "use super::{FontFaceAsset, FontFormat, RegisteredFamily};\n",
        "\n",
        "/// `Fonts.registered` in upstream's order (`Fonts.ts:398-411`).\n",
        f"pub(super) static REGISTERED: [RegisteredFamily; {len(manifest['families'])}] = [\n",
    ]
    for fam in manifest["families"]:
        out.append("    RegisteredFamily {\n")
        out.append(f"        id: FontFamily({fam['id']}),\n")
        out.append(f"        family: {_rs_str(fam['family'])},\n")
        out.append(f"        local: {'true' if fam['local'] else 'false'},\n")
        if fam["faces"]:
            out.append("        faces: &[\n")
            for face in fam["faces"]:
                out.append(_rs_face(fam["family"], face, "            "))
            out.append("        ],\n")
        else:
            out.append("        faces: &[],\n")
        out.append("    },\n")
    out.append("];\n\n")
    ui = manifest["ui"]
    out.append("/// The UI font faces (`fonts/fonts.css:5-35`).\n")
    out.append(f"pub(super) static UI_FACES: [FontFaceAsset; {len(ui['faces'])}] = [\n")
    for face in ui["faces"]:
        out.append(_rs_face(ui["family"], face, "    "))
    out.append("];\n")
    return "".join(out)


def _golden() -> dict:
    return json.loads(GOLDEN.read_text(encoding="utf-8"))


def write(golden: dict, assets: Path, generated: Path) -> None:
    manifest = build_manifest(golden, assets)
    (assets / "manifest.json").write_text(manifest_text(manifest), encoding="utf-8")
    generated.parent.mkdir(parents=True, exist_ok=True)
    generated.write_text(rust_text(manifest), encoding="utf-8")


def check(golden: dict, assets: Path, generated: Path) -> list[str]:
    errs: list[str] = []
    manifest = build_manifest(golden, assets)
    listed = set()
    for fam in manifest["families"]:
        for face in fam["faces"]:
            listed.add(face["file"])
    for face in manifest["ui"]["faces"]:
        listed.add(face["file"])
    for lic in manifest["licences"]:
        listed.add(lic["file"])
    for f in sorted(listed):
        if not (assets / f).is_file():
            errs.append(f"{f}: listed but missing from {assets}")
    expected_lic = {f"{l.directory}/{l.file}": l for l in LICENCES}
    for lic in manifest["licences"]:
        want = expected_lic[lic["file"]].sha256
        if lic["sha256"] != want and (assets / lic["file"]).is_file():
            errs.append(f"{lic['file']}: sha256 {lic['sha256']}, the recorded source has {want}")
    for path in sorted(p for p in assets.rglob("*") if p.is_file()):
        rel = path.relative_to(assets).as_posix()
        if rel != "manifest.json" and rel not in listed:
            errs.append(f"{rel}: on disk but not in the manifest")
    committed = (assets / "manifest.json")
    if not committed.is_file() or committed.read_text(encoding="utf-8") != manifest_text(manifest):
        errs.append(f"{committed}: stale (sizes, hashes or registry differ); run scripts/fonts/assets.py write")
    if not generated.is_file() or generated.read_text(encoding="utf-8") != rust_text(manifest):
        errs.append(f"{generated}: stale; run scripts/fonts/assets.py write")
    return errs


def verify_upstream(golden: dict, assets: Path, upstream: Path) -> list[str]:
    errs: list[str] = []
    fonts = upstream / "packages" / "excalidraw" / "fonts"
    manifest = build_manifest(golden, assets)
    faces = [f for fam in manifest["families"] for f in fam["faces"]] + manifest["ui"]["faces"]
    for face in faces:
        up = fonts / face["upstreamFile"]
        if not up.is_file():
            errs.append(f"{face['upstreamFile']}: missing from {fonts}")
            continue
        if face["upstreamFile"] in SUBSTITUTIONS:
            continue
        if face["sha256"] != sha256(up.read_bytes()):
            errs.append(f"{face['file']}: differs from upstream {face['upstreamFile']}")
    css = (fonts / "fonts.css").read_text(encoding="utf-8")
    declared = re.findall(
        r'@font-face \{\s*font-family: "([^"]+)";\s*src: url\(\.\./fonts/([^)]+)\) format\("woff2"\);\s*font-weight: (\d+);',
        css,
    )
    want = [(UI_FAMILY, file, weight) for file, weight in UI_FACES]
    if declared != want:
        errs.append(f"fonts.css declares {declared}, UI_FACES records {want}")
    return errs


def _fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=60) as r:  # noqa: S310 (fixed https URLs)
        return r.read()


def _excalifont_licence(upstream: Path) -> bytes:
    """The copyright and licence lines of Excalifont's origin name table,
    quoted in upstream's Excalifont/index.ts (the subset files keep only
    name ID 0)."""
    src = (upstream / "packages" / "excalidraw" / "fonts" / "Excalifont" / "index.ts").read_text(encoding="utf-8")
    table = src.split("Origin File Name Table:\n", 1)[1].split("\nlicenseURL:", 1)[0]
    lines = table.split("\n")
    copyright_line = next(l for l in lines if l.startswith("copyright: "))
    licence = table.split("license: ", 1)[1]
    header = (
        "Excalifont\n"
        f"{copyright_line.removeprefix('copyright: ')}\n"
        "Source: the origin font's name table, as quoted in upstream Excalidraw's\n"
        "packages/excalidraw/fonts/Excalifont/index.ts. Publisher page:\n"
        "https://plus.excalidraw.com/excalifont (OFL-1.1).\n\n"
    )
    return (header + licence.rstrip("\n") + "\n").encode("utf-8")


def _liberation_tarball() -> Path:
    cache = LIBERATION_TARBALL["cache"]
    if not cache.is_file():
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(_fetch(LIBERATION_TARBALL["url"]))
    digest = sha256(cache.read_bytes())
    if digest != LIBERATION_TARBALL["sha256"]:
        raise SystemExit(f"{cache}: sha256 {digest}, expected {LIBERATION_TARBALL['sha256']}")
    return cache


def vendor(golden: dict, assets: Path, upstream: Path) -> None:
    fonts = upstream / "packages" / "excalidraw" / "fonts"
    if assets.exists():
        shutil.rmtree(assets)
    assets.mkdir(parents=True)
    files = [f["file"] for fam in golden["registered"] if not fam["local"] for f in fam["faces"]]
    files += [file for file, _ in UI_FACES]
    for file in files:
        if file in SUBSTITUTIONS:
            continue
        dest = assets / file
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(fonts / file, dest)
    with tarfile.open(_liberation_tarball()) as tar:
        for member, dest in LIBERATION_TARBALL["members"].items():
            data = tar.extractfile(member).read()
            (assets / dest).parent.mkdir(parents=True, exist_ok=True)
            (assets / dest).write_bytes(data)
    for lic in LICENCES:
        dest = assets / lic.directory / lic.file
        if lic.directory == "Liberation":
            continue
        if lic.directory == "Excalifont":
            dest.write_bytes(_excalifont_licence(upstream))
            continue
        dest.write_bytes(_fetch(lic.source))


def _upstream_dir() -> Path:
    out = subprocess.run(
        ["bash", str(ROOT / "scripts" / "upstream" / "checkout.sh"), "--print-dir"],
        capture_output=True, text=True, check=True,
    )
    return Path(out.stdout.strip())


def _report(errs: list[str], what: str) -> int:
    if errs:
        print(f"font assets FAILED ({what}): {len(errs)} problem(s)")
        for e in errs:
            print(f"  {e}")
        return 1
    print(f"font assets OK ({what})")
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in ("vendor", "write", "check", "verify-upstream"):
        print(__doc__)
        return 2
    golden = _golden()
    cmd = argv[0]
    if cmd == "vendor":
        upstream = Path(argv[2]) if len(argv) == 3 and argv[1] == "--upstream" else _upstream_dir()
        vendor(golden, ASSETS, upstream)
        write(golden, ASSETS, GENERATED)
        return _report(check(golden, ASSETS, GENERATED), "vendor")
    if cmd == "write":
        write(golden, ASSETS, GENERATED)
        return _report(check(golden, ASSETS, GENERATED), "write")
    if cmd == "check":
        return _report(check(golden, ASSETS, GENERATED), "check")
    if len(argv) != 2:
        print("usage: assets.py verify-upstream DIR")
        return 2
    return _report(verify_upstream(golden, ASSETS, Path(argv[1])), "verify-upstream")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
