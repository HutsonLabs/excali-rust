#!/usr/bin/env python3
"""Tests for the fixture corpus and scripts/fixtures/corpus.py (task ex-003).

Two groups:

* CorpusOnDiskTests pin the committed corpus: fixtures/manifest.json lists
  every file under fixtures/ with its sha256 and origin, the files on disk
  match it byte for byte, the upstream copies equal the pinned upstream
  checkout, and the library catalogue is complete (232 entries on
  2026-09-28: 71 version 1, 161 version 2).
* SyncAndCheckTests drive corpus.py against a throwaway fake upstream
  checkout and a fake excalidraw-libraries tree served over file://, so the
  sync and check logic is pinned without the network.

The upstream copy test needs the pinned checkout; run
scripts/upstream/checkout.sh first (CI does).

Run: python3 scripts/fixtures/test_corpus.py
"""
import collections
import gzip
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))

import corpus  # noqa: E402

FIXTURES = ROOT / "fixtures"
CONFIG = ROOT / "site" / "config.toml"


def config_value(key: str) -> str:
    m = re.search(rf'^{key}\s*=\s*"([^"]*)"', CONFIG.read_text(), re.M)
    assert m, f"site/config.toml has no extra.{key}"
    return m.group(1)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def upstream_dir() -> Path:
    out = subprocess.run(
        ["bash", str(ROOT / "scripts" / "upstream" / "checkout.sh"), "--print-dir"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return Path(out)


# ---------------------------------------------------------------------------
# The committed corpus
# ---------------------------------------------------------------------------


class CorpusOnDiskTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((FIXTURES / "manifest.json").read_text(encoding="utf-8"))
        cls.entries = {e["path"]: e for e in cls.manifest["files"]}

    def test_manifest_matches_disk(self):
        # The ex-003 acceptance check.
        self.assertEqual(corpus.check(FIXTURES, corpus.config_pins(CONFIG)), [])

    def test_every_file_has_sha256_and_origin(self):
        on_disk = sorted(
            p.relative_to(FIXTURES).as_posix()
            for p in FIXTURES.rglob("*")
            if p.is_file() and p != FIXTURES / "manifest.json"
        )
        self.assertEqual(sorted(self.entries), on_disk)
        for e in self.manifest["files"]:
            self.assertRegex(e["sha256"], r"^[0-9a-f]{64}$", e["path"])
            self.assertTrue(e["origin"].startswith("https://"), e["path"])
            self.assertIn(e["source"], ("upstream", "libraries"), e["path"])

    def test_sources_are_pinned_to_config(self):
        self.assertEqual(self.manifest["sources"]["upstream"]["commit"], config_value("upstream_commit"))
        self.assertEqual(self.manifest["sources"]["upstream"]["repo"], config_value("upstream"))
        self.assertEqual(self.manifest["sources"]["libraries"]["commit"], config_value("libraries_commit"))
        self.assertEqual(self.manifest["sources"]["libraries"]["repo"], config_value("libraries"))

    def test_upstream_fixture_set(self):
        # data-model.md section 9: every file in tests/fixtures plus the
        # restore/reconcile tests and the restore snapshot.
        up = {e["origin_path"] for e in self.manifest["files"] if e["source"] == "upstream"}
        expected = {
            "LICENSE",
            "packages/excalidraw/tests/fixtures/constants.ts",
            "packages/excalidraw/tests/fixtures/deer.png",
            "packages/excalidraw/tests/fixtures/diagramFixture.ts",
            "packages/excalidraw/tests/fixtures/elementFixture.ts",
            "packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib",
            "packages/excalidraw/tests/fixtures/smiley.png",
            "packages/excalidraw/tests/fixtures/smiley_embedded_v2.png",
            "packages/excalidraw/tests/fixtures/smiley_embedded_v2.svg",
            "packages/excalidraw/tests/fixtures/svg-image-exporting-reference.svg",
            "packages/excalidraw/tests/fixtures/test_embedded_v1.png",
            "packages/excalidraw/tests/fixtures/test_embedded_v1.svg",
            "packages/excalidraw/tests/data/restore.test.ts",
            "packages/excalidraw/tests/data/reconcile.test.ts",
            "packages/excalidraw/tests/data/__snapshots__/restore.test.ts.snap",
            "packages/excalidraw/tests/export.test.tsx",
            "packages/excalidraw/tests/__snapshots__/export.test.tsx.snap",
            "packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap",
        }
        self.assertEqual(up, expected)
        for p in expected:
            self.assertIn(f"upstream/{p}", self.entries)

    def test_upstream_copies_equal_pinned_checkout(self):
        src = upstream_dir()
        self.assertTrue(
            (src / "packages").is_dir(),
            f"no upstream checkout at {src}; run scripts/upstream/checkout.sh",
        )
        self.assertEqual(corpus.verify_upstream(FIXTURES, src), [])
        listed = corpus.upstream_file_list(src)
        self.assertEqual(
            sorted(listed),
            sorted(e["origin_path"] for e in self.manifest["files"] if e["source"] == "upstream"),
        )

    def test_library_catalogue_counts(self):
        catalogue = json.loads((FIXTURES / "libraries" / "libraries.json").read_text(encoding="utf-8"))
        self.assertEqual(len(catalogue), 232)
        versions = collections.Counter(item["version"] for item in catalogue)
        self.assertEqual(versions, {1: 71, 2: 161})
        sources = [item["source"] for item in catalogue]
        self.assertEqual(len(set(sources)), 232)
        lib_entries = {
            e["origin_path"]: e for e in self.manifest["files"]
            if e["source"] == "libraries" and e["origin_path"].endswith(".excalidrawlib")
        }
        self.assertEqual(sorted(lib_entries), sorted(f"libraries/{s}" for s in sources))

    def test_every_library_decodes_to_an_excalidrawlib(self):
        catalogue = json.loads((FIXTURES / "libraries" / "libraries.json").read_text(encoding="utf-8"))
        file_versions = collections.Counter()
        for item in catalogue:
            data = corpus.read_fixture(FIXTURES, f"libraries/{item['source']}")
            doc = json.loads(data.decode("utf-8"))
            self.assertEqual(doc["type"], "excalidrawlib", item["source"])
            # v1 files carry `library`, v2 files `libraryItems`.
            key = "library" if doc["version"] == 1 else "libraryItems"
            self.assertIsInstance(doc[key], list, item["source"])
            file_versions[doc["version"]] += 1
        self.assertEqual(sum(file_versions.values()), 232)

    def test_libraries_are_stored_gzip_with_content_hash(self):
        for e in self.manifest["files"]:
            if e["source"] != "libraries" or not e["origin_path"].endswith(".excalidrawlib"):
                continue
            self.assertEqual(e["encoding"], "gzip", e["path"])
            self.assertTrue(e["path"].endswith(".excalidrawlib.gz"), e["path"])
            raw = gzip.decompress((FIXTURES / e["path"]).read_bytes())
            self.assertEqual(sha256(raw), e["content_sha256"], e["path"])
            self.assertEqual(len(raw), e["content_bytes"], e["path"])

    def test_manifest_is_canonical(self):
        # Regenerating must not reorder or reformat: sorted paths, stable JSON.
        text = (FIXTURES / "manifest.json").read_text(encoding="utf-8")
        self.assertEqual(text, corpus.dump_manifest(self.manifest))
        paths = [e["path"] for e in self.manifest["files"]]
        self.assertEqual(paths, sorted(paths))


# ---------------------------------------------------------------------------
# corpus.py against fakes
# ---------------------------------------------------------------------------

LIB_V1 = {"type": "excalidrawlib", "version": 1, "source": "x", "library": [[{"type": "rectangle"}]]}
LIB_V2 = {"type": "excalidrawlib", "version": 2, "source": "x", "libraryItems": [{"id": "a", "elements": []}]}


class SyncAndCheckTests(unittest.TestCase):
    UP_COMMIT = "1" * 40
    LIB_COMMIT = "2" * 40

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ex-corpus-test-"))
        # fake upstream checkout
        self.up = self.tmp / "upstream"
        fx = self.up / "packages" / "excalidraw" / "tests" / "fixtures"
        fx.mkdir(parents=True)
        (fx / "a.png").write_bytes(b"\x89PNG\r\n\x1a\n" + bytes(range(256)))
        (fx / "lib.excalidrawlib").write_text(json.dumps(LIB_V1))
        (fx / "sub").mkdir()
        (fx / "sub" / "nested.ts").write_text("export const n = 1;\n")
        data = self.up / "packages" / "excalidraw" / "tests" / "data"
        (data / "__snapshots__").mkdir(parents=True)
        (data / "restore.test.ts").write_text("// restore\n")
        (data / "reconcile.test.ts").write_text("// reconcile\n")
        (data / "__snapshots__" / "restore.test.ts.snap").write_text("// snap\n")
        tests = self.up / "packages" / "excalidraw" / "tests"
        (tests / "export.test.tsx").write_text("// export\n")
        (tests / "__snapshots__").mkdir()
        (tests / "__snapshots__" / "export.test.tsx.snap").write_text("// export snap\n")
        (tests / "scene" / "__snapshots__").mkdir(parents=True)
        (tests / "scene" / "__snapshots__" / "export.test.ts.snap").write_text("// scene snap\n")
        (self.up / "LICENSE").write_text("MIT upstream\n")
        # fake raw.githubusercontent tree: <base>/<commit>/<path>
        self.raw = self.tmp / "raw"
        lib_root = self.raw / self.LIB_COMMIT
        (lib_root / "libraries" / "alice").mkdir(parents=True)
        (lib_root / "libraries" / "bob").mkdir(parents=True)
        # a variation selector inside text: must survive byte-exact
        v2 = dict(LIB_V2, libraryItems=[{"id": "e", "name": "star " + chr(0x2B50) + chr(0xFE0F)}])
        (lib_root / "libraries" / "alice" / "one.excalidrawlib").write_text(json.dumps(LIB_V1))
        (lib_root / "libraries" / "bob" / "two.excalidrawlib").write_text(json.dumps(v2, ensure_ascii=False))
        catalogue = [
            {"name": "One", "source": "alice/one.excalidrawlib", "version": 1},
            {"name": "Two", "source": "bob/two.excalidrawlib", "version": 2},
        ]
        (lib_root / "libraries.json").write_text(json.dumps(catalogue, indent=2))
        (lib_root / "LICENSE").write_text("MIT libraries\n")
        self.pins = corpus.Pins(
            upstream_repo="https://github.com/excalidraw/excalidraw",
            upstream_commit=self.UP_COMMIT,
            libraries_repo="https://github.com/excalidraw/excalidraw-libraries",
            libraries_commit=self.LIB_COMMIT,
        )
        self.fixtures = self.tmp / "fixtures"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def sync(self, dest=None):
        corpus.sync(
            dest or self.fixtures, self.up, self.pins,
            libraries_raw_base=self.raw.as_uri(), retrieved="2026-09-28",
        )

    def manifest(self):
        return json.loads((self.fixtures / "manifest.json").read_text())

    def check(self):
        return corpus.check(self.fixtures, self.pins)

    # --- sync --------------------------------------------------------------

    def test_sync_writes_every_file_and_a_clean_manifest(self):
        self.sync()
        self.assertEqual(self.check(), [])
        paths = [e["path"] for e in self.manifest()["files"]]
        self.assertEqual(paths, sorted([
            "libraries/LICENSE",
            "libraries/alice/one.excalidrawlib.gz",
            "libraries/bob/two.excalidrawlib.gz",
            "libraries/libraries.json",
            "upstream/LICENSE",
            "upstream/packages/excalidraw/tests/data/__snapshots__/restore.test.ts.snap",
            "upstream/packages/excalidraw/tests/data/reconcile.test.ts",
            "upstream/packages/excalidraw/tests/data/restore.test.ts",
            "upstream/packages/excalidraw/tests/export.test.tsx",
            "upstream/packages/excalidraw/tests/fixtures/a.png",
            "upstream/packages/excalidraw/tests/fixtures/lib.excalidrawlib",
            "upstream/packages/excalidraw/tests/fixtures/sub/nested.ts",
            "upstream/packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap",
            "upstream/packages/excalidraw/tests/__snapshots__/export.test.tsx.snap",
        ]))

    def test_sync_records_sha256_and_origin(self):
        self.sync()
        m = self.manifest()
        self.assertEqual(m["sources"]["upstream"], {
            "repo": self.pins.upstream_repo, "commit": self.UP_COMMIT,
        })
        self.assertEqual(m["sources"]["libraries"]["commit"], self.LIB_COMMIT)
        self.assertEqual(m["sources"]["libraries"]["retrieved"], "2026-09-28")
        by = {e["path"]: e for e in m["files"]}
        png = by["upstream/packages/excalidraw/tests/fixtures/a.png"]
        raw_png = (self.up / "packages/excalidraw/tests/fixtures/a.png").read_bytes()
        self.assertEqual(png["sha256"], sha256(raw_png))
        self.assertEqual(png["bytes"], len(raw_png))
        self.assertEqual(png["origin_path"], "packages/excalidraw/tests/fixtures/a.png")
        self.assertEqual(
            png["origin"],
            f"https://raw.githubusercontent.com/excalidraw/excalidraw/{self.UP_COMMIT}"
            "/packages/excalidraw/tests/fixtures/a.png",
        )
        self.assertNotIn("encoding", png)
        two = by["libraries/bob/two.excalidrawlib.gz"]
        original = (self.raw / self.LIB_COMMIT / "libraries/bob/two.excalidrawlib").read_bytes()
        self.assertEqual(two["encoding"], "gzip")
        self.assertEqual(two["content_sha256"], sha256(original))
        self.assertEqual(two["content_bytes"], len(original))
        self.assertEqual(two["sha256"], sha256((self.fixtures / two["path"]).read_bytes()))
        self.assertEqual(two["origin_path"], "libraries/bob/two.excalidrawlib")
        self.assertEqual(
            two["origin"],
            f"https://raw.githubusercontent.com/excalidraw/excalidraw-libraries/{self.LIB_COMMIT}"
            "/libraries/bob/two.excalidrawlib",
        )
        self.assertEqual(corpus.read_fixture(self.fixtures, "libraries/bob/two.excalidrawlib"), original)

    def test_sync_is_deterministic(self):
        self.sync()
        first = {p.relative_to(self.fixtures): p.read_bytes() for p in self.fixtures.rglob("*") if p.is_file()}
        other = self.tmp / "again"
        self.sync(other)
        second = {p.relative_to(other): p.read_bytes() for p in other.rglob("*") if p.is_file()}
        self.assertEqual(first, second)

    def test_resync_removes_stale_files(self):
        self.sync()
        stale = self.fixtures / "libraries" / "gone" / "old.excalidrawlib.gz"
        stale.parent.mkdir(parents=True)
        stale.write_bytes(gzip.compress(b"{}"))
        self.sync()
        self.assertFalse(stale.exists())
        self.assertEqual(self.check(), [])

    def test_sync_fails_when_a_catalogue_library_is_missing(self):
        (self.raw / self.LIB_COMMIT / "libraries/bob/two.excalidrawlib").unlink()
        with self.assertRaises(corpus.CorpusError):
            self.sync()
        self.assertFalse((self.fixtures / "manifest.json").exists())

    def test_sync_rejects_catalogue_paths_that_escape(self):
        cat = self.raw / self.LIB_COMMIT / "libraries.json"
        cat.write_text(json.dumps([{"source": "../evil.excalidrawlib", "version": 1}]))
        with self.assertRaises(corpus.CorpusError):
            self.sync()

    # --- check -------------------------------------------------------------

    def test_check_reports_modified_file(self):
        self.sync()
        p = self.fixtures / "upstream/packages/excalidraw/tests/fixtures/a.png"
        p.write_bytes(p.read_bytes() + b"x")
        problems = self.check()
        self.assertTrue(any("a.png" in x and "sha256" in x for x in problems), problems)

    def test_check_reports_unlisted_file(self):
        self.sync()
        (self.fixtures / "upstream" / "extra.txt").write_text("stray\n")
        problems = self.check()
        self.assertTrue(any("upstream/extra.txt" in x and "not in manifest" in x for x in problems), problems)

    def test_check_reports_missing_file(self):
        self.sync()
        (self.fixtures / "libraries" / "LICENSE").unlink()
        problems = self.check()
        self.assertTrue(any("libraries/LICENSE" in x and "missing" in x for x in problems), problems)

    def test_check_reports_gzip_content_swap(self):
        # Replace a stored library with different content and forge the
        # stored-file hash: the content hash still catches it.
        self.sync()
        m = self.manifest()
        e = next(x for x in m["files"] if x["path"] == "libraries/alice/one.excalidrawlib.gz")
        forged = gzip.compress(b'{"type":"excalidrawlib","version":1,"library":[]}', mtime=0)
        (self.fixtures / e["path"]).write_bytes(forged)
        e["sha256"] = sha256(forged)
        e["bytes"] = len(forged)
        (self.fixtures / "manifest.json").write_text(corpus.dump_manifest(m))
        problems = self.check()
        self.assertTrue(any("content_sha256" in x for x in problems), problems)

    def test_check_reports_catalogue_library_without_file(self):
        self.sync()
        m = self.manifest()
        m["files"] = [x for x in m["files"] if x["path"] != "libraries/bob/two.excalidrawlib.gz"]
        (self.fixtures / "libraries/bob/two.excalidrawlib.gz").unlink()
        (self.fixtures / "manifest.json").write_text(corpus.dump_manifest(m))
        problems = self.check()
        self.assertTrue(any("bob/two.excalidrawlib" in x and "catalogue" in x for x in problems), problems)

    def test_check_reports_entry_without_origin(self):
        self.sync()
        m = self.manifest()
        m["files"][0]["origin"] = ""
        (self.fixtures / "manifest.json").write_text(corpus.dump_manifest(m))
        problems = self.check()
        self.assertTrue(any("origin" in x for x in problems), problems)

    def test_check_reports_pin_drift(self):
        self.sync()
        drifted = corpus.Pins(**{**self.pins._asdict(), "upstream_commit": "3" * 40})
        problems = corpus.check(self.fixtures, drifted)
        self.assertTrue(any("upstream" in x and "commit" in x for x in problems), problems)
        drifted = corpus.Pins(**{**self.pins._asdict(), "libraries_commit": "3" * 40})
        problems = corpus.check(self.fixtures, drifted)
        self.assertTrue(any("libraries" in x and "commit" in x for x in problems), problems)

    def test_check_reports_unsorted_or_duplicate_manifest(self):
        self.sync()
        m = self.manifest()
        m["files"].append(dict(m["files"][0]))
        (self.fixtures / "manifest.json").write_text(json.dumps(m))
        problems = self.check()
        self.assertTrue(any("duplicate" in x for x in problems), problems)
        self.assertTrue(any("canonical" in x for x in problems), problems)

    def test_check_without_manifest(self):
        self.fixtures.mkdir()
        problems = self.check()
        self.assertTrue(any("manifest.json" in x for x in problems), problems)

    # --- verify against origins ---------------------------------------------

    def test_verify_upstream_detects_divergence(self):
        self.sync()
        self.assertEqual(corpus.verify_upstream(self.fixtures, self.up), [])
        (self.up / "packages/excalidraw/tests/data/restore.test.ts").write_text("// changed\n")
        problems = corpus.verify_upstream(self.fixtures, self.up)
        self.assertTrue(any("restore.test.ts" in x for x in problems), problems)

    def test_verify_upstream_detects_new_upstream_fixture(self):
        self.sync()
        (self.up / "packages/excalidraw/tests/fixtures/new.svg").write_text("<svg/>\n")
        problems = corpus.verify_upstream(self.fixtures, self.up)
        self.assertTrue(any("new.svg" in x for x in problems), problems)

    def test_verify_remote_uses_origins(self):
        self.sync()
        # Serve the fake upstream the way raw.githubusercontent does.
        up_raw = self.tmp / "upraw"
        shutil.copytree(self.up, up_raw / self.UP_COMMIT)
        bases = {"upstream": up_raw.as_uri(), "libraries": self.raw.as_uri()}
        self.assertEqual(corpus.verify_remote(self.fixtures, raw_bases=bases), [])
        (self.raw / self.LIB_COMMIT / "libraries/alice/one.excalidrawlib").write_text("{}")
        (up_raw / self.UP_COMMIT / "LICENSE").write_text("changed\n")
        problems = corpus.verify_remote(self.fixtures, raw_bases=bases)
        self.assertTrue(any("alice/one.excalidrawlib" in x for x in problems), problems)
        self.assertTrue(any("upstream/LICENSE" in x for x in problems), problems)
        (up_raw / self.UP_COMMIT / "LICENSE").unlink()
        problems = corpus.verify_remote(self.fixtures, raw_bases=bases)
        self.assertTrue(any("upstream/LICENSE" in x and "fetch" in x for x in problems), problems)

    # --- CLI -----------------------------------------------------------------

    def test_cli_check_exit_status(self):
        self.sync()
        cli = [sys.executable, str(HERE / "corpus.py")]
        env_args = [
            "--fixtures", str(self.fixtures),
            "--upstream-commit", self.UP_COMMIT, "--libraries-commit", self.LIB_COMMIT,
        ]
        ok = subprocess.run([*cli, "check", *env_args], capture_output=True, text=True)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertIn("14 files", ok.stdout)
        (self.fixtures / "stray").write_text("x")
        bad = subprocess.run([*cli, "check", *env_args], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        self.assertIn("stray", bad.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
