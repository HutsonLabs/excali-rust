#!/usr/bin/env python3
"""Tests for scripts/gates/workspace.py (the crate-graph gate).

Run: python3 scripts/gates/test_workspace.py
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import workspace  # noqa: E402

TABLE = """
## Crates

| Crate | Upstream counterpart | May depend on | Targets |
|---|---|---|---|
| `excali-math` | `packages/math` | `std` only | native, wasm32 |
| `excali-core` | `types.ts` | `excali-math`, `serde`, `serde_json` | native, wasm32 |
| `excali-scene` | `shape.ts` | `excali-core` | native, wasm32 |
| `excali-raster` | `export.ts` | `excali-scene`, `tiny-skia` | native |
| `excali-cli` | none (new) | `excali-raster`, `clap` | native |

The dependency direction is enforced in CI: `excali-core`, `excali-math` and `excali-scene` are built with `--target wasm32-unknown-unknown` and must not pull `std::fs`, `tokio` or `web-sys`.

## Data flow
"""


def node_id(name, path=True):
    return f"path+file:///w/crates/{name}#0.0.0" if path else f"registry+x#{name}@1.0.0"


def pkg(name, deps=(), path_deps=()):
    return {
        "name": name,
        "id": node_id(name),
        "manifest_path": f"/w/crates/{name}/Cargo.toml",
        "dependencies": [{"name": d, "path": f"/w/crates/{d}", "kind": None} for d in path_deps]
        + [{"name": d, "kind": None} for d in deps],
    }


def meta(packages, extra_nodes=()):
    nodes = []
    for p in packages:
        dep_ids = [
            {"pkg": node_id(d["name"], "path" in d), "dep_kinds": [{"kind": None, "target": None}]}
            for d in p["dependencies"]
        ]
        nodes.append({"id": p["id"], "deps": dep_ids})
    nodes.extend(extra_nodes)
    return {
        "packages": packages,
        "workspace_members": [p["id"] for p in packages],
        "resolve": {"nodes": nodes},
    }


GOOD = [
    pkg("excali-math"),
    pkg("excali-core", deps=["serde_json"], path_deps=["excali-math"]),
    pkg("excali-scene", path_deps=["excali-core", "excali-math"]),
    pkg("excali-raster", path_deps=["excali-scene"]),
    pkg("excali-cli", path_deps=["excali-raster", "excali-core"]),
]


class ParseTable(unittest.TestCase):
    def test_reads_every_row(self):
        spec = workspace.parse_overview(TABLE)
        self.assertEqual(
            list(spec.crates),
            ["excali-math", "excali-core", "excali-scene", "excali-raster", "excali-cli"],
        )

    def test_allowed_internal_deps_are_the_excali_names_only(self):
        spec = workspace.parse_overview(TABLE)
        self.assertEqual(spec.crates["excali-core"].allowed, {"excali-math"})
        self.assertEqual(spec.crates["excali-math"].allowed, set())
        self.assertEqual(spec.crates["excali-cli"].allowed, {"excali-raster"})

    def test_targets(self):
        spec = workspace.parse_overview(TABLE)
        self.assertEqual(spec.crates["excali-core"].targets, {"native", "wasm32"})
        self.assertEqual(spec.crates["excali-raster"].targets, {"native"})
        self.assertEqual(spec.wasm_crates(), ["excali-math", "excali-core", "excali-scene"])

    def test_wasm_pure_list_comes_from_the_enforcement_sentence(self):
        spec = workspace.parse_overview(TABLE)
        self.assertEqual(spec.wasm_pure, ["excali-core", "excali-math", "excali-scene"])
        self.assertEqual(spec.forbidden, ["tokio", "web-sys"])

    def test_missing_table_is_an_error(self):
        with self.assertRaises(ValueError):
            workspace.parse_overview("## Crates\n\nnothing here\n")


class Check(unittest.TestCase):
    def setUp(self):
        self.spec = workspace.parse_overview(TABLE)

    def test_clean_workspace_passes(self):
        self.assertEqual(workspace.check(self.spec, meta(GOOD), meta(GOOD)), [])

    def test_transitive_dependency_is_allowed(self):
        # excali-cli -> excali-core is reachable through excali-raster -> excali-scene.
        problems = workspace.check(self.spec, meta(GOOD), meta(GOOD))
        self.assertFalse(any("excali-cli" in p for p in problems))

    def test_missing_crate_is_reported(self):
        m = meta([p for p in GOOD if p["name"] != "excali-scene"])
        problems = workspace.check(self.spec, m, m)
        self.assertTrue(any("excali-scene" in p and "missing" in p for p in problems), problems)

    def test_unlisted_member_is_reported(self):
        m = meta(GOOD + [pkg("excali-stray")])
        problems = workspace.check(self.spec, m, m)
        self.assertTrue(any("excali-stray" in p for p in problems), problems)

    def test_upward_dependency_is_reported(self):
        bad = [pkg("excali-math", path_deps=["excali-scene"])] + GOOD[1:]
        m = meta(bad)
        problems = workspace.check(self.spec, m, m)
        self.assertTrue(
            any("excali-math" in p and "excali-scene" in p for p in problems), problems
        )

    def test_forbidden_crate_in_wasm_graph_is_reported(self):
        core = pkg("excali-core", deps=["web-sys"], path_deps=["excali-math"])
        pkgs = [GOOD[0], core] + GOOD[2:]
        m = meta(pkgs, extra_nodes=[{"id": node_id("web-sys", False), "deps": []}])
        problems = workspace.check(self.spec, m, m)
        self.assertTrue(any("excali-core" in p and "web-sys" in p for p in problems), problems)

    def test_forbidden_crate_reached_transitively_is_reported(self):
        # excali-scene is clean itself but reaches web-sys through excali-core.
        core = pkg("excali-core", deps=["web-sys"], path_deps=["excali-math"])
        pkgs = [GOOD[0], core] + GOOD[2:]
        m = meta(pkgs, extra_nodes=[{"id": node_id("web-sys", False), "deps": []}])
        problems = workspace.check(self.spec, m, m)
        self.assertTrue(any("excali-scene" in p and "web-sys" in p for p in problems), problems)

    def test_dev_dependencies_do_not_count_toward_the_wasm_graph(self):
        m = meta(GOOD, extra_nodes=[{"id": node_id("tokio", False), "deps": []}])
        for node in m["resolve"]["nodes"]:
            if node["id"] == node_id("excali-math"):
                node["deps"].append(
                    {"pkg": node_id("tokio", False), "dep_kinds": [{"kind": "dev", "target": None}]}
                )
        self.assertEqual(workspace.check(self.spec, m, m), [])

    def test_std_fs_in_pure_crate_source_is_reported(self):
        problems = workspace.scan_sources({"excali-core": {"src/lib.rs": "use std::fs;\n"}})
        self.assertTrue(any("excali-core" in p and "std::fs" in p for p in problems), problems)

    def test_std_fs_in_a_comment_is_ignored(self):
        self.assertEqual(
            workspace.scan_sources({"excali-core": {"src/lib.rs": "// never use std::fs here\n"}}),
            [],
        )


class Repository(unittest.TestCase):
    """The real repository satisfies its own architecture page."""

    def test_repository_passes_the_gate(self):
        self.assertEqual(workspace.main(["check"]), 0)


if __name__ == "__main__":
    unittest.main()
