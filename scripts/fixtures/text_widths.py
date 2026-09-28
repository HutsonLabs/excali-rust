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
PAIRS. The pairs were selected on the outcome: a pair was kept only if every
one of its eligible texts measured within 0.5 px with the port's FontStore
(the vendored fonts, shaped with rustybuzz). Nothing independent of that
measurement establishes that these texts were measured with the font builds
the port vendors, or in a browser that shapes with kerning; the fixture shows
that the port reproduces these widths, not that it reproduces the corpus.

The whole-corpus baseline, measured on 2026-09-28 with the same eligibility
rule (1866 eligible texts in 51 (library, fontFamily) pairs):

  fontFamily        within 0.5 px   max |deviation|
  1 Virgil           1149 / 1243     116.7 px
  3 Cascadia           34 / 35         0.58
  5 Excalifont        181 / 198        2.20
  6 Nunito             46 / 348       47.5
  7 Lilita One          2 / 3          0.89
  8 Comic Shanns       38 / 39         0.52

37 of the 51 pairs pass in full and are PAIRS; the other 14 are EXCLUDED
below, each with its pass count. Of the 416 texts outside 0.5 px, 363 measure
narrower than their stored width (39 of them by an exact 1, 2, 3 or 5 px,
consistent with widths edited by hand or kept from an older font build) and
53 wider, all 53 in the Nunito texts of datavizfairy/dashboard-charts, a
generated library. The whole-corpus gate and its per-family report are
ex-308's (crates/excali-text/tests/text_width_corpus.rs); `check` fails if
PAIRS and EXCLUDED stop covering every pair with eligible texts or an
excluded pair's text count changes.

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

# (library, fontFamily) -> (texts within 0.5 px, eligible texts, max
# |deviation| in px): the pairs left out of the fixture because at least one
# eligible text measured outside 0.5 px (FontStore, 2026-09-28).
EXCLUDED = {
    ("childishgirl/aws-architecture-icons.excalidrawlib", 1): (222, 262, 3.00),
    ("erlina/data-processing.excalidrawlib", 1): (8, 9, 2.59),
    ("gabrielamacakova/halloween-elements.excalidrawlib", 1): (2, 6, 0.73),
    ("hartmut-co-uk/kafka-streams-topology-design.excalidrawlib", 1): (57, 65, 2.13),
    ("infamousjoeg/cyberark.excalidrawlib", 1): (11, 25, 2.30),
    ("pratheeshpm/basic-system-design.excalidrawlib", 1): (34, 47, 116.74),
    ("stojanovic/aws-serverless-icons-v2.excalidrawlib", 1): (10, 24, 1.46),
    # `{}` stores its width at 14 px, Math.round of its fontSize (ex-g302,
    # KNOWN_DEVIATIONS in crates/excali-text/tests/text_width_corpus.rs).
    ("childishgirl/aws-architecture-icons.excalidrawlib", 3): (6, 7, 0.58),
    ("childishgirl/aws-architecture-icons.excalidrawlib", 5): (0, 1, 1.00),
    ("hartmut-co-uk/kafka-streams-topology-design.excalidrawlib", 5): (27, 31, 1.02),
    ("martinberger-ch/oracle-cloud-infrastructure-icons.excalidrawlib", 5): (31, 43, 2.20),
    ("datavizfairy/dashboard-charts.excalidrawlib", 6): (38, 340, 47.51),
    # `Metrics` stores 3.3345 em; Lilita One measures 3.304 em at every size
    # (ex-g302, KNOWN_DEVIATIONS in text_width_corpus.rs).
    ("devdaejungyoon/github-actions.excalidrawlib", 7): (1, 2, 0.89),
    ("hartmut-co-uk/kafka-streams-topology-design.excalidrawlib", 8): (12, 13, 0.52),
}


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


def corpus_pairs(files: dict[str, dict]) -> dict[tuple[str, int], int]:
    """Every (library, fontFamily) pair of the corpus with eligible texts,
    and how many."""
    pairs: dict[tuple[str, int], int] = {}
    for origin, entry in sorted(files.items()):
        if not origin.endswith(".excalidrawlib"):
            continue
        library = origin[len("libraries/"):]
        for _, e in elements(read_library(entry)):
            if isinstance(e, dict) and eligible(e):
                key = (library, e["fontFamily"])
                pairs[key] = pairs.get(key, 0) + 1
    return pairs


def check_partition(corpus: dict[tuple[str, int], int]) -> None:
    """PAIRS and EXCLUDED together are every pair with eligible texts, once,
    and each excluded pair still has the number of texts recorded."""
    kept = set(PAIRS)
    excluded = set(EXCLUDED)
    problems = []
    if len(kept) != len(PAIRS):
        problems.append("PAIRS lists a pair twice")
    if kept & excluded:
        problems.append(f"in both PAIRS and EXCLUDED: {sorted(kept & excluded)}")
    missing = set(corpus) - kept - excluded
    if missing:
        problems.append(f"pairs with eligible texts in neither PAIRS nor EXCLUDED: {sorted(missing)}")
    gone = (kept | excluded) - set(corpus)
    if gone:
        problems.append(f"pairs without eligible texts: {sorted(gone)}")
    for key, (passed, total, _) in EXCLUDED.items():
        if key in corpus and corpus[key] != total:
            problems.append(f"{key}: {corpus[key]} eligible texts, EXCLUDED records {total}")
        if not 0 <= passed < total:
            problems.append(f"{key}: an excluded pair must have a failing text ({passed}/{total})")
    if problems:
        raise SystemExit("\n".join(problems))


def build() -> dict:
    files = manifest()
    check_partition(corpus_pairs(files))
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
