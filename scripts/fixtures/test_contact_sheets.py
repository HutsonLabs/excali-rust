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


    def ok(self, n, items=1):
        return {"path": f"libraries/a/{n}.excalidrawlib.gz", "sheet": f"a/{n}.png", "items": [{}] * items}

    def test_a_full_catalogue_with_items_is_not_a_problem(self):
        entries = [self.ok(i) for i in range(contact_sheets.EXPECTED_LIBRARIES)]
        self.assertEqual(contact_sheets.problems(entries), [])

    def test_rendering_no_libraries_is_a_problem(self):
        found = contact_sheets.problems([])
        self.assertTrue(any("0 libraries rendered, expected 232" in p for p in found), found)
        self.assertTrue(any("no items rendered" in p for p in found), found)

    def test_a_short_catalogue_is_a_problem(self):
        entries = [self.ok(i) for i in range(5)]
        self.assertEqual(contact_sheets.problems(entries), ["5 libraries rendered, expected 232"])

    def test_a_library_with_no_items_is_a_problem(self):
        entries = [self.ok(i) for i in range(contact_sheets.EXPECTED_LIBRARIES)]
        entries[3] = self.ok(3, items=0)
        self.assertEqual(
            contact_sheets.problems(entries), ["libraries/a/3.excalidrawlib.gz: no items rendered"]
        )

    def test_a_failed_library_is_a_problem(self):
        entries = [self.ok(i) for i in range(contact_sheets.EXPECTED_LIBRARIES)]
        entries[0] = {"path": "libraries/a/0.excalidrawlib.gz", "error": "exit 4: boom"}
        self.assertEqual(
            contact_sheets.problems(entries), ["libraries/a/0.excalidrawlib.gz: exit 4: boom"]
        )

    def test_main_fails_when_the_manifest_yields_no_libraries(self):
        import tempfile
        from unittest import mock

        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(
            contact_sheets, "catalogue", return_value=[]
        ):
            code = contact_sheets.main(["--excali", "/nonexistent/excali", "--out", tmp])
            self.assertEqual(code, 1)
            self.assertEqual(json.loads((Path(tmp) / "report.json").read_text()), [])

if __name__ == "__main__":
    unittest.main()
