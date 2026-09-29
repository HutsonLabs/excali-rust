#!/usr/bin/env python3
"""Tests for scripts/gates/perf_budget.py (the performance budgets, ex-710).

Run: python3 scripts/gates/test_perf_budget.py -v
"""
from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import perf_budget  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]

TABLE = """
## Budgets

| Budget | Value | Basis |
|---|---|---|
| WASM module, no fonts, gzip | ≤ 1.5 MB | term.hut ships a 5.4 MB dmg. |
| First paint after module load | ≤ {paint} | Upstream shows its canvas immediately. |
| Pan frame at 1,000 elements, 95th percentile | ≤ {pan} | 60 fps. |

## Next
"""


def page(tmp: Path, paint: str = "300 ms", pan: str = "16.7 ms") -> Path:
    p = tmp / "phases.md"
    p.write_text(TABLE.format(paint=paint, pan=pan), encoding="utf-8")
    return p


class BudgetsTest(unittest.TestCase):
    def test_the_phases_page_budgets(self):
        # site/content/plan/phases.md#budgets is the single source.
        self.assertEqual(
            perf_budget.budgets(ROOT / "site/content/plan/phases.md"),
            {"firstPaintMs": 300.0, "panFrameP95Ms": 16.7},
        )

    def test_durations(self):
        self.assertEqual(perf_budget.parse_ms("≤ 300 ms"), 300.0)
        self.assertEqual(perf_budget.parse_ms("16.7 ms"), 16.7)
        for bad in ("fast", "1.5 MB", "300ms?", "0.3 s", ""):
            with self.assertRaises(ValueError, msg=bad):
                perf_budget.parse_ms(bad)

    def test_a_missing_row_is_an_error(self):
        with tempfile.TemporaryDirectory() as t:
            p = page(Path(t))
            p.write_text(p.read_text().replace("| Pan frame", "| Something"), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Pan frame at 1,000 elements"):
                perf_budget.budgets(p)

    def test_rows_outside_the_section_do_not_count(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / "phases.md"
            p.write_text(
                "| First paint after module load | ≤ 1 ms |\n"
                + TABLE.format(paint="300 ms", pan="16.7 ms")
                + "| Pan frame at 1,000 elements | ≤ 1 ms |\n",
                encoding="utf-8",
            )
            self.assertEqual(
                perf_budget.budgets(p), {"firstPaintMs": 300.0, "panFrameP95Ms": 16.7}
            )


class CheckTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def run_check(self, results: dict, phases: Path | None = None) -> tuple[int, str]:
        path = self.tmp / "perf.json"
        path.write_text(json.dumps(results), encoding="utf-8")
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            code = perf_budget.main(
                ["check", str(path), "--phases", str(phases or page(self.tmp))]
            )
        return code, out.getvalue()

    def test_within_budget_passes_and_reports(self):
        code, out = self.run_check({"firstPaintMs": 120.5, "panFrameP95Ms": 9.25})
        self.assertEqual(code, 0, out)
        self.assertIn("First paint after module load: 120.5 ms", out)
        self.assertIn("budget 16.7 ms", out)

    def test_exactly_at_the_budget_passes(self):
        code, out = self.run_check({"firstPaintMs": 300, "panFrameP95Ms": 16.7})
        self.assertEqual(code, 0, out)

    def test_a_slow_pan_fails(self):
        code, out = self.run_check({"firstPaintMs": 100, "panFrameP95Ms": 16.71})
        self.assertEqual(code, 1, out)
        self.assertIn("Pan frame at 1,000 elements is over budget", out)

    def test_a_slow_first_paint_fails(self):
        code, out = self.run_check({"firstPaintMs": 300.01, "panFrameP95Ms": 5})
        self.assertEqual(code, 1, out)
        self.assertIn("First paint after module load is over budget", out)

    def test_the_page_sets_the_limit(self):
        code, out = self.run_check(
            {"firstPaintMs": 100, "panFrameP95Ms": 12}, page(self.tmp, pan="10 ms")
        )
        self.assertEqual(code, 1, out)

    def test_a_missing_or_bad_measurement_fails(self):
        for results in ({"firstPaintMs": 100}, {"firstPaintMs": 100, "panFrameP95Ms": None},
                        {"firstPaintMs": "fast", "panFrameP95Ms": 1},
                        {"firstPaintMs": True, "panFrameP95Ms": 1}):
            code, out = self.run_check(results)
            self.assertEqual(code, 1, (results, out))
            self.assertIn("perf budget: no", out)

    def test_usage(self):
        out = io.StringIO()
        with redirect_stderr(out):
            self.assertEqual(perf_budget.main(["measure"]), 2)


if __name__ == "__main__":
    unittest.main()
