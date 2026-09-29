#!/usr/bin/env python3
"""Checks a smoke run of the example app (scripts/smoke.sh): the report
ui/app.js wrote through smoke_report, and the files open, save, save as and
the exports wrote in the smoke directory.

    check-smoke.py DIR SCENE
"""
import json
import pathlib
import sys


def main(dir_, scene_path):
    d = pathlib.Path(dir_).resolve()
    scene = json.loads(pathlib.Path(scene_path).read_text())
    report = json.loads((d / "report.json").read_text())
    failures = []

    def check(ok, what):
        if not ok:
            failures.append(what)

    check("error" not in report, f"the smoke run threw: {report.get('error')}")
    check(report.get("problems") == [], f"CSP violations or errors: {report.get('problems')}")
    check(report.get("mounted") is True, "<excali-editor> has no canvas")
    opened = d / "open.excalidraw"
    for key, path in [("opened", opened), ("saved", opened), ("savedAs", opened),
                      ("png", d / "open.png"), ("svg", d / "open.svg")]:
        got = report.get(key)
        check(got is not None and pathlib.Path(got).resolve() == path, f"{key}: {got}, want {path}")
    live = [e for e in scene["elements"] if not e.get("isDeleted")]
    check(report.get("stateAfterOpen", {}).get("elementCount") == len(live),
          f"elementCount after open: {report.get('stateAfterOpen')}, want {len(live)}")
    ids = [e["id"] for e in report.get("scene", {}).get("elements", [])]
    check(ids == [e["id"] for e in live], f"the editor's scene: {ids}")
    saved = json.loads(opened.read_text())
    check(saved.get("type") == "excalidraw"
          and [e["id"] for e in saved["elements"]] == [e["id"] for e in live],
          "open.excalidraw after save is not the scene")
    png = (d / "open.png").read_bytes() if (d / "open.png").exists() else b""
    check(png.startswith(b"\x89PNG\r\n\x1a\n"), "open.png is not a PNG")
    svg = (d / "open.svg").read_text() if (d / "open.svg").exists() else ""
    check("<svg" in svg and "original text" in svg, "open.svg lacks the scene's text")

    if failures:
        print("smoke: FAILED", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        print(json.dumps(report, indent=2)[:4000], file=sys.stderr)
        return 1
    print(f"smoke: ok (opened, saved, saved as, exported png {len(png)} bytes and svg {len(svg)} bytes; "
          f"{len(live)} elements; no CSP violations)")
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
