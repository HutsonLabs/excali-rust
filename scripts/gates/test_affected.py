#!/usr/bin/env python3
"""Tests for scripts/gates/affected.py (pull-request CI on the affected crates).

The unit cases use a synthetic workspace; the command-line cases build a
scratch git repository in a temporary directory with a metadata file in the
shape `cargo metadata --no-deps --format-version 1` prints, so no cargo is
needed.

Run: python3 scripts/gates/test_affected.py -v
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import affected  # noqa: E402

SCRIPT = HERE / "affected.py"

# math <- core <- scene <- raster <- cli ; core <- editor (dev-dependency on scene)
GRAPH = {
    "excali-math": [],
    "excali-core": ["excali-math"],
    "excali-scene": ["excali-core"],
    "excali-raster": ["excali-scene"],
    "excali-cli": ["excali-raster"],
    "excali-editor": ["excali-core", "excali-scene"],
}


def metadata(root: str, graph=GRAPH, extra=()) -> dict:
    pkgs = []
    for name, deps in graph.items():
        pkgs.append(
            {
                "name": name,
                "id": f"path+file://{root}/crates/{name}#26.9.1",
                "manifest_path": f"{root}/crates/{name}/Cargo.toml",
                "dependencies": [
                    {"name": d, "kind": "dev" if (name, d) == ("excali-editor", "excali-scene") else None,
                     "path": f"{root}/crates/{d}"}
                    for d in deps
                ]
                + [{"name": "serde_json", "kind": None}],
            }
        )
    pkgs.extend(extra)
    return {
        "packages": pkgs,
        "workspace_members": [p["id"] for p in pkgs if not p["id"].startswith("registry")],
        "workspace_root": root,
    }


CRATES = affected.crates_from_metadata(metadata("/w"))
NO_SOURCES = {c.name: ("", "") for c in CRATES}


def run(changed, sources=None):
    return affected.cargo_args(affected.affected(changed, CRATES, sources or NO_SOURCES))


class Metadata(unittest.TestCase):
    def test_workspace_path_dependencies_only(self):
        by = {c.name: c for c in CRATES}
        self.assertEqual(by["excali-core"].dir, "crates/excali-core")
        self.assertEqual(by["excali-core"].deps, {"excali-math"})
        self.assertEqual(by["excali-math"].deps, set())  # serde_json is not a path dependency

    def test_non_members_are_ignored(self):
        reg = {"name": "serde", "id": "registry+x#serde@1.0.0", "manifest_path": "/reg/serde/Cargo.toml", "dependencies": []}
        crates = affected.crates_from_metadata(metadata("/w", extra=[reg]))
        self.assertNotIn("serde", {c.name for c in crates})

    def test_nested_crate_directory_wins(self):
        outer = affected.Crate("outer", "tools/outer", set())
        inner = affected.Crate("inner", "tools/outer/inner", set())
        self.assertEqual(affected.owner("tools/outer/inner/src/lib.rs", [outer, inner]).name, "inner")
        self.assertEqual(affected.owner("tools/outer/src/lib.rs", [outer, inner]).name, "outer")
        self.assertIsNone(affected.owner("tools/outerx/src/lib.rs", [outer, inner]))


class Global(unittest.TestCase):
    def test_global_files_run_the_workspace(self):
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "clippy.toml",
            ".github/workflows/rust.yml",
            ".cargo/config.toml",
            "goldens/math.json",
            "goldens/sub/dir/x.json",
            "fixtures/upstream/a.excalidraw",
            "tests/fixtures/x.json",
            "scripts/gates/affected.py",
        ]:
            self.assertEqual(run(["crates/excali-math/src/lib.rs", path]), "--workspace", path)

    def test_crate_manifest_is_not_global(self):
        self.assertEqual(run(["crates/excali-cli/Cargo.toml"]), "-p excali-cli")

    def test_unattributed_file_runs_the_workspace(self):
        for path in ["scripts/web/build.sh", "tools/goldens/generate.mjs", "examples/tauri-app/src/main.rs", "tests/web/x.spec.ts"]:
            self.assertEqual(run([path]), "--workspace", path)


class Docs(unittest.TestCase):
    def test_docs_and_site_only_print_nothing(self):
        self.assertEqual(
            run(["site/content/plan/phases.md", "README.md", "AGENTS.md", "plan/tasks.json", ".beads/issues.jsonl", "tests/site/smoke.spec.ts", "LICENSE"]),
            "",
        )
        self.assertEqual(run([]), "")

    def test_a_site_file_a_crate_reads_selects_that_crate(self):
        sources = dict(NO_SOURCES)
        sources["excali-cli"] = ("", 'concat!(env!("CARGO_MANIFEST_DIR"), "/../../site/content/architecture/cli.md")')
        sources["excali-raster"] = ("", 'repo().join("site/config.toml")')
        self.assertEqual(run(["site/content/architecture/cli.md"], sources), "-p excali-cli")
        self.assertEqual(run(["site/config.toml"], sources), "-p excali-raster")
        self.assertEqual(run(["site/content/plan/phases.md"], sources), "")

    def test_a_directory_literal_covers_its_files(self):
        sources = dict(NO_SOURCES)
        sources["excali-math"] = ("", '.join("../../site/static/schema")')
        self.assertEqual(run(["site/static/schema/scene.json"], sources), "-p excali-math")
        # A literal must end at the name: "site/static/schemas" is another path.
        sources["excali-math"] = ("", '.join("../../site/static/schemas")')
        self.assertEqual(run(["site/static/schema/scene.json"], sources), "")


class Crates(unittest.TestCase):
    def test_library_change_adds_reverse_dependents_transitively(self):
        self.assertEqual(
            run(["crates/excali-core/src/lib.rs"]),
            "-p excali-cli -p excali-core -p excali-editor -p excali-raster -p excali-scene",
        )
        self.assertEqual(run(["crates/excali-raster/src/lib.rs"]), "-p excali-cli -p excali-raster")
        self.assertEqual(run(["crates/excali-cli/src/main.rs"]), "-p excali-cli")

    def test_dev_dependents_are_included(self):
        self.assertIn("-p excali-editor", run(["crates/excali-scene/src/lib.rs"]))

    def test_build_script_and_assets_are_library(self):
        self.assertEqual(run(["crates/excali-raster/build.rs"]), "-p excali-cli -p excali-raster")
        self.assertEqual(run(["crates/excali-raster/assets/font.woff2"]), "-p excali-cli -p excali-raster")

    def test_test_files_do_not_reach_dependents(self):
        self.assertEqual(run(["crates/excali-core/tests/fixtures/library.json"]), "-p excali-core")
        self.assertEqual(run(["crates/excali-core/tests/schema.rs", "crates/excali-core/benches/b.rs"]), "-p excali-core")

    def test_crate_reading_another_crates_fixture(self):
        sources = dict(NO_SOURCES)
        sources["excali-editor"] = ("", 'include_str!("../../excali-core/tests/fixtures/library.json")')
        self.assertEqual(run(["crates/excali-core/tests/fixtures/library.json"], sources), "-p excali-core -p excali-editor")
        sources["excali-editor"] = ("", 'repo().join("crates/excali-core/tests/fixtures/library.json")')
        self.assertEqual(run(["crates/excali-core/tests/fixtures/library.json"], sources), "-p excali-core -p excali-editor")

    def test_library_reader_brings_its_dependents(self):
        sources = dict(NO_SOURCES)
        sources["excali-raster"] = ('Path::new(env!("CARGO_MANIFEST_DIR")).join("../excali-core/tests/fixtures")', "")
        self.assertEqual(
            run(["crates/excali-core/tests/fixtures/library.json"], sources),
            "-p excali-cli -p excali-core -p excali-raster",
        )

    def test_needles(self):
        c = affected.Crate("excali-core", "crates/excali-core", set())
        self.assertEqual(
            affected.needles("crates/excali-core/tests/fixtures/a.json", c),
            [
                "crates/excali-core/tests/fixtures/a.json",
                "crates/excali-core/tests/fixtures",
                "crates/excali-core/tests",
                "crates/excali-core",
                "excali-core/tests/fixtures/a.json",
                "excali-core/tests/fixtures",
                "excali-core/tests",
            ],
        )


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-c", "user.name=T", "-c", "user.email=t@example.invalid", *args],
        cwd=cwd, capture_output=True, text=True, check=True,
    ).stdout


class CommandLine(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name).resolve()
        r = self.root
        git(r, "init", "-q", "-b", "main")
        for name in GRAPH:
            (r / "crates" / name / "src").mkdir(parents=True)
            (r / "crates" / name / "src" / "lib.rs").write_text("// lib\n")
        (r / "site").mkdir()
        (r / "site" / "page.md").write_text("page\n")
        git(r, "add", "-A")
        git(r, "commit", "-q", "-m", "base")
        self.meta = r / "meta.json"
        self.meta.write_text(json.dumps(metadata(str(r))))
        git(r, "switch", "-q", "-c", "topic")

    def tearDown(self):
        self.tmp.cleanup()

    def cli(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args], cwd=self.root, capture_output=True, text=True
        )

    def commit(self, path: str, text: str = "x\n"):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
        git(self.root, "add", path)
        git(self.root, "commit", "-q", "-m", path)

    def test_crate_change(self):
        self.commit("crates/excali-raster/src/lib.rs")
        r = self.cli("main", "--metadata", str(self.meta))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "-p excali-cli -p excali-raster\n")

    def test_docs_only_prints_nothing(self):
        self.commit("site/page.md", "new\n")
        r = self.cli("main", "--metadata", str(self.meta))
        self.assertEqual((r.returncode, r.stdout), (0, ""), r.stderr)

    def test_global_change(self):
        self.commit("Cargo.lock")
        r = self.cli("main", "--metadata", str(self.meta))
        self.assertEqual((r.returncode, r.stdout), (0, "--workspace\n"), r.stderr)

    def test_diff_is_from_the_merge_base(self):
        # A commit that reached the base after the branch forked is not the
        # branch's change.
        self.commit("crates/excali-cli/src/lib.rs")
        git(self.root, "switch", "-q", "main")
        self.commit("Cargo.lock")
        git(self.root, "switch", "-q", "topic")
        r = self.cli("main", "--metadata", str(self.meta))
        self.assertEqual((r.returncode, r.stdout), (0, "-p excali-cli\n"), r.stderr)

    def test_usage_errors(self):
        self.assertEqual(self.cli().returncode, 2)
        self.assertEqual(self.cli("main", "--metadata").returncode, 2)
        self.assertEqual(self.cli("no-such-ref", "--metadata", str(self.meta)).returncode, 2)


if __name__ == "__main__":
    unittest.main()
