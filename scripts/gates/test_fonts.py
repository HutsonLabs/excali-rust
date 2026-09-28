#!/usr/bin/env python3
"""Tests for scripts/gates/fonts.py (font licences, ADR-004, task ex-306).

The gate reads the verification table and the licence gaps of ADR-004 and
checks every font file in the tree against them: a font is vendored only if
its family's licence is confirmed as SIL OFL 1.1, MIT or Apache 2.0 with a
source URL and a date, the licence text sits next to the file, and the file
is not one the ADR lists as not vendorable.

Run: python3 scripts/gates/test_fonts.py
"""
from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import fonts  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
ADR = ROOT / "site" / "content" / "decisions" / "adr-004-fonts.md"

# sha256 of upstream packages/excalidraw/fonts/Liberation/LiberationSans-Regular.woff2
# at the pinned commit: Liberation Sans 1.05 (Ascender, 2007), whose name table
# licence is "subject to the license agreement under which you accepted the
# Liberation font software" (GPLv2 with the font exception), not OFL/MIT/Apache.
LIBERATION_105_SHA256 = "006a2b28cbbeeaec937d1b367d0949f5f48ef4b2e7b4e1d8dc7a799d2d639be8"
# sha256 of LiberationSans-Regular.ttf in liberation-fonts-ttf-2.1.5.tar.gz
# (OFL 1.1), the build ADR-004 vendors instead.
LIBERATION_215_SHA256 = "76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8"
# Stand-in bytes for the recorded 2.1.5 build in the synthetic ADR below.
LIB_215_BYTES = b"\x02font"

OFL_TEXT = (
    "Copyright 2014 The Nunito Project Authors\n\n"
    "This Font Software is licensed under the SIL Open Font License, Version 1.1.\n"
    "SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007\n"
)
MIT_TEXT = (
    "MIT License\n\nCopyright (c) 2018 Shannon Miwa\n\n"
    "Permission is hereby granted, free of charge, to any person obtaining a copy\n"
)

GOOD_ADR = """
## Verification table

| Family | Id | Licence | Source | Checked | Evidence | Vendored from |
|---|---|---|---|---|---|---|
| Virgil | 1 | SIL OFL 1.1 | <https://github.com/excalidraw/virgil/blob/main/LICENSE.md> | 2026-09-28 | name table | upstream `fonts/Virgil/` |
| Helvetica | 2 | local only | n/a | 2026-09-28 | `local:` upstream | never vendored |
| Nunito | 6 | SIL OFL 1.1 | <https://github.com/google/fonts/blob/main/ofl/nunito/OFL.txt> | 2026-09-28 | OFL.txt | upstream `fonts/Nunito/` |
| Comic Shanns | 8 | MIT | <https://github.com/jesusmgg/comic-shanns-mono/blob/master/LICENSE.md> | 2026-09-28 | LICENSE.md | upstream `fonts/ComicShanns/` |
| Liberation Sans | 9 | SIL OFL 1.1 | <https://github.com/liberationfonts/liberation-fonts/blob/main/LICENSE> | 2026-09-28 | 2.1.5 | liberation-fonts 2.1.5 |

## Licence gaps

| Family | Not vendored | sha256 | Fallback | Fallback licence |
|---|---|---|---|---|
| Liberation Sans | upstream `fonts/Liberation/LiberationSans-Regular.woff2` (1.05) | `{sha}` | Liberation Sans 2.1.5 | SIL OFL 1.1 |

## Vendored builds

| Family | File | sha256 | Derived from |
|---|---|---|---|
| Liberation Sans | `LiberationSans-Regular.ttf` 2.1.5 | `{lib}` | liberation-fonts 2.1.5 |
""".replace("{sha}", "ab" * 32).replace("{lib}", hashlib.sha256(LIB_215_BYTES).hexdigest())


