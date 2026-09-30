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
| Calibration workload, reference | {calibration} | The pinned runner. |

## Next
"""


def page(
    tmp: Path, paint: str = "300 ms", pan: str = "16.7 ms", calibration: str = "100 ms"
) -> Path:
    p = tmp / "phases.md"
    p.write_text(
        TABLE.format(paint=paint, pan=pan, calibration=calibration), encoding="utf-8"
    )
    return p


class BudgetsTest(unittest.TestCase):
    def test_the_phases_page_budgets(self):
        # site/content/plan/phases.md#budgets is the single source.
        self.assertEqual(
            perf_budget.budgets(ROOT / "site/content/plan/phases.md"),
            {"firstPaintMs": 300.0, "panFrameP95Ms": 16.7},
        )

    def test_the_calibration_reference(self):
        # the calibration workload's time on the pinned runner when the
        # budgets were set, from the same table
        self.assertGreater(
            perf_budget.calibration_reference(ROOT / "site/content/plan/phases.md"), 0
        )
        with tempfile.TemporaryDirectory() as t:
            self.assertEqual(
                perf_budget.calibration_reference(page(Path(t), calibration="≤ 80.5 ms")),
                80.5,
            )

    def test_a_missing_or_zero_calibration_reference_is_an_error(self):
        with tempfile.TemporaryDirectory() as t:
            p = page(Path(t))
            p.write_text(
                p.read_text().replace("| Calibration workload", "| Something"), encoding="utf-8"
            )
            with self.assertRaisesRegex(ValueError, "Calibration workload"):
                perf_budget.calibration_reference(p)
            with self.assertRaisesRegex(ValueError, "Calibration workload"):
                perf_budget.calibration_reference(page(Path(t), calibration="0 ms"))

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
                + TABLE.format(paint="300 ms", pan="16.7 ms", calibration="100 ms")
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

    def results(self, paint=120.5, pan=9.25, paint_cal=100, pan_cal=100, **extra) -> dict:
        return {
            "firstPaintMs": paint,
            "firstPaintCalibrationMs": paint_cal,
            "panFrameP95Ms": pan,
            "panCalibrationMs": pan_cal,
            **extra,
        }

    def test_within_budget_passes_and_reports(self):
        code, out = self.run_check(self.results())
        self.assertEqual(code, 0, out)
        self.assertIn("First paint after module load: 120.5 ms", out)
        self.assertIn("calibration 100 ms", out)
        self.assertIn("budget 16.7 ms", out)

    def test_exactly_at_the_budget_passes(self):
        code, out = self.run_check(self.results(paint=300, pan=16.7))
        self.assertEqual(code, 0, out)

    def test_a_slow_pan_fails(self):
        code, out = self.run_check(self.results(paint=100, pan=16.71))
        self.assertEqual(code, 1, out)
        self.assertIn("Pan frame at 1,000 elements is over budget", out)

    def test_a_slow_first_paint_fails(self):
        code, out = self.run_check(self.results(paint=300.01, pan=5))
        self.assertEqual(code, 1, out)
        self.assertIn("First paint after module load is over budget", out)

    def test_a_slower_runner_is_held_to_the_same_budget(self):
        # twice the reference calibration: twice the milliseconds are the
        # same normalised time
        code, out = self.run_check(self.results(paint=600, pan=33.4, paint_cal=200, pan_cal=200))
        self.assertEqual(code, 0, out)
        self.assertIn("600 ms raw", out)
        self.assertIn("300 ms normalised", out)
        self.assertIn("ratio 3", out)
        code, out = self.run_check(self.results(paint=601, pan=10, paint_cal=200, pan_cal=200))
        self.assertEqual(code, 1, out)
        self.assertIn("First paint after module load is over budget", out)

    def test_a_faster_runner_gets_less_time(self):
        # half the reference calibration: 151 ms is 302 ms at the reference
        code, out = self.run_check(self.results(paint=151, pan=5, paint_cal=50, pan_cal=100))
        self.assertEqual(code, 1, out)
        self.assertIn("First paint after module load is over budget", out)

    def test_each_measurement_uses_its_own_calibration(self):
        # the pan's calibration is slow, the first paint's is not
        code, out = self.run_check(self.results(paint=150, pan=30, paint_cal=100, pan_cal=200))
        self.assertEqual(code, 0, out)
        code, out = self.run_check(self.results(paint=150, pan=30, paint_cal=200, pan_cal=100))
        self.assertEqual(code, 1, out)
        self.assertIn("Pan frame at 1,000 elements is over budget", out)

    def test_the_normalisation_is_in_the_report(self):
        self.assertAlmostEqual(perf_budget.normalise(150, 200, 100), 75)
        self.assertAlmostEqual(perf_budget.normalise(16.7, 50, 100), 33.4)

    def test_the_page_sets_the_limit(self):
        code, out = self.run_check(self.results(paint=100, pan=12), page(self.tmp, pan="10 ms"))
        self.assertEqual(code, 1, out)

    def test_the_page_sets_the_reference(self):
        code, out = self.run_check(self.results(), page(self.tmp, calibration="1000 ms"))
        self.assertEqual(code, 1, out)

    def test_a_missing_or_bad_measurement_fails(self):
        for results in ({"firstPaintMs": 100}, self.results(pan=None), self.results(paint="fast"),
                        self.results(paint=True)):
            code, out = self.run_check(results)
            self.assertEqual(code, 1, (results, out))
            self.assertIn("perf budget: no", out)

    def test_a_missing_or_bad_calibration_fails(self):
        for results in (self.results(paint_cal=None), self.results(pan_cal=0),
                        self.results(pan_cal=-1), self.results(paint_cal=float("inf"))):
            results = {k: v for k, v in results.items() if v is not None}
            code, out = self.run_check(results)
            self.assertEqual(code, 1, (results, out))
            self.assertIn("perf budget: no", out)

    def test_usage(self):
        out = io.StringIO()
        with redirect_stderr(out):
            self.assertEqual(perf_budget.main(["measure"]), 2)


if __name__ == "__main__":
    unittest.main()
