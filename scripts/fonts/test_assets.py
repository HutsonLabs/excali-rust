#!/usr/bin/env python3
"""Tests for scripts/fonts/assets.py (ex-307): the committed font assets pass
`check`, and planted problems (a changed, missing or extra file, a stale
manifest or Rust table, a licence that is not the recorded one) are caught.
verify-upstream needs the pinned checkout (scripts/upstream/checkout.sh,
UPSTREAM_DIR honoured); the suite fails without it."""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import assets  # noqa: E402


def _upstream() -> Path:
    r = subprocess.run(
        ["bash", str(assets.ROOT / "scripts" / "upstream" / "checkout.sh"), "--verify"],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        raise AssertionError(f"pinned upstream checkout required: {r.stderr.strip()}")
    return assets._upstream_dir()


class CommittedAssets(unittest.TestCase):
    def test_check_is_clean(self):
        self.assertEqual(assets.check(assets._golden(), assets.ASSETS, assets.GENERATED), [])

    def test_manifest_lists_every_registered_file_once(self):
        m = json.loads(assets.MANIFEST.read_text(encoding="utf-8"))
        files = [f["file"] for fam in m["families"] for f in fam["faces"]]
        self.assertEqual(len(files), len(set(files)))
        self.assertEqual(len(files), 1 + 4 + 7 + 1 + 2 + 5 + 1 + 209)
        self.assertEqual([f["id"] for f in m["families"]], [3, 8, 5, 2, 9, 7, 6, 1, 100, 1000])
        local = {f["family"]: f["faces"] for f in m["families"] if f["local"]}
        self.assertEqual(local, {"Helvetica": [], "Segoe UI Emoji": []})

    def test_every_licence_is_recorded_with_its_hash(self):
        self.assertTrue(all(len(l.sha256) == 64 for l in assets.LICENCES))
        dirs = {p.parent.name for p in assets.ASSETS.rglob("*") if p.suffix in (".woff2", ".ttf")}
        self.assertEqual(dirs, {l.directory for l in assets.LICENCES})

    def test_parse_unicode_range(self):
        self.assertEqual(
            assets.parse_unicode_range("U+0000-00FF, U+0131,U+2000-206F"),
            [(0, 0xFF), (0x131, 0x131), (0x2000, 0x206F)],
        )

    def test_verify_upstream_is_clean(self):
        self.assertEqual(assets.verify_upstream(assets._golden(), assets.ASSETS, _upstream()), [])


class PlantedProblems(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="font-assets-"))
        self.assets = self.tmp / "fonts"
        shutil.copytree(assets.ASSETS, self.assets)
        self.generated = self.tmp / "generated.rs"
        shutil.copyfile(assets.GENERATED, self.generated)
        self.golden = assets._golden()

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def errors(self):
        return assets.check(self.golden, self.assets, self.generated)

    def test_copy_is_clean(self):
        self.assertEqual(self.errors(), [])

    def test_changed_font_file(self):
        f = self.assets / "Excalifont" / "Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"
        f.write_bytes(f.read_bytes() + b"\0")
        errs = self.errors()
        self.assertTrue(any("manifest.json: stale" in e for e in errs), errs)
        self.assertTrue(any("generated.rs: stale" in e for e in errs), errs)

    def test_missing_font_file(self):
        (self.assets / "Nunito" / "Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2").unlink()
        self.assertTrue(any("listed but missing" in e for e in self.errors()))

    def test_extra_file(self):
        (self.assets / "Virgil" / "Virgil-Bold.woff2").write_bytes(b"x")
        self.assertTrue(any("Virgil/Virgil-Bold.woff2: on disk but not in the manifest" in e for e in self.errors()))

    def test_changed_licence(self):
        f = self.assets / "Nunito" / "OFL.txt"
        f.write_text(f.read_text(encoding="utf-8") + "\nextra\n", encoding="utf-8")
        self.assertTrue(any("Nunito/OFL.txt: sha256" in e for e in self.errors()))

    def test_stale_rust_table(self):
        self.generated.write_text(self.generated.read_text(encoding="utf-8").replace('"500"', '"400"', 1))
        self.assertEqual(len(self.errors()), 1)

    def test_registry_change_in_the_golden(self):
        self.golden["registered"][2]["faces"][0]["unicodeRange"] = "U+20-7f"
        errs = self.errors()
        self.assertTrue(any("manifest.json: stale" in e for e in errs), errs)

    def test_verify_upstream_catches_a_changed_copy(self):
        f = self.assets / "Lilita" / "Lilita-Regular-i7dPIFZ9Zz-WBtRtedDbYEF8RXi4EwQ.woff2"
        f.write_bytes(b"not the upstream file")
        errs = assets.verify_upstream(self.golden, self.assets, _upstream())
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("differs from upstream", errs[0])


if __name__ == "__main__":
    unittest.main()
