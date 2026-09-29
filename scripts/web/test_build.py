#!/usr/bin/env python3
"""Tests for scripts/web/build.sh (the web build pipeline, ex-501).

build.sh runs from a copy of the repository's scripts in a temporary root,
with stub cargo, wasm-bindgen and wasm-opt on PATH that record their
arguments and write stand-in outputs, so the pipeline's order, flags and
outputs are checked in seconds. The real build (and the size budget on its
output) runs in CI: rust.yml, job web-runtime.

Run: python3 scripts/web/test_build.py -v
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Each stub appends {"tool", "args"} to $STUB_LOG.
STUB_CARGO = r"""#!/usr/bin/env python3
import json, os, sys
with open(os.environ["STUB_LOG"], "a") as f:
    f.write(json.dumps({"tool": "cargo", "args": sys.argv[1:]}) + "\n")
args = sys.argv[1:]
profile = args[args.index("--profile") + 1]
target = os.environ["CARGO_TARGET_DIR"]
out = os.path.join(target, "wasm32-unknown-unknown", profile)
os.makedirs(out, exist_ok=True)
open(os.path.join(out, "excali_wasm.wasm"), "wb").write(b"\0asm-from-cargo")
"""

STUB_WASM_BINDGEN = r"""#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
if args == ["--version"]:
    print("wasm-bindgen " + os.environ.get("STUB_BINDGEN_VERSION", "0.2.129"))
    sys.exit(0)
with open(os.environ["STUB_LOG"], "a") as f:
    f.write(json.dumps({"tool": "wasm-bindgen", "args": args}) + "\n")
out = args[args.index("--out-dir") + 1]
name = args[args.index("--out-name") + 1]
open(os.path.join(out, name + ".js"), "w").write("export default function init() {}\n")
open(os.path.join(out, name + "_bg.wasm"), "wb").write(b"\0asm-from-bindgen")
"""

STUB_WASM_OPT = r"""#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
if args == ["--version"]:
    print("wasm-opt version 133 (version_133)")
    sys.exit(0)
with open(os.environ["STUB_LOG"], "a") as f:
    f.write(json.dumps({"tool": "wasm-opt", "args": args}) + "\n")
src = args[-1]
out = args[args.index("-o") + 1]
assert open(src, "rb").read() == b"\0asm-from-bindgen", "wasm-opt must read wasm-bindgen's output"
open(out, "wb").write(b"\0asm-optimised")
"""

LOCK = """version = 4