def write(path: Path, data: bytes | str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(data, str):
        data = data.encode()
    path.write_bytes(data)
    return path


class ParseTest(unittest.TestCase):
    def test_families_parse(self):
        fams = fonts.families(GOOD_ADR)
        self.assertEqual(
            [f.name for f in fams],
            ["Virgil", "Helvetica", "Nunito", "Comic Shanns", "Liberation Sans"],
        )
        nunito = fams[2]
        self.assertEqual(nunito.licence, "SIL OFL 1.1")
        self.assertEqual(nunito.source, "https://github.com/google/fonts/blob/main/ofl/nunito/OFL.txt")
        self.assertEqual(nunito.checked, "2026-09-28")
        self.assertEqual(nunito.id, "6")
        self.assertTrue(nunito.confirmed)
        self.assertFalse(fams[1].confirmed)

    def test_gaps_parse(self):
        gaps = fonts.gaps(GOOD_ADR)
        self.assertEqual(len(gaps), 1)
        self.assertEqual(gaps[0].family, "Liberation Sans")
        self.assertEqual(gaps[0].sha256, "ab" * 32)
        self.assertEqual(gaps[0].fallback_licence, "SIL OFL 1.1")

    def test_builds_parse(self):
        builds = fonts.builds(GOOD_ADR)
        self.assertEqual(len(builds), 1)
        self.assertEqual(builds[0].family, "Liberation Sans")
        self.assertEqual(builds[0].sha256, hashlib.sha256(LIB_215_BYTES).hexdigest())

    def test_missing_table_is_an_error(self):
        errs = fonts.check_adr("# nothing here\n", required=())
        self.assertTrue(any("Verification table" in e for e in errs), errs)


class CheckAdrTest(unittest.TestCase):
    REQ = ("Virgil", "Helvetica", "Nunito", "Comic Shanns", "Liberation Sans")

    def test_good_adr_is_clean(self):
        self.assertEqual(fonts.check_adr(GOOD_ADR, required=self.REQ), [])

    def test_required_family_missing(self):
        errs = fonts.check_adr(GOOD_ADR, required=self.REQ + ("Lilita One",))
        self.assertTrue(any("Lilita One" in e and "missing" in e for e in errs), errs)

    def test_licence_outside_allowed_set(self):
        bad = GOOD_ADR.replace("| Nunito | 6 | SIL OFL 1.1 |", "| Nunito | 6 | GPL-2.0 |")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Nunito" in e and "GPL-2.0" in e for e in errs), errs)

    def test_confirmed_needs_https_source(self):
        bad = GOOD_ADR.replace(
            "<https://github.com/google/fonts/blob/main/ofl/nunito/OFL.txt>", "upstream repo"
        )
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Nunito" in e and "source URL" in e for e in errs), errs)

    def test_confirmed_needs_date(self):
        bad = GOOD_ADR.replace("| MIT | <https://github.com/jesusmgg/comic-shanns-mono/blob/master/LICENSE.md> | 2026-09-28 |",
                               "| MIT | <https://github.com/jesusmgg/comic-shanns-mono/blob/master/LICENSE.md> | soon |")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Comic Shanns" in e and "date" in e for e in errs), errs)

    def test_pending_family_must_be_listed_as_gap(self):
        bad = GOOD_ADR.replace("| Nunito | 6 | SIL OFL 1.1 |", "| Nunito | 6 | pending |")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Nunito" in e and "Licence gaps" in e for e in errs), errs)

    def test_pending_family_listed_as_gap_is_fine(self):
        bad = GOOD_ADR.replace("| Nunito | 6 | SIL OFL 1.1 |", "| Nunito | 6 | pending |")
        bad = bad.replace(
            "| SIL OFL 1.1 |\n\n## Vendored builds",
            "| SIL OFL 1.1 |\n| Nunito | all files | | host `sans-serif` (nothing shipped) | n/a |\n\n## Vendored builds",
        )
        self.assertEqual(fonts.check_adr(bad, required=self.REQ), [])

    def test_gap_fallback_licence_must_be_allowed(self):
        bad = GOOD_ADR.replace("| Liberation Sans 2.1.5 | SIL OFL 1.1 |", "| Arial | proprietary |")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("fallback" in e and "proprietary" in e for e in errs), errs)

    def test_confirmed_family_with_a_file_gap_needs_a_vendored_build(self):
        # Liberation Sans is confirmed but one build is a gap: without a
        # recorded build the gate cannot tell a licensed file from the gap.
        bad = GOOD_ADR.split("## Vendored builds")[0]
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Liberation Sans" in e and "Vendored builds" in e for e in errs), errs)

    def test_vendored_build_sha256_must_be_hex(self):
        bad = GOOD_ADR.replace(hashlib.sha256(LIB_215_BYTES).hexdigest(), "nope")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("Liberation Sans" in e and "sha256" in e for e in errs), errs)

    def test_gap_sha256_must_be_hex(self):
        bad = GOOD_ADR.replace("ab" * 32, "not-a-hash")
        errs = fonts.check_adr(bad, required=self.REQ)
        self.assertTrue(any("sha256" in e for e in errs), errs)


class CheckTreeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_empty_tree_is_clean(self):
        self.assertEqual(fonts.check_tree(self.root, GOOD_ADR), [])

    def test_confirmed_family_with_licence_file_is_clean(self):
        write(self.root / "assets/fonts/Nunito/Nunito-Regular-latin.woff2", b"\x00font")
        write(self.root / "assets/fonts/Nunito/OFL.txt", OFL_TEXT)
        write(self.root / "assets/fonts/ComicShanns/ComicShanns-Regular-1.woff2", b"\x01font")
        write(self.root / "assets/fonts/ComicShanns/LICENSE.md", MIT_TEXT)
        write(self.root / "assets/fonts/Liberation/LiberationSans-Regular.ttf", LIB_215_BYTES)
        write(self.root / "assets/fonts/Liberation/LICENSE", OFL_TEXT)
        self.assertEqual(fonts.check_tree(self.root, GOOD_ADR), [])

    def test_font_without_licence_file(self):
        write(self.root / "assets/fonts/Nunito/Nunito-Regular.woff2", b"\x00font")
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertTrue(any("Nunito-Regular.woff2" in e and "licence file" in e for e in errs), errs)

    def test_licence_file_of_the_wrong_licence(self):
        write(self.root / "assets/fonts/Nunito/Nunito-Regular.woff2", b"\x00font")
        write(self.root / "assets/fonts/Nunito/LICENSE", MIT_TEXT)
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertTrue(any("Nunito-Regular.woff2" in e and "SIL OFL 1.1" in e for e in errs), errs)

    def test_font_of_unknown_family(self):
        write(self.root / "assets/fonts/Arial/Arial.ttf", b"\x00font")
        write(self.root / "assets/fonts/Arial/LICENSE", OFL_TEXT)
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertTrue(any("Arial.ttf" in e and "no family" in e for e in errs), errs)

    def test_font_of_unconfirmed_family(self):
        write(self.root / "assets/fonts/Helvetica/Helvetica.ttf", b"\x00font")
        write(self.root / "assets/fonts/Helvetica/LICENSE", OFL_TEXT)
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertTrue(any("Helvetica.ttf" in e and "not confirmed" in e for e in errs), errs)

    def test_file_listed_as_gap_is_rejected_by_hash(self):
        data = b"liberation 1.05"
        adr = GOOD_ADR.replace("ab" * 32, hashlib.sha256(data).hexdigest())
        write(self.root / "assets/fonts/Liberation/LiberationSans-Regular.woff2", data)
        write(self.root / "assets/fonts/Liberation/LICENSE", OFL_TEXT)
        errs = fonts.check_tree(self.root, adr)
        self.assertTrue(any("LiberationSans-Regular.woff2" in e and "Licence gaps" in e for e in errs), errs)

    def test_reencoded_gap_build_is_rejected(self):
        # The 1.05 file converted to ttf, re-subset or re-compressed has a new
        # hash, sits under Liberation/ (a confirmed family) with an OFL.txt:
        # it is still not a recorded Liberation Sans build, so it fails.
        write(self.root / "assets/fonts/Liberation/LiberationSans-Regular.ttf", b"liberation 1.05 as ttf")
        write(self.root / "assets/fonts/Liberation/OFL.txt", OFL_TEXT)
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertTrue(
            any("LiberationSans-Regular.ttf" in e and "Vendored builds" in e for e in errs), errs
        )

    def test_recorded_build_of_a_gap_family_is_clean(self):
        write(self.root / "assets/fonts/Liberation/LiberationSans-Regular.woff2", LIB_215_BYTES)
        write(self.root / "assets/fonts/Liberation/OFL.txt", OFL_TEXT)
        self.assertEqual(fonts.check_tree(self.root, GOOD_ADR), [])

    def test_family_without_file_gap_needs_no_recorded_build(self):
        write(self.root / "assets/fonts/Nunito/Nunito-Regular-any-bytes.woff2", b"any subset")
        write(self.root / "assets/fonts/Nunito/OFL.txt", OFL_TEXT)
        self.assertEqual(fonts.check_tree(self.root, GOOD_ADR), [])

    def test_unlisted_prefix_directory_is_rejected(self):
        # Directories map to families through an explicit table of upstream's
        # directory names; a prefix or a family's display name is not enough.
        for d in ("L", "Lib", "C", "A", "N", "Nun", "Comic", "Cascadia Code", "Liberation Sans", "nunito"):
            with self.subTest(directory=d):
                with tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    write(root / "assets/fonts" / d / "Font.woff2", b"\x00font")
                    write(root / "assets/fonts" / d / "OFL.txt", OFL_TEXT)
                    errs = fonts.check_tree(root, GOOD_ADR)
                    self.assertTrue(any("Font.woff2" in e and "no family" in e for e in errs), errs)

    def test_upstream_directory_names_map_to_families(self):
        self.assertEqual(fonts.DIRECTORY_FAMILIES, {
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
        })
        self.assertEqual(set(fonts.DIRECTORY_FAMILIES.values()), set(fonts.UPSTREAM_FAMILIES))

    def test_ignored_directories(self):
        for d in (".git", "target", ".tools", "node_modules", ".hidden"):
            write(self.root / d / "x/Unknown.ttf", b"\x00")
        self.assertEqual(fonts.check_tree(self.root, GOOD_ADR), [])

    def test_all_font_extensions_are_scanned(self):
        # distinct stems: macOS file systems are case-insensitive
        for i, ext in enumerate((".woff2", ".woff", ".ttf", ".otf", ".TTF")):
            write(self.root / f"f/Unknown/a{i}{ext}", b"\x00")
        errs = fonts.check_tree(self.root, GOOD_ADR)
        self.assertEqual(len(errs), 5, errs)


