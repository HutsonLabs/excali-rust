#!/usr/bin/env python3
"""Font licence gate (ADR-004, task ex-306).

ADR-004 (site/content/decisions/adr-004-fonts.md) holds three tables:

  ## Verification table   one row per upstream family: Family | Id | Licence |
                          Source | Checked | Evidence | Vendored from
  ## Licence gaps         files or families that are not vendored and the
                          licensed fallback each maps to: Family | Not vendored |
                          sha256 | Fallback | Fallback licence
  ## Vendored builds      for a confirmed family with a gap on one build, the
                          only files that may ship: Family | File | sha256 |
                          Derived from

A family counts as confirmed when its licence is SIL OFL 1.1, MIT or Apache
2.0 with an https source URL and an ISO date. Every font file in the tree
must sit in one of upstream's font directory names (DIRECTORY_FAMILIES, e.g.
`Nunito/`, `ComicShanns/`, `Liberation/`), whose family must be confirmed,
next to a licence file with that licence's text, and not hash to a file the
gaps table lists as not vendored. When a confirmed family has a gap on one
build (a gap row with a sha256), each of its files must hash to one of its
Vendored builds rows, so a re-encoded, re-subset or re-compressed copy of the
gap file cannot pass under the family's directory.

Subcommands:
  check                   the ADR tables and the tree (exit 0 clean, 1 violations)
  verify-upstream DIR     each gap's upstream file under the upstream checkout
                          DIR still hashes to the recorded sha256, and every
                          DIRECTORY_FAMILIES directory exists there
"""
from __future__ import annotations

import hashlib
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ADR = ROOT / "site" / "content" / "decisions" / "adr-004-fonts.md"

ALLOWED = ("SIL OFL 1.1", "MIT", "Apache 2.0")
LOCAL_ONLY = "local only"
PENDING = "pending"

# Families upstream registers or names: Fonts.init()
# (packages/excalidraw/fonts/Fonts.ts:375-416), the UI font Assistant
# (packages/excalidraw/fonts/fonts.css) and FONT_FAMILY / the fallback ids
# (packages/common/src/constants.ts:140-167).
UPSTREAM_FAMILIES = (
    "Virgil",
    "Helvetica",
    "Cascadia Code",
    "Excalifont",
    "Nunito",
    "Lilita One",
    "Comic Shanns",
    "Liberation Sans",
    "Assistant",
    "Xiaolai",
    "Segoe UI Emoji",
)

# Upstream's font directories (packages/excalidraw/fonts/<dir>/ at the pin)
# and the family each holds. A font file's directory must be one of these
# names exactly; nothing else maps to a family.
DIRECTORY_FAMILIES = {
    "Virgil": "Virgil",
    "Helvetica": "Helvetica",
    "Cascadia": "Cascadia Code",
    "Excalifont": "Excalifont",
    "Nunito": "Nunito",
    "Lilita": "Lilita One",
    "ComicShanns": "Comic Shanns",
    "Liberation": "Liberation Sans",
    "Assistant": "Assistant",
    "Xiaolai": "Xiaolai",
    "Emoji": "Segoe UI Emoji",
}

