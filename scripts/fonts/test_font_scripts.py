#!/usr/bin/env python3
"""Tests for scripts/fonts/font_faces.py and the range parser of advances.py
(ex-302), on a synthetic upstream tree."""
from __future__ import annotations

import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def _load(name: str):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


font_faces = _load("font_faces")

METADATA = '''export const GOOGLE_FONTS_RANGES = {
  LATIN:
    "U+0000-00FF, U+0131",
  LATIN_EXT: "U+0100-02AF",
  CYRILIC_EXT: "U+0460-052F",
  CYRILIC: "U+0301, U+0400-045F",
  VIETNAMESE: "U+0102-0103",
};
'''

CSS = '''@font-face {
  font-family: "Assistant";
  src: url(../fonts/Assistant/Assistant-Regular.woff2) format("woff2");
  font-weight: 400;
}
'''


def _write(root: Path, rel: str, text: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class FontFacesTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        fonts = "packages/excalidraw/fonts"
        _write(root, "packages/common/src/font-metadata.ts", METADATA)
        _write(root, f"{fonts}/fonts.css", CSS)
        for family, directory, export in font_faces.FAMILIES:
            _write(root, f"{fonts}/{directory}/index.ts",
                   f'import {{ LOCAL_FONT_PROTOCOL }} from "@excalidraw/common";\n'
                   f"export const {export}: ExcalidrawFontFaceDescriptor[] = [\n"
                   f"  {{\n    uri: LOCAL_FONT_PROTOCOL,\n  }},\n];\n")
        _write(root, f"{fonts}/Nunito/index.ts", '''import { GOOGLE_FONTS_RANGES } from "@excalidraw/common";
import Latin from "./Nunito-Latin.woff2";
import Cyr from "./Nunito-Cyr.woff2";

export const NunitoFontFaces: ExcalidrawFontFaceDescriptor[] = [
  {
    uri: Cyr,
    descriptors: { unicodeRange: GOOGLE_FONTS_RANGES.CYRILIC, weight: "500" },
  },
  {
    uri: Latin,
    descriptors: {
      unicodeRange: GOOGLE_FONTS_RANGES.LATIN,
      weight: "500",
    },
  },
];
''')
        _write(root, f"{fonts}/Excalifont/index.ts", '''import _1 from "./Ex-1.woff2";
import _0 from "./Ex-0.woff2";
export const ExcalifontFontFaces: ExcalidrawFontFaceDescriptor[] = [
  {
    uri: _0,
    descriptors: {
      unicodeRange:
        "U+20-7e,U+a0-a3",
    },
  },
  { uri: _1, descriptors: { unicodeRange: "U+300-301,U+303" } },
];
''')
        _write(root, f"{fonts}/Liberation/index.ts", '''import LiberationSansRegular from "./LiberationSans-Regular.woff2";
export const LiberationFontFaces: ExcalidrawFontFaceDescriptor[] = [
  {
    uri: LiberationSansRegular,
  },
];
''')
        self.root = root

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_descriptors_in_registration_order(self) -> None:
        faces = font_faces.faces(self.root)
        by_family = {}
        for f in faces:
            by_family.setdefault(f["family"], []).append(f)
        self.assertEqual(
            [f["upstream"] for f in by_family["Nunito"]],
            ["Nunito/Nunito-Cyr.woff2", "Nunito/Nunito-Latin.woff2"],
        )
        self.assertEqual(
            [f["range"] for f in by_family["Nunito"]],
            ["U+0301, U+0400-045F", "U+0000-00FF, U+0131"],
        )
        self.assertEqual({f["weight"] for f in by_family["Nunito"]}, {"500"})
        self.assertEqual(
            [(f["upstream"], f["range"]) for f in by_family["Excalifont"]],
            [("Excalifont/Ex-0.woff2", "U+20-7e,U+a0-a3"), ("Excalifont/Ex-1.woff2", "U+300-301,U+303")],
        )
        self.assertEqual(
            [f["family"] for f in faces][-1], "Assistant",
        )
        self.assertEqual(faces[-1]["weight"], "400")
        helvetica = by_family["Helvetica"]
        self.assertEqual(helvetica, [{"family": "Helvetica", "range": None, "weight": None,
                                      "upstream": None, "vendored": None}])

    def test_liberation_maps_to_the_licensed_build(self) -> None:
        faces = font_faces.faces(self.root)
        lib = [f for f in faces if f["family"] == "Liberation Sans"]
        self.assertEqual(lib[0]["upstream"], "Liberation/LiberationSans-Regular.woff2")
        self.assertEqual(lib[0]["vendored"], "Liberation/LiberationSans-Regular.ttf")

    def test_render_is_a_rustfmt_skipped_const(self) -> None:
        text = font_faces.render(font_faces.faces(self.root))
        self.assertIn("#[rustfmt::skip]", text)
        self.assertIn("pub static FONT_FACES: [FontFaceDescriptor; ", text)
        self.assertIn('unicode_range: Some("U+0301, U+0400-045F"), weight: Some("500") },', text)

    def test_missing_descriptor_array_fails(self) -> None:
        _write(self.root, "packages/excalidraw/fonts/Virgil/index.ts", "export const Other = [];\n")
        with self.assertRaises(ValueError):
            font_faces.faces(self.root)


class AdvancesRangeTest(unittest.TestCase):
    def test_parse_range(self) -> None:
        advances = _load("advances")
        self.assertEqual(advances.parse_range(None), [(0, 0x10FFFF)])
        self.assertEqual(advances.parse_range("U+20-7e, U+3bb,U+4??"),
                         [(0x20, 0x7E), (0x3BB, 0x3BB), (0x400, 0x4FF)])
        self.assertTrue(advances.in_range([(1, 3)], 2))
        self.assertFalse(advances.in_range([(1, 3)], 4))


class TextWidthPairsTest(unittest.TestCase):
    """scripts/fixtures/text_widths.py: the fixture's pairs and the recorded
    exclusions together cover the corpus (ex-302 review)."""

    @classmethod
    def setUpClass(cls) -> None:
        spec = importlib.util.spec_from_file_location(
            "text_widths", HERE.parent / "fixtures" / "text_widths.py")
        cls.tw = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.tw)
        cls.corpus = cls.tw.corpus_pairs(cls.tw.manifest())

    def test_pairs_and_exclusions_cover_the_corpus(self) -> None:
        self.assertEqual(len(self.corpus), 51)
        self.assertEqual(sum(self.corpus.values()), 1866)
        self.assertEqual((len(self.tw.PAIRS), len(self.tw.EXCLUDED)), (37, 14))
        self.tw.check_partition(self.corpus)

    def test_baseline_totals_per_family(self) -> None:
        # The docstring's table: eligible texts per fontFamily.
        totals: dict[int, int] = {}
        for (_, family), n in self.corpus.items():
            totals[family] = totals.get(family, 0) + n
        self.assertEqual(totals, {1: 1243, 3: 35, 5: 198, 6: 348, 7: 3, 8: 39})

    def test_an_unlisted_pair_fails(self) -> None:
        corpus = dict(self.corpus)
        corpus[("someone/new.excalidrawlib", 1)] = 4
        with self.assertRaises(SystemExit):
            self.tw.check_partition(corpus)

    def test_a_changed_excluded_count_fails(self) -> None:
        corpus = dict(self.corpus)
        key = next(iter(self.tw.EXCLUDED))
        corpus[key] += 1
        with self.assertRaises(SystemExit):
            self.tw.check_partition(corpus)


if __name__ == "__main__":
    os.chdir(HERE)
    unittest.main()