class RepositoryTest(unittest.TestCase):
    """ADR-004 as committed: every upstream family verified (ex-306)."""

    def setUp(self):
        self.text = ADR.read_text(encoding="utf-8")

    def test_adr_passes_the_gate(self):
        self.assertEqual(fonts.check_adr(self.text), [])

    def test_repository_tree_passes_the_gate(self):
        self.assertEqual(fonts.check_tree(ROOT, self.text), [])

    def test_every_upstream_family_is_listed(self):
        # Fonts.init() (packages/excalidraw/fonts/Fonts.ts:375-416), the UI
        # font Assistant (fonts.css) and FONT_FAMILY (common/src/constants.ts:140-167).
        names = {f.name for f in fonts.families(self.text)}
        self.assertEqual(set(fonts.UPSTREAM_FAMILIES), names)
        self.assertEqual(
            set(fonts.UPSTREAM_FAMILIES),
            {"Virgil", "Helvetica", "Cascadia Code", "Excalifont", "Nunito", "Lilita One",
             "Comic Shanns", "Liberation Sans", "Assistant", "Xiaolai", "Segoe UI Emoji"},
        )

    def test_licences_recorded_per_family(self):
        got = {f.name: (f.id, f.licence) for f in fonts.families(self.text)}
        self.assertEqual(got, {
            "Virgil": ("1", "SIL OFL 1.1"),
            "Helvetica": ("2", "local only"),
            "Cascadia Code": ("3", "SIL OFL 1.1"),
            "Excalifont": ("5", "SIL OFL 1.1"),
            "Nunito": ("6", "SIL OFL 1.1"),
            "Lilita One": ("7", "SIL OFL 1.1"),
            "Comic Shanns": ("8", "MIT"),
            "Liberation Sans": ("9", "SIL OFL 1.1"),
            "Assistant": ("10", "SIL OFL 1.1"),
            "Xiaolai": ("100", "SIL OFL 1.1"),
            "Segoe UI Emoji": ("1000", "local only"),
        })

    def test_every_confirmed_family_checked_2026_09_28_at_https_source(self):
        for f in fonts.families(self.text):
            if f.confirmed:
                self.assertEqual(f.checked, "2026-09-28", f.name)
                self.assertTrue(f.source.startswith("https://"), f.name)
                self.assertNotIn("excalidraw/excalidraw/", f.source, f.name)

    def test_cascadia_row_quotes_both_parts_of_name_id_13(self):
        # CascadiaCode-Regular.woff2 at the pin: name ID 13 opens with
        # Microsoft's product-font notice ("Any other use is prohibited") and
        # continues with the OFL-based grant and conditions 1-5. The row must
        # quote both and say why the first does not stop redistribution.
        row = next(ln for ln in self.text.splitlines() if ln.startswith("| Cascadia Code |"))
        for needle in (
            "Microsoft supplied font",
            "You may only (i) embed this font",
            "(ii) temporarily download this font to a printer",
            "Any other use is prohibited.",
            "The following license, based on the SIL Open Font license (https://scripts.sil.org/OFL), applies to this font",
            "to use, study, copy, merge, embed, modify, redistribute, and sell",
            "conditions 1-5",
            "does not stop redistribution",
            "LICENSE at tag v2005.15",
        ):
            self.assertIn(needle, row)
        self.assertNotIn("names no licence of its own", row)

    def test_liberation_215_is_the_recorded_build(self):
        got = {(b.family, b.sha256) for b in fonts.builds(self.text)}
        self.assertEqual(got, {("Liberation Sans", LIBERATION_215_SHA256)})

    def test_liberation_105_is_a_gap_with_its_hash(self):
        gaps = {g.family: g for g in fonts.gaps(self.text)}
        self.assertIn("Liberation Sans", gaps)
        lib = gaps["Liberation Sans"]
        self.assertEqual(lib.sha256, LIBERATION_105_SHA256)
        self.assertIn("2.1.5", lib.fallback)
        self.assertEqual(lib.fallback_licence, "SIL OFL 1.1")


