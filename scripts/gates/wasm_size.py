#!/usr/bin/env python3
"""Size budget of the web build (ex-501).

The budgets are the rows of the table under "## Budgets" on
site/content/plan/phases.md, the single source; this gate reads them from
there so the page and CI cannot disagree:

    | WASM module, no fonts, gzip | <= 1.5 MB |  -> excali_editor_bg.wasm
    | ES-module JS (...), gzip    | <= 20 KB  |  -> excali_editor.js

Sizes are gzip at level 9 with no timestamp or file name in the header
(deterministic on every machine), in decimal units (1 KB = 1,000 bytes,
1 MB = 1,000,000 bytes). A file at its budget passes; one byte over fails.

    python3 scripts/gates/wasm_size.py check DIST [--phases PAGE]

Exit 0 within budget, 1 over budget or a file missing, 2 usage error.
Tests: scripts/gates/test_wasm_size.py
"""
from __future__ import annotations

import gzip
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PHASES = ROOT / "site" / "content" / "plan" / "phases.md"

# Budget row label (its start, on the phases page) -> file in the build.
ROWS = {
    "WASM module": "excali_editor_bg.wasm",
    "ES-module JS": "excali_editor.js",
}

UNITS = {"B": 1, "KB": 1_000, "MB": 1_000_000}
SIZE = re.compile(r"^(?:≤\s*)?(\d+(?:\.\d+)?)\s*(B|KB|MB)$")


def parse_size(text: str) -> int:
    m = SIZE.match(text.strip())
    if not m:
        raise ValueError(f"'{text}' is not a size in B, KB or MB")
    value = float(m.group(1)) * UNITS[m.group(2)]
    if value != int(value):
        raise ValueError(f"'{text}' is not a whole number of bytes")
    return int(value)


def budgets(phases: Path = PHASES) -> dict[str, int]:
    """File name -> gzip budget in bytes, from the page's Budgets table."""
    text = phases.read_text(encoding="utf-8")
    section = text.split("\n## Budgets", 1)
    if len(section) != 2:
        raise ValueError(f"{phases}: no '## Budgets' section")
    body = section[1].split("\n## ", 1)[0]
    found: dict[str, int] = {}
    for line in body.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 2:
            continue
        for label, file in ROWS.items():
            if cells[0].startswith(label):
                if not cells[0].endswith(", gzip"):
                    raise ValueError(
                        f"{phases}: the '{label}' budget must be measured gzip ('..., gzip')"
                    )
                found[file] = parse_size(cells[1])
    for label, file in ROWS.items():
        if file not in found:
            raise ValueError(f"{phases}: no '{label}' row in the Budgets table")
    return found


def gzip_size(data: bytes) -> int:
    return len(gzip.compress(data, compresslevel=9, mtime=0))


def check(dist: Path, phases: Path = PHASES) -> int:
    limits = budgets(phases)
    failed = False
    for file, limit in limits.items():
        path = dist / file
        if not path.is_file():
            print(f"wasm size: {file} missing from {dist}; run scripts/web/build.sh", file=sys.stderr)
            failed = True
            continue
        raw = path.stat().st_size
        size = gzip_size(path.read_bytes())
        verdict = "ok" if size <= limit else "OVER BUDGET"
        print(
            f"{file}: {size:,} bytes gzip ({raw:,} raw), budget {limit:,} "
            f"({size / limit:.1%}) {verdict}"
        )
        if size > limit:
            print(
                f"wasm size: {file} is over budget by {size - limit:,} bytes "
                f"(site/content/plan/phases.md#budgets)",
                file=sys.stderr,
            )
            failed = True
    return 1 if failed else 0


def main(argv: list[str]) -> int:
    usage = "usage: wasm_size.py check DIST [--phases PAGE]"
    if len(argv) not in (2, 4) or argv[0] != "check":
        print(usage, file=sys.stderr)
        return 2
    phases = PHASES
    if len(argv) == 4:
        if argv[2] != "--phases":
            print(usage, file=sys.stderr)
            return 2
        phases = Path(argv[3])
    try:
        return check(Path(argv[1]), phases)
    except (OSError, ValueError) as e:
        print(f"wasm size: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
