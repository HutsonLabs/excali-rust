#!/usr/bin/env python3
"""Crate-graph gate: the Cargo workspace must match the architecture page.

The single source of truth is the crate table in
site/content/architecture/overview.md (and ADR-008). This script reads that
table and checks the real workspace against it:

  1. every crate in the table is a workspace member under crates/<name>, and
     no member is missing from the table;
  2. every workspace-internal dependency of a crate is reachable from the
     crates its "May depend on" cell lists (layers only point downwards);
  3. the crates named in the "dependency direction is enforced in CI"
     sentence do not pull the forbidden crates (web-sys, tokio) into their
     normal dependency graph for wasm32-unknown-unknown, and do not use
     std::fs in their sources.

Subcommands:
  check        run all checks (exit 0 clean, 1 violations, 2 usage error)
  wasm-crates  print the crates whose Targets cell includes wasm32, one per
               line, for the CI wasm32 build job
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OVERVIEW = ROOT / "site" / "content" / "architecture" / "overview.md"
WASM_TARGET = "wasm32-unknown-unknown"

TICKED = re.compile(r"`([^`]+)`")


@dataclass
class CrateSpec:
    name: str
    allowed: set[str]
    targets: set[str]


@dataclass
class Spec:
    crates: dict[str, CrateSpec] = field(default_factory=dict)
    wasm_pure: list[str] = field(default_factory=list)
    forbidden: list[str] = field(default_factory=list)

    def wasm_crates(self) -> list[str]:
        return [c.name for c in self.crates.values() if "wasm32" in c.targets]


def parse_overview(text: str) -> Spec:
    """Read the crate table and the CI-enforcement sentence."""
    spec = Spec()
    in_table = False
    header: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if not s.startswith("|"):
            if in_table:
                break
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        if not in_table:
            if cells and cells[0] == "Crate":
                header = cells
                in_table = True
            continue
        if set("".join(cells)) <= set("-: "):
            continue
        row = dict(zip(header, cells))
        names = TICKED.findall(row.get("Crate", ""))
        if not names:
            continue
        name = names[0]
        allowed = {d for d in TICKED.findall(row.get("May depend on", "")) if d.startswith("excali-")}
        targets = {t.strip() for t in row.get("Targets", "").split(",") if t.strip()}
        spec.crates[name] = CrateSpec(name, allowed, targets)
    if not spec.crates:
        raise ValueError("no crate table (a markdown table whose first column is 'Crate') found")

    m = re.search(r"enforced in CI:(.*?)must not pull(.*?)(?:\.\s|\.$)", text, re.S)
    if m:
        spec.wasm_pure = [n for n in TICKED.findall(m.group(1)) if n in spec.crates]
        spec.forbidden = sorted(
            n for n in TICKED.findall(m.group(2)) if n != "std::fs"
        )
    return spec


def _dep_is_normal(dep: dict) -> bool:
    return any(k.get("kind") in (None, "normal") for k in dep.get("dep_kinds", [{"kind": None}]))


def _pkg_name(pkg_id: str, by_id: dict[str, dict]) -> str:
    if pkg_id in by_id:
        return by_id[pkg_id]["name"]
    # Fallback for ids of packages not listed (e.g. registry crates with
    # --no-deps metadata): "registry+...#name@1.2.3" or "...#name@1.2.3".
    tail = pkg_id.rsplit("#", 1)[-1]
    return tail.split("@", 1)[0]


def check(spec: Spec, metadata: dict, wasm_metadata: dict) -> list[str]:
    """metadata: `cargo metadata` for the host; wasm_metadata: filtered to wasm32."""
    problems: list[str] = []
    by_id = {p["id"]: p for p in metadata["packages"]}
    members = {by_id[i]["name"]: by_id[i] for i in metadata["workspace_members"] if i in by_id}

    for name in spec.crates:
        if name not in members:
            problems.append(f"{name}: listed in the architecture table but missing from the workspace")
            continue
        manifest = Path(members[name]["manifest_path"])
        if manifest.parent.name != name or manifest.parent.parent.name != "crates":
            problems.append(f"{name}: expected at crates/{name}/Cargo.toml, found {manifest}")
    for name in members:
        if name not in spec.crates:
            problems.append(f"{name}: workspace member not listed in the architecture table")

    # Allowed closure: a crate may use anything reachable through the crates
    # its cell lists (excali-scene may use excali-math via excali-core).
    def closure(name: str, seen: set[str] | None = None) -> set[str]:
        seen = set() if seen is None else seen
        for d in spec.crates.get(name, CrateSpec(name, set(), set())).allowed:
            if d not in seen:
                seen.add(d)
                closure(d, seen)
        return seen

    for name, pkg in members.items():
        if name not in spec.crates:
            continue
        allowed = closure(name)
        for dep in pkg.get("dependencies", []):
            if "path" not in dep or dep.get("kind") == "dev":
                continue
            if dep["name"] not in allowed:
                problems.append(
                    f"{name}: depends on {dep['name']}, which its 'May depend on' cell does not reach"
                )

    # Forbidden crates in the wasm32 normal-dependency graph of pure crates.
    wby_id = {p["id"]: p for p in wasm_metadata["packages"]}
    nodes = {n["id"]: n for n in wasm_metadata.get("resolve", {}).get("nodes", [])}
    wmembers = {
        wby_id[i]["name"]: i for i in wasm_metadata["workspace_members"] if i in wby_id
    }
    for name in spec.wasm_pure:
        start = wmembers.get(name)
        if start is None:
            continue
        seen = {start}
        stack = [start]
        while stack:
            node = nodes.get(stack.pop())
            if not node:
                continue
            for dep in node.get("deps", []):
                if _dep_is_normal(dep) and dep["pkg"] not in seen:
                    seen.add(dep["pkg"])
                    stack.append(dep["pkg"])
        pulled = {_pkg_name(i, wby_id) for i in seen}
        for bad in spec.forbidden:
            if bad in pulled:
                problems.append(f"{name}: pulls {bad} into its {WASM_TARGET} build (ADR-008)")
    return problems


COMMENT = re.compile(r"//.*?$|/\*.*?\*/", re.S | re.M)


def scan_sources(sources: dict[str, dict[str, str]]) -> list[str]:
    """sources: crate -> {relative path -> text}. Flags std::fs outside comments."""
    problems = []
    for crate, files in sources.items():
        for rel, text in sorted(files.items()):
            code = COMMENT.sub("", text)
            if re.search(r"\bstd\s*::\s*fs\b", code):
                problems.append(f"{crate}: {rel} uses std::fs (ADR-008)")
    return problems


def _cargo_metadata(*extra: str) -> dict:
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", *extra],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as err:
        raise RuntimeError(f"cargo not found on PATH ({err})") from err
    if out.returncode != 0:
        raise RuntimeError(f"cargo metadata failed:\n{out.stderr}")
    return json.loads(out.stdout)


def _read_sources(crates: list[str]) -> dict[str, dict[str, str]]:
    result: dict[str, dict[str, str]] = {}
    for crate in crates:
        src = ROOT / "crates" / crate / "src"
        files = {}
        if src.is_dir():
            for path in src.rglob("*.rs"):
                files[str(path.relative_to(ROOT / "crates" / crate))] = path.read_text()
        result[crate] = files
    return result


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in ("check", "wasm-crates"):
        print(__doc__, file=sys.stderr)
        return 2
    spec = parse_overview(OVERVIEW.read_text())
    if argv[0] == "wasm-crates":
        print("\n".join(spec.wasm_crates()))
        return 0
    try:
        host = _cargo_metadata()
        wasm = _cargo_metadata("--filter-platform", WASM_TARGET)
    except RuntimeError as err:
        print(err, file=sys.stderr)
        return 1
    problems = check(spec, host, wasm) + scan_sources(_read_sources(spec.wasm_pure))
    if problems:
        print("workspace gate FAILED:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"workspace gate OK ({len(spec.crates)} crates, {len(spec.wasm_pure)} wasm-pure)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