class VerifyUpstreamTest(unittest.TestCase):
    """`fonts.py verify-upstream DIR`: each gap's upstream file hashes as recorded."""

    ADR = """
## Licence gaps

| Family | Not vendored | sha256 | Fallback | Fallback licence |
|---|---|---|---|---|
| Liberation Sans | upstream `fonts/Liberation/LiberationSans-Regular.woff2` (1.05) | `{sha}` | Liberation Sans 2.1.5 | SIL OFL 1.1 |
"""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.up = Path(self.tmp.name)
        self.data = b"liberation 1.05 bytes"
        write(self.up / "packages/excalidraw/fonts/Liberation/LiberationSans-Regular.woff2", self.data)
        for d in fonts.DIRECTORY_FAMILIES:
            (self.up / "packages/excalidraw/fonts" / d).mkdir(parents=True, exist_ok=True)

    def test_every_mapped_directory_exists_upstream(self):
        adr = self.ADR.replace("{sha}", hashlib.sha256(self.data).hexdigest())
        (self.up / "packages/excalidraw/fonts/Lilita").rmdir()
        errs = fonts.verify_upstream(self.up, adr)
        self.assertTrue(any("Lilita" in e and "directory" in e for e in errs), errs)

    def tearDown(self):
        self.tmp.cleanup()

    def test_matching_hash(self):
        adr = self.ADR.replace("{sha}", hashlib.sha256(self.data).hexdigest())
        self.assertEqual(fonts.verify_upstream(self.up, adr), [])

    def test_changed_file(self):
        adr = self.ADR.replace("{sha}", "cd" * 32)
        errs = fonts.verify_upstream(self.up, adr)
        self.assertTrue(any("LiberationSans-Regular.woff2" in e and "sha256" in e for e in errs), errs)

    def test_missing_file(self):
        adr = self.ADR.replace("{sha}", "cd" * 32).replace("LiberationSans-Regular", "Gone")
        errs = fonts.verify_upstream(self.up, adr)
        self.assertTrue(any("Gone.woff2" in e and "missing" in e for e in errs), errs)

    def test_gap_without_upstream_path(self):
        adr = self.ADR.replace("upstream `fonts/Liberation/LiberationSans-Regular.woff2` (1.05)", "all files")
        adr = adr.replace("{sha}", "")
        self.assertEqual(fonts.verify_upstream(self.up, adr), [])


if __name__ == "__main__":
    unittest.main()
