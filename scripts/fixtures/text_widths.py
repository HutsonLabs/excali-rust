#!/usr/bin/env python3
"""Text-width fixture for excali-text's measurement tests (ex-302).

Upstream stores every text element's `width` as the browser measured it
(`measureText`, packages/element/src/textMeasurements.ts:12-27, 121-168). The
fixture is a set of such texts from the ex-003 library corpus
(fixtures/libraries, excalidraw-libraries at the pinned commit), with the
stored `width` and `height` the port's measurement must reproduce within
0.5 px (crates/excali-text/tests/text_widths.rs).

A text element is eligible when it has `lineHeight` (written by an upstream
recent enough to measure with the unitless line height), `autoResize` is not
false (so `width` is the measured width, not a width the user wrapped to), and
its `fontFamily` is one the port vendors (1 Virgil, 3 Cascadia, 5 Excalifont,
6 Nunito, 7 Lilita One, 8 Comic Shanns, 9 Liberation Sans, 10 Assistant).
The fixture takes every eligible text of the (library, fontFamily) pairs in
PAIRS. Those are the pairs whose texts were measured with the font builds the
port vendors, in a browser that shapes with kerning; the corpus also holds
widths from older font builds, texts widened by hand after measuring
(exactly +1, +2 or +3 px) and generated libraries, and the per-family
deviation over the whole corpus is ex-308's report.

  text_widths.py write   regenerate crates/excali-text/tests/fixtures/text-widths.json
  text_widths.py check   exit 1 if the committed fixture differs from the corpus
"""
from __future__ import annotations

import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"
OUT = ROOT / "crates" / "excali-text" / "tests" / "fixtures" / "text-widths.json"

VENDORED_FAMILIES = {1, 3, 5, 6, 7, 8, 9, 10}

# (library file under fixtures/libraries, fontFamily)
PAIRS = (
    ("aimpizza/3d-coordinate-systems-graphs.excalidrawlib", 1),
    ("dmtwng/archimate-application-layer.excalidrawlib", 1),
    ("booknerdonmars/bullet-journal-trackers.excalidrawlib", 1),
    ("itsmestefanjay/camunda-platform-icons.excalidrawlib", 1),
    ("gabrielamacakova/christmas-essentials.excalidrawlib", 1),
    ("chuqbach/data-platform.excalidrawlib", 1),
    ("mrmaffen/dnd-ttrpg-battle-map-creature-tokens.excalidrawlib", 1),
    ("webkul/ecommerce-mobile-ui.excalidrawlib", 1),
    ("r4z4/elixir.excalidrawlib", 1),
    ("timothygalvin/excalidraw-archimate-template.excalidrawlib", 1),
    ("finfin/flow-chart-symbols.excalidrawlib", 1),
    ("mateuszbaransanok/it-icons.excalidrawlib", 1),
    ("pclainchard/it-logos.excalidrawlib", 1),
    ("boemska-nik/kubernetes-icons.excalidrawlib", 1),
    ("wictorwilen/microsoft-365-icons.excalidrawlib", 1),
    ("mwc360/microsoft-fabric-architecture-icons.excalidrawlib", 1),
    ("gabrielamacakova/presentation-bundle.excalidrawlib", 1),
    ("moochin/simple-characters.excalidrawlib", 1),
    ("odraghi/vmware-architecture-design.excalidrawlib", 1),
    ("stojanovic/aws-serverless-icons-v2.excalidrawlib", 3),
    ("infamousjoeg/cyberark.excalidrawlib", 3),
    ("gabrielamacakova/presentation-bundle.excalidrawlib", 3),
    ("odraghi/vmware-architecture-design.excalidrawlib", 3),
    ("https-github-com-jinmingyi1998/collective-operation.excalidrawlib", 5),
    ("datavizfairy/dashboard-charts.excalidrawlib", 5),
    ("lukethorp/databricks-architecture-icons.excalidrawlib", 5),
    ("timothygalvin/excalidraw-archimate-template.excalidrawlib", 5),
    ("fortijosh/fortinet.excalidrawlib", 5),
    ("mwc360/microsoft-fabric-architecture-icons.excalidrawlib", 5),
    ("ewels/nextflow-seqera-nf-core.excalidrawlib", 5),
    ("simonthomine/pathology.excalidrawlib", 5),
    ("moochin/simple-characters.excalidrawlib", 5),
    ("https-github-com-tomorrowx-dev/tomorrowx-composable-agentic-platform-cap.excalidrawlib", 5),
    ("jordangeurtsen/uml-component-diagram.excalidrawlib", 6),
    ("jordangeurtsen/uml-deployment-diagram.excalidrawlib", 6),
    ("fortijosh/fortinet.excalidrawlib", 7),
    ("moochin/simple-characters.excalidrawlib", 8),
)


def manifest() -> dict[str, dict]:
    data = json.loads((FIXTURES / "manifest.json").read_text())
    return {f["origin_path"]: f for f in data["files"] if f["source"] == "libraries"}


def read_library(entry: dict) -> dict:
    raw = (FIXTURES / entry["path"]).read_bytes()
    if entry.get("encoding") == "gzip":
        raw = gzip.decompress(raw)
    return json.loads(raw)


def elements(library: dict):
    items = library.get("libraryItems") or library.get("library") or []
    for index, item in enumerate(items):
        els = item["elements"] if isinstance(item, dict) else item
        for element in els:
            yield index, element


def eligible(e: dict) -> bool:
    return (
        e.get("type") == "text"
        and "lineHeight" in e
        and e.get("autoResize") is not False
        and e.get("fontFamily") in VENDORED_FAMILIES
    )


def build() -> dict:
    files = manifest()
    cases = []
    for library, family in PAIRS:
        entry = files.get(f"libraries/{library}")
        if entry is None:
            raise SystemExit(f"{library}: not in fixtures/manifest.json")
        found = 0
        for item, e in elements(read_library(entry)):
            if not eligible(e) or e["fontFamily"] != family:
                continue
            found += 1
            cases.append({
                "source": entry["path"],
                "item": item,
                "id": e["id"],
                "fontFamily": e["fontFamily"],
                "fontSize": e["fontSize"],
                "lineHeight": e["lineHeight"],
                "text": e["text"],
                "width": e["width"],
                "height": e["height"],
            })
        if not found:
            raise SystemExit(f"{library}: no eligible text in fontFamily {family}")
    return {
        "description": (
            "Text elements from the ex-003 library corpus with the width and height "
            "upstream stored; generated by scripts/fixtures/text_widths.py."
        ),
        "cases": cases,
    }


def render(data: dict) -> str:
    # ASCII only: library texts carry emoji variation selectors, which the
    # authorship gate rejects in tracked text files.
    return json.dumps(data, indent=1, ensure_ascii=True) + "\n"


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[1] not in ("write", "check"):
        print(__doc__, file=sys.stderr)
        return 2
    data = build()
    text = render(data)
    if argv[1] == "write":
        OUT.write_text(text)
        print(f"wrote {OUT.relative_to(ROOT)} ({len(data['cases'])} cases)")
        return 0
    if not OUT.is_file() or OUT.read_text() != text:
        print(f"{OUT.relative_to(ROOT)} is stale; run scripts/fixtures/text_widths.py write", file=sys.stderr)
        return 1
    print("text-width fixture OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
