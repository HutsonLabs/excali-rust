#!/usr/bin/env python3
"""Contact sheets of the library catalogue (task ex-410).

Renders every item of every catalogue library in fixtures/libraries (the
232 .excalidrawlib.gz files fixtures/manifest.json lists, ex-003) with
`excali lib preview`: each library's items drawn onto the preview image
upstream's publish dialog generates (generatePreviewImage,
packages/excalidraw/components/PublishLibrary.tsx:38-105). Writes into OUT:

  <author>/<library>.png   one contact sheet per library
  report.json              per library: path, sheet, item count and each
                           item's canvas size, or the error
  index.html               every sheet with its name and item count

The rust workflow's corpus-render job runs it on every pull request and
keeps OUT as the corpus-contact-sheets artifact.

  contact_sheets.py --excali target/release/excali --out target/contact-sheets

Exit status: 0 when all 232 catalogue libraries rendered with at least one
item each, 1 otherwise (a library failed or had no items, or the manifest
did not yield the 232 libraries; each problem is printed), 2 usage error.
"""
from __future__ import annotations

import argparse
import html
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"
LIBRARY_SUFFIX = ".excalidrawlib.gz"
# The catalogue libraries fixtures/manifest.json lists (ex-003); the same count
# crates/excali-cli/tests/corpus.rs asserts.
EXPECTED_LIBRARIES = 232


def catalogue(manifest: dict) -> list[str]:
    """The catalogue libraries' paths under fixtures/, in manifest order."""
    return [
        entry["path"]
        for entry in manifest["files"]
        if entry.get("source") == "libraries" and entry["path"].endswith(LIBRARY_SUFFIX)
    ]


def sheet_name(path: str) -> str:
    """libraries/<author>/<name>.excalidrawlib.gz -> <author>/<name>.png"""
    rel = path.removeprefix("libraries/")
    return rel[: -len(LIBRARY_SUFFIX)] + ".png"


def index_html(entries: list[dict]) -> str:
    """The page showing every sheet, failures first."""
    failed = [e for e in entries if "error" in e]
    items = sum(len(e.get("items", [])) for e in entries)
    rows = []
    for e in failed + [e for e in entries if "error" not in e]:
        name = html.escape(e["path"])
        if "error" in e:
            rows.append(f'<section class="failed"><h2>{name}</h2><pre>{html.escape(e["error"])}</pre></section>')
        else:
            sheet = html.escape(e["sheet"], quote=True)
            rows.append(
                f"<section><h2>{name} ({len(e['items'])} items)</h2>"
                f'<img src="{sheet}" alt="{name}" loading="lazy"></section>'
            )
    return (
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Catalogue contact sheets</title>\n"
        "<style>body{font-family:sans-serif}h2{font-size:14px}"
        ".failed{color:#c92a2a}img{max-width:100%}</style>\n"
        f"<h1>{len(entries)} libraries, {items} items, {len(failed)} failed</h1>\n"
        + "\n".join(rows)
        + "\n"
    )


def render(excali: Path, out: Path, paths: list[str]) -> list[dict]:
    entries = []
    for path in paths:
        sheet = sheet_name(path)
        target = out / sheet
        target.parent.mkdir(parents=True, exist_ok=True)
        run = subprocess.run(
            [str(excali), "lib", "preview", str(FIXTURES / path), "-o", str(target), "--json"],
            capture_output=True,
            text=True,
        )
        entry: dict = {"path": path}
        if run.returncode == 0:
            report = json.loads(run.stdout)
            entry.update(sheet=sheet, width=report["width"], height=report["height"])
            entry["items"] = [
                {k: item[k] for k in ("id", "name", "width", "height")} for item in report["items"]
            ]
        else:
            entry["error"] = f"exit {run.returncode}: {run.stderr.strip()}"
        entries.append(entry)
    return entries


def problems(entries: list[dict]) -> list[str]:
    """Why a run must fail: a library that failed or drew no items, a
    catalogue that is not the expected 232 libraries, or no items at all."""
    found = []
    for e in entries:
        if "error" in e:
            found.append(f"{e['path']}: {e['error']}")
        elif not e.get("items"):
            found.append(f"{e['path']}: no items rendered")
    if len(entries) != EXPECTED_LIBRARIES:
        found.append(f"{len(entries)} libraries rendered, expected {EXPECTED_LIBRARIES}")
    if not any(e.get("items") for e in entries):
        found.append("no items rendered")
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--excali", type=Path, required=True, help="the excali binary")
    parser.add_argument("--out", type=Path, required=True, help="the output directory")
    args = parser.parse_args(argv)

    manifest = json.loads((FIXTURES / "manifest.json").read_text())
    paths = catalogue(manifest)
    args.out.mkdir(parents=True, exist_ok=True)
    entries = render(args.excali, args.out, paths)
    (args.out / "report.json").write_text(json.dumps(entries, indent=2) + "\n")
    (args.out / "index.html").write_text(index_html(entries))

    failed = [e for e in entries if "error" in e]
    items = sum(len(e.get("items", [])) for e in entries)
    found = problems(entries)
    for problem in found:
        print(problem, file=sys.stderr)
    print(f"{len(entries)} libraries, {items} items rendered, {len(failed)} failed")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