FONT_EXTENSIONS = {".woff2", ".woff", ".ttf", ".otf"}
LICENCE_FILES = ("OFL.txt", "OFL", "LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "LICENCE.md")
# Text a licence file must contain, per licence: any one alternative, all of
# whose strings are present.
LICENCE_MARKERS = {
    "SIL OFL 1.1": (("SIL OPEN FONT LICENSE Version 1.1",), ("SIL Open Font License, Version 1.1",)),
    "MIT": (("Permission is hereby granted, free of charge",),),
    "Apache 2.0": (("Apache License", "Version 2.0"),),
}
SKIP_DIRS = {"target", "node_modules"}

DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
URL = re.compile(r"https://[^\s>)|]+")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
UPSTREAM_PATH = re.compile(r"upstream `(fonts/[^`]+)`")


@dataclass
class Family:
    name: str
    id: str
    licence: str
    source: str
    checked: str
    vendored_from: str

    @property
    def confirmed(self) -> bool:
        return self.licence in ALLOWED


@dataclass
class Gap:
    family: str
    not_vendored: str
    sha256: str
    fallback: str
    fallback_licence: str


@dataclass
class Build:
    family: str
    file: str
    sha256: str
    derived_from: str


def _section(text: str, heading: str) -> str | None:
    marker = f"## {heading}"
    if marker not in text:
        return None
    return text.split(marker, 1)[1].split("\n## ", 1)[0]


def _table(section: str) -> list[list[str]]:
    """Body rows of the first markdown table in `section`, cells stripped."""
    rows: list[list[str]] = []
    lines = [ln.strip() for ln in section.splitlines()]
    in_table = False
    for ln in lines:
        if ln.startswith("|"):
            cells = [c.strip() for c in ln.strip("|").split("|")]
            if not in_table:
                in_table = True  # header
                continue
            if all(set(c) <= set("-: ") for c in cells):
                continue
            rows.append(cells)
        elif in_table:
            break
    return rows


def _plain(cell: str) -> str:
    return cell.replace("`", "").strip()


def families(text: str) -> list[Family]:
    section = _section(text, "Verification table")
    if section is None:
        return []
    out = []
    for cells in _table(section):
        cells += [""] * (7 - len(cells))
        url = URL.search(cells[3])
        out.append(Family(
            name=_plain(cells[0]),
            id=_plain(cells[1]),
            licence=_plain(cells[2]),
            source=url.group(0) if url else "",
            checked=_plain(cells[4]),
            vendored_from=cells[6],
        ))
    return out


def gaps(text: str) -> list[Gap]:
    section = _section(text, "Licence gaps")
    if section is None:
        return []
    out = []
    for cells in _table(section):
        cells += [""] * (5 - len(cells))
        out.append(Gap(
            family=_plain(cells[0]),
            not_vendored=cells[1],
            sha256=_plain(cells[2]),
            fallback=_plain(cells[3]),
            fallback_licence=_plain(cells[4]),
        ))
    return out


def builds(text: str) -> list[Build]:
    section = _section(text, "Vendored builds")
    if section is None:
        return []
    out = []
    for cells in _table(section):
        cells += [""] * (4 - len(cells))
        out.append(Build(
            family=_plain(cells[0]),
            file=cells[1],
            sha256=_plain(cells[2]),
            derived_from=cells[3],
        ))
    return out


def _file_gap_families(text: str) -> set[str]:
    """Families with a gap on one particular build (a gap row with a hash)."""
    return {g.family for g in gaps(text) if g.sha256}


def check_adr(text: str, required: tuple[str, ...] = UPSTREAM_FAMILIES) -> list[str]:
    errs: list[str] = []
    if _section(text, "Verification table") is None:
        return ["ADR-004 has no '## Verification table' section"]
    if _section(text, "Licence gaps") is None:
        errs.append("ADR-004 has no '## Licence gaps' section")
    fams = families(text)
    names = [f.name for f in fams]
    for name in required:
        if name not in names:
            errs.append(f"{name}: missing from the verification table")
    gap_families = {g.family for g in gaps(text)}
    for f in fams:
        if f.licence in ALLOWED:
            if not f.source:
                errs.append(f"{f.name}: confirmed licence needs an https source URL")
            if not DATE.match(f.checked):
                errs.append(f"{f.name}: confirmed licence needs the date checked (YYYY-MM-DD), got '{f.checked}'")
        elif f.licence == PENDING:
            if f.name not in gap_families:
                errs.append(f"{f.name}: pending licence must be listed under Licence gaps with its fallback")
        elif f.licence != LOCAL_ONLY:
            errs.append(
                f"{f.name}: licence '{f.licence}' is not one of {', '.join(ALLOWED)} "
                f"(or '{PENDING}' / '{LOCAL_ONLY}')"
            )
    for g in gaps(text):
        if g.fallback_licence not in ALLOWED and g.fallback_licence != "n/a":
            errs.append(f"{g.family}: fallback licence '{g.fallback_licence}' is not one of {', '.join(ALLOWED)}")
        if g.sha256 and not HEX64.match(g.sha256):
            errs.append(f"{g.family}: gap sha256 '{g.sha256}' is not 64 lowercase hex digits")
    confirmed = {f.name for f in fams if f.confirmed}
    recorded = {b.family for b in builds(text)}
    for name in sorted(_file_gap_families(text) & confirmed):
        if name not in recorded:
            errs.append(
                f"{name}: confirmed with a gap on one build, so its shippable files must be "
                "listed under '## Vendored builds' with their sha256"
            )
    for b in builds(text):
        if not HEX64.match(b.sha256):
            errs.append(f"{b.family}: vendored build sha256 '{b.sha256}' is not 64 lowercase hex digits")
        if b.family not in confirmed:
            errs.append(f"{b.family}: vendored build listed for a family whose licence is not confirmed")
    return errs


def _family_for(path: Path, fams: list[Family]) -> Family | None:
    name = DIRECTORY_FAMILIES.get(path.parent.name)
    if name is None:
        return None
    return next((f for f in fams if f.name == name), None)


def font_files(root: Path) -> list[Path]:
    out = []
    stack = [root]
    while stack:
        d = stack.pop()
        for p in sorted(d.iterdir()):
            if p.is_dir():
                if p.name.startswith(".") or p.name in SKIP_DIRS:
                    continue
                stack.append(p)
            elif p.suffix.lower() in FONT_EXTENSIONS:
                out.append(p)
    return sorted(out)


def _carries(text: str, licence: str) -> bool:
    return any(all(m in text for m in alt) for alt in LICENCE_MARKERS[licence])


def check_tree(root: Path, text: str) -> list[str]:
    errs: list[str] = []
    fams = families(text)
    blocked = {g.sha256: g for g in gaps(text) if HEX64.match(g.sha256)}
    pinned = _file_gap_families(text)
    allowed_builds: dict[str, set[str]] = {}
    for b in builds(text):
        allowed_builds.setdefault(b.family, set()).add(b.sha256)
    for path in font_files(root):
        rel = path.relative_to(root).as_posix()
        fam = _family_for(path, fams)
        if fam is None:
            errs.append(
                f"{rel}: no family in the ADR-004 verification table for directory "
                f"'{path.parent.name}' (directories must be one of {', '.join(DIRECTORY_FAMILIES)})"
            )
            continue
        if not fam.confirmed:
            errs.append(f"{rel}: {fam.name} licence is not confirmed ('{fam.licence}'); not vendorable")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest in blocked:
            errs.append(f"{rel}: listed under Licence gaps as not vendored ({blocked[digest].not_vendored})")
            continue
        if fam.name in pinned and digest not in allowed_builds.get(fam.name, set()):
            errs.append(
                f"{rel}: {fam.name} has a licence gap on one build; sha256 {digest} is not one of "
                "its recorded Vendored builds in ADR-004"
            )
            continue
        licence_files = [path.parent / n for n in LICENCE_FILES if (path.parent / n).is_file()]
        if not licence_files:
            errs.append(f"{rel}: no licence file ({', '.join(LICENCE_FILES)}) next to the font")
            continue
        texts = [p.read_text(encoding="utf-8", errors="replace") for p in licence_files]
        if not any(_carries(t, fam.licence) for t in texts):
            errs.append(f"{rel}: licence file next to the font does not carry the {fam.licence} text")
    return errs


def verify_upstream(upstream: Path, text: str) -> list[str]:
    errs: list[str] = []
    fonts_dir = upstream / "packages" / "excalidraw" / "fonts"
    for d in DIRECTORY_FAMILIES:
        if not (fonts_dir / d).is_dir():
            errs.append(f"{DIRECTORY_FAMILIES[d]}: upstream font directory fonts/{d}/ is missing under {upstream}")
    for g in gaps(text):
        m = UPSTREAM_PATH.search(g.not_vendored)
        if not m:
            continue
        path = upstream / "packages" / "excalidraw" / m.group(1)
        if not path.is_file():
            errs.append(f"{g.family}: upstream file {m.group(1)} is missing under {upstream}")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != g.sha256:
            errs.append(f"{g.family}: upstream {m.group(1)} has sha256 {digest}, ADR-004 records '{g.sha256}'")
    return errs


def _report(errs: list[str], what: str) -> int:
    if errs:
        print(f"font licence gate FAILED ({what}): {len(errs)} violation(s)")
        for e in errs:
            print(f"  {e}")
        return 1
    print(f"font licence gate OK ({what})")
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in ("check", "verify-upstream"):
        print(__doc__)
        return 2
    text = ADR.read_text(encoding="utf-8")
    if argv[0] == "check":
        return _report(check_adr(text) + check_tree(ROOT, text), "check")
    if len(argv) != 2:
        print("usage: fonts.py verify-upstream DIR")
        return 2
    return _report(verify_upstream(Path(argv[1]), text), "verify-upstream")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
