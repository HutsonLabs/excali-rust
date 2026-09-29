#!/usr/bin/env python3
"""Tests for contact_sheets.py (ex-410)."""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import contact_sheets  # noqa: E402


class ContactSheetsTest(unittest.TestCase):
    def test_the_catalogue_is_the_232_libraries_of_the_manifest(self):
        manifest = json.loads((contact_sheets.FIXTURES / "manifest.json").read_text())
        paths = contact_sheets.catalogue(manifest)
        self.assertEqual(len(paths), 232)
        self.assertTrue(all(p.startswith("libraries/") for p in paths))
        self.assertEqual(len(set(map(contact_sheets.sheet_name, paths))), 232)

    def test_sheet_names_keep_the_author_directory(self):
        self.assertEqual(
            contact_sheets.sheet_name("libraries/childishgirl/aws-architecture-icons.excalidrawlib.gz"),
            "childishgirl/aws-architecture-icons.png",
        )

    def test_the_index_lists_failures_first_and_escapes(self):
        page = contact_sheets.index_html(
            [
                {"path": "libraries/a/ok.excalidrawlib.gz", "sheet": "a/ok.png", "items": [{}, {}]},
                {"path": "libraries/b/<bad>.excalidrawlib.gz", "error": "exit 4: x & y"},
            ]
        )
        self.assertIn("2 libraries, 2 items, 1 failed", page)
        self.assertLess(page.index("&lt;bad&gt;"), page.index("a/ok.png"))
        self.assertIn("x &amp; y", page)
        self.assertIn('<img src="a/ok.png"', page)


if __name__ == "__main__":
    unittest.main()
