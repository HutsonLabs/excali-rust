#!/usr/bin/env python3
"""ex-206 counterfactual: roughr 0.14.0 with rough.js's random generator.

Copies the roughr 0.14.0 sources Cargo resolved for tools/roughr-eval into
tools/roughr-eval/target/fork/roughr, applies park-miller.patch (rough.js
4.6.4's Park-Miller Random in place of rand's StdRng, nothing else), builds a
copy of the evaluation against it with [patch.crates-io], and writes or
checks tools/roughr-eval/report-fork.json. That report is what "fork roughr
and fix the generator" would reach, and it is how report.json attributes a
mismatch to the generator: a case the fork matches differs only by `rng`.

    python3 tools/roughr-eval/fork.py --write   # regenerate report-fork.json
    python3 tools/roughr-eval/fork.py --check   # exit 1 if it is stale

Everything it builds stays under tools/roughr-eval/target/ (git-ignored); the
committed Cargo.lock is not touched.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools" / "roughr-eval"
SCRATCH = TOOL / "target" / "fork"
PATCH = TOOL / "park-miller.patch"
ROUGHR = ("roughr", "0.14.0")


def roughr_source() -> Path:
    """The directory of the roughr package tools/roughr-eval resolves."""
    out = subprocess.run(
        [
            "cargo", "metadata", "--manifest-path", str(TOOL / "Cargo.toml"),
            "--format-version", "1", "--locked",
        ],
        check=True, capture_output=True, text=True,
    ).stdout
    for pkg in json.loads(out)["packages"]:
        if (pkg["name"], pkg["version"]) == ROUGHR and pkg["source"].startswith("registry+"):
            return Path(pkg["manifest_path"]).parent
    raise SystemExit(f"{ROUGHR[0]} {ROUGHR[1]} from crates.io is not in the resolved graph")


def prepare() -> Path:
    """Patched roughr and a copy of the evaluation that uses it."""
    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    roughr = SCRATCH / "roughr"
    shutil.copytree(roughr_source(), roughr, ignore=shutil.ignore_patterns("target"))
    subprocess.run(
        ["patch", "-p1", "--batch", "--forward", "-d", str(roughr), "-i", str(PATCH)],
        check=True,
    )
    evaluation = SCRATCH / "eval"
    evaluation.mkdir()
    shutil.copytree(TOOL / "src", evaluation / "src")
    shutil.copy(TOOL / "Cargo.lock", evaluation / "Cargo.lock")
    manifest = (TOOL / "Cargo.toml").read_text(encoding="utf-8")
    manifest += '\n[patch.crates-io]\nroughr = { path = "../roughr" }\n'
    (evaluation / "Cargo.toml").write_text(manifest, encoding="utf-8")
    return evaluation


def main(argv: list[str]) -> int:
    if argv not in (["--write"], ["--check"]):
        print("usage: fork.py --write | --check", file=sys.stderr)
        return 2
    evaluation = prepare()
    return subprocess.run(
        [
            "cargo", "run", "--quiet", "--release",
            "--manifest-path", str(evaluation / "Cargo.toml"),
            "--", argv[0], "--root", str(ROOT),
        ],
    ).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