[[package]]
name = "wasm-bindgen"
version = "0.2.129"
source = "registry+https://github.com/rust-lang/crates.io-index"
"""


class BuildTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="web-build-test."))
        self.root = self.tmp / "repo"
        for rel in (
            "scripts/web/build.sh",
            "scripts/web/binaryen.py",
            "scripts/gates/wasm_size.py",
            "site/content/plan/phases.md",
            "crates/excali-wasm/js/excali-editor.js",
            "crates/excali-wasm/excali.css",
        ):
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / rel, self.root / rel)
        (self.root / "Cargo.lock").write_text(LOCK)
        fonts = self.root / "crates/excali-text/assets/fonts"
        (fonts / "Excalifont").mkdir(parents=True)
        (fonts / "manifest.json").write_text("{}")
        (fonts / "Excalifont" / "a.woff2").write_bytes(b"wOF2")
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        for name, body in (
            ("cargo", STUB_CARGO),
            ("wasm-bindgen", STUB_WASM_BINDGEN),
            ("wasm-opt", STUB_WASM_OPT),
        ):
            (self.bin / name).write_text(body)
            (self.bin / name).chmod(0o755)
        self.log = self.tmp / "calls.jsonl"
        self.target = self.tmp / "target"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def build(self, *args: str, **env: str) -> subprocess.CompletedProcess[str]:
        python_dir = str(Path(sys.executable).parent)
        full_env = {
            "PATH": f"{self.bin}:{python_dir}:/usr/bin:/bin",
            "HOME": os.environ.get("HOME", str(self.tmp)),
            "STUB_LOG": str(self.log),
            "CARGO_TARGET_DIR": str(self.target),
            "BINARYEN_TOOLS_DIR": str(self.tmp / "tools"),
        }
        full_env.update(env)
        return subprocess.run(
            ["sh", str(self.root / "scripts/web/build.sh"), *args],
            env=full_env,
            capture_output=True,
            text=True,
        )

    def calls(self) -> list[dict]:
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_emits_dist_excali_editor_js_and_wasm_by_default(self):
        r = self.build()
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        dist = self.root / "dist"
        # wasm-bindgen's glue, then the custom element shim (ex-530)
        shim = (ROOT / "crates/excali-wasm/js/excali-editor.js").read_text()
        self.assertEqual(
            (dist / "excali_editor.js").read_text(),
            "export default function init() {}\n" + shim,
        )
        self.assertEqual(
            (dist / "excali.css").read_text(),
            (ROOT / "crates/excali-wasm/excali.css").read_text(),
        )
        # The shipped module is wasm-opt's output, not wasm-bindgen's.
        self.assertEqual((dist / "excali_editor_bg.wasm").read_bytes(), b"\0asm-optimised")
        self.assertEqual((dist / "fonts" / "Excalifont" / "a.woff2").read_bytes(), b"wOF2")
        self.assertTrue((dist / "fonts" / "manifest.json").is_file())
        # Nothing else: no TypeScript, no intermediate files.
        self.assertEqual(
            sorted(p.name for p in dist.iterdir()),
            ["excali.css", "excali_editor.js", "excali_editor_bg.wasm", "fonts"],
        )

    def test_pipeline_order_and_flags(self):
        r = self.build()
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        calls = self.calls()
        self.assertEqual([c["tool"] for c in calls], ["cargo", "wasm-bindgen", "wasm-opt"])
        cargo, bindgen, opt = (c["args"] for c in calls)
        self.assertEqual(
            cargo,
            [
                "build",
                "--locked",
                "--profile",
                "web-release",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "excali-wasm",
            ],
        )
        self.assertEqual(bindgen[bindgen.index("--target") + 1], "web")
        self.assertIn("--no-typescript", bindgen)
        self.assertEqual(bindgen[bindgen.index("--out-name") + 1], "excali_editor")
        self.assertEqual(
            bindgen[-1],
            str(self.target / "wasm32-unknown-unknown" / "web-release" / "excali_wasm.wasm"),
        )
        self.assertIn("-Oz", opt)
        self.assertIn("--strip-debug", opt)
        self.assertIn("--strip-producers", opt)
        self.assertEqual(opt[opt.index("-o") + 1], str(self.root / "dist" / "excali_editor_bg.wasm"))

    def test_reports_gzip_sizes_against_the_budget(self):
        r = self.build()
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        self.assertIn("excali_editor_bg.wasm:", r.stdout)
        self.assertIn("budget 1,500,000", r.stdout)
        self.assertIn("excali_editor.js:", r.stdout)
        self.assertIn("budget 20,000", r.stdout)

    def test_over_budget_fails_the_build(self):
        phases = self.root / "site/content/plan/phases.md"
        text = phases.read_text(encoding="utf-8")
        phases.write_text(text.replace("≤ 1.5 MB", "≤ 10 B"), encoding="utf-8")
        r = self.build()
        self.assertEqual(r.returncode, 1, r.stderr + r.stdout)
        self.assertIn("over budget", r.stderr)

    def test_explicit_out_dir_replaces_its_contents(self):
        out = self.tmp / "elsewhere"
        out.mkdir()
        (out / "stale.txt").write_text("old")
        r = self.build(str(out))
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        self.assertEqual(
            sorted(p.name for p in out.iterdir()),
            ["excali.css", "excali_editor.js", "excali_editor_bg.wasm", "fonts"],
        )
        self.assertFalse((self.root / "dist").exists())

    def test_wasm_bindgen_version_must_match_cargo_lock(self):
        r = self.build(STUB_BINDGEN_VERSION="0.2.100")
        self.assertEqual(r.returncode, 1)
        self.assertIn("wasm-bindgen 0.2.100, Cargo.lock pins 0.2.129", r.stderr)
        self.assertEqual(self.calls(), [])



class ProfileTest(unittest.TestCase):
    """The workspace's web-release profile: the settings the size budget
    was measured with (risks page: wasm-opt, panic = "abort")."""

    def section(self, name: str) -> dict[str, str]:
        lines = (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines()
        start = lines.index(f"[{name}]")
        values: dict[str, str] = {}
        for line in lines[start + 1 :]:
            line = line.split("#", 1)[0].strip()
            if line.startswith("["):
                break
            if "=" in line:
                key, value = (x.strip() for x in line.split("=", 1))
                values[key] = value
        return values

    def test_web_release_profile(self):
        self.assertEqual(
            self.section("profile.web-release"),
            {
                "inherits": '"release"',
                "opt-level": '"z"',
                "lto": "true",
                "codegen-units": "1",
                "panic": '"abort"',
            },
        )


if __name__ == "__main__":
    unittest.main()
