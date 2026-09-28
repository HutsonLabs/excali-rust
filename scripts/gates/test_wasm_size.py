#!/usr/bin/env python3
"""Tests for scripts/gates/wasm_size.py (the web build's size budget, ex-501).

Run: python3 scripts/gates/test_wasm_size.py -v
"""
from __future__ import annotations

import gzip
import io
import os
import random
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import wasm_size  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]

TABLE = """
## Budgets

| Budget | Value | Basis |
|---|---|---|
| WASM module, no fonts, gzip | ≤ {wasm} | term.hut ships a 5.4 MB dmg. |
| ES-module JS (wasm-bindgen glue and shim), gzip | ≤ {js} | It only mounts the element. |
| First paint after module load | ≤ 300 ms | Upstream shows its canvas immediately. |
"""


def page(tmp: Path, wasm: str = "1.5 MB", js: str = "20 KB") -> Path:
    p = tmp / "phases.md"
    p.write_text(TABLE.format(wasm=wasm, js=js), encoding="utf-8")
    return p


def incompressible(n: int) -> bytes:
    return random.Random(n).randbytes(n) if hasattr(random.Random, "randbytes") else bytes(
        random.Random(n).getrandbits(8) for _ in range(n)
    )


class BudgetsTest(unittest.TestCase):
    def test_the_phases_page_budgets(self):
        # site/content/plan/phases.md#budgets is the single source.
        budgets = wasm_size.budgets(ROOT / "site/content/plan/phases.md")
        self.assertEqual(
            budgets,
            {"excali_editor_bg.wasm": 1_500_000, "excali_editor.js": 20_000},
        )

    def test_units_are_decimal(self):
        self.assertEqual(wasm_size.parse_size("1.5 MB"), 1_500_000)
        self.assertEqual(wasm_size.parse_size("20 KB"), 20_000)
        self.assertEqual(wasm_size.parse_size("≤ 750 KB"), 750_000)
        self.assertEqual(wasm_size.parse_size("123 B"), 123)
        for bad in ("fast", "1.5 MiB", "300 ms", ""):
            with self.assertRaises(ValueError, msg=bad):
                wasm_size.parse_size(bad)

    def test_a_missing_row_is_an_error(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "phases.md"
            p.write_text(
                TABLE.format(wasm="1.5 MB", js="20 KB").replace("| ES-module JS", "| Something"),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "ES-module JS"):
                wasm_size.budgets(p)

    def test_a_budget_not_measured_gzipped_is_an_error(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "phases.md"
            p.write_text(
                TABLE.format(wasm="1.5 MB", js="20 KB").replace("no fonts, gzip", "no fonts"),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "WASM module"):
                wasm_size.budgets(p)


class GzipSizeTest(unittest.TestCase):
    def test_level_9_without_a_timestamp(self):
        data = b"excalidraw " * 1000
        want = len(gzip.compress(data, compresslevel=9, mtime=0))
        self.assertEqual(wasm_size.gzip_size(data), want)
        # Deterministic: no mtime or file name in the header.
        self.assertEqual(wasm_size.gzip_size(data), wasm_size.gzip_size(bytes(data)))


class CheckTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.dist = self.tmp / "dist"
        self.dist.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, wasm: bytes, js: bytes) -> None:
        (self.dist / "excali_editor_bg.wasm").write_bytes(wasm)
        (self.dist / "excali_editor.js").write_bytes(js)

    def run_check(self, phases: Path) -> tuple[int, str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            code = wasm_size.main(["check", str(self.dist), "--phases", str(phases)])
        return code, out.getvalue()

    def test_within_budget_passes_and_reports_sizes(self):
        wasm, js = incompressible(3000), b"export default 1;\n" * 10
        self.write(wasm, js)
        code, out = self.run_check(page(self.tmp, wasm="4 KB", js="1 KB"))
        self.assertEqual(code, 0, out)
        self.assertIn(f"excali_editor_bg.wasm: {wasm_size.gzip_size(wasm):,} bytes gzip", out)
        self.assertIn("budget 4,000", out)

    def test_exactly_at_the_budget_passes(self):
        wasm = incompressible(2000)
        self.write(wasm, b"x")
        n = wasm_size.gzip_size(wasm)
        code, out = self.run_check(page(self.tmp, wasm=f"{n} B", js="1 KB"))
        self.assertEqual(code, 0, out)

    def test_one_byte_over_fails(self):
        wasm = incompressible(2000)
        self.write(wasm, b"x")
        n = wasm_size.gzip_size(wasm)
        code, out = self.run_check(page(self.tmp, wasm=f"{n - 1} B", js="1 KB"))
        self.assertEqual(code, 1, out)
        self.assertIn("excali_editor_bg.wasm", out)
        self.assertIn("over budget", out)

    def test_js_over_budget_fails(self):
        self.write(b"\0asm", incompressible(2000))
        code, out = self.run_check(page(self.tmp, wasm="1 KB", js="1 KB"))
        self.assertEqual(code, 1, out)
        self.assertIn("excali_editor.js", out)

    def test_a_missing_file_fails(self):
        (self.dist / "excali_editor.js").write_bytes(b"x")
        code, out = self.run_check(page(self.tmp))
        self.assertEqual(code, 1, out)
        self.assertIn("excali_editor_bg.wasm missing", out)

    def test_usage(self):
        with redirect_stderr(io.StringIO()):
            self.assertEqual(wasm_size.main([]), 2)
            self.assertEqual(wasm_size.main(["check"]), 2)

    def test_default_page_is_the_phases_page(self):
        self.write(b"\0asm", b"x")
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            code = wasm_size.main(["check", str(self.dist)])
        self.assertEqual(code, 0, out.getvalue())
        self.assertIn("budget 1,500,000", out.getvalue())


if __name__ == "__main__":
    os.chdir(ROOT)
    unittest.main()
