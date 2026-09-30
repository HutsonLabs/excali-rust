#!/usr/bin/env python3
"""Performance budgets of the web runtime (ex-710).

The budgets are the rows of the table under "## Budgets" on
site/content/plan/phases.md, the single source, as for the size gate
(scripts/gates/wasm_size.py):

    | First paint after module load   | <= 300 ms |  -> firstPaintMs
    | Pan frame at 1,000 elements     | <= 16.7 ms | -> panFrameP95Ms
    | Calibration workload, reference | 100 ms |     the reference

The measurements are the JSON file tests/web/perf/perf.spec.mjs writes
(scripts/web/perf.sh runs it against the release in dist/). Each comes with
the median time of the calibration workload (tests/web/perf/page/
calibrate.html) run in the same browser just before it (firstPaint-
CalibrationMs, panCalibrationMs). The budgets hold on a machine whose
calibration takes the reference time, so a measurement is normalised to that
machine before it is compared:

    normalised = measured × reference / calibration

A normalised measurement at its budget passes; anything over fails, as does
a budget, the reference, a measurement or its calibration missing.

    python3 scripts/gates/perf_budget.py check RESULTS [--phases PAGE]

Exit 0 within budget, 1 over budget or a value missing, 2 usage error.
Tests: scripts/gates/test_perf_budget.py
"""
from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PHASES = ROOT / "site" / "content" / "plan" / "phases.md"

# Budget row label (its start, on the phases page) -> measurement key.
ROWS = {
    "First paint after module load": "firstPaintMs",
    "Pan frame at 1,000 elements": "panFrameP95Ms",
}

# The row holding the calibration workload's reference time.
CALIBRATION_ROW = "Calibration workload"

# Measurement key -> the key of its calibration.
CALIBRATIONS = {
    "firstPaintMs": "firstPaintCalibrationMs",
    "panFrameP95Ms": "panCalibrationMs",
}

DURATION = re.compile(r"^(?:≤\s*)?(\d+(?:\.\d+)?)\s*ms$")


def parse_ms(text: str) -> float:
    m = DURATION.match(text.strip())
    if not m:
        raise ValueError(f"'{text}' is not a duration in ms")
    return float(m.group(1))


def table_rows(phases: Path) -> list[list[str]]:
    """The cells of each row of the Budgets table."""
    text = phases.read_text(encoding="utf-8")
    section = text.split("\n## Budgets", 1)
    if len(section) != 2:
        raise ValueError(f"{phases}: no '## Budgets' section")
    body = section[1].split("\n## ", 1)[0]
    rows = []
    for line in body.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2:
            rows.append(cells)
    return rows


def budgets(phases: Path = PHASES) -> dict[str, float]:
    """Measurement key -> budget in milliseconds, from the Budgets table."""
    found: dict[str, float] = {}
    for cells in table_rows(phases):
        for label, key in ROWS.items():
            if cells[0].startswith(label):
                found[key] = parse_ms(cells[1])
    for label, key in ROWS.items():
        if key not in found:
            raise ValueError(f"{phases}: no '{label}' row in the Budgets table")
    return found


def calibration_reference(phases: Path = PHASES) -> float:
    """The calibration workload's time on the machine the budgets hold on."""
    for cells in table_rows(phases):
        if cells[0].startswith(CALIBRATION_ROW):
            ms = parse_ms(cells[1])
            if ms <= 0:
                raise ValueError(f"{phases}: the '{CALIBRATION_ROW}' reference is not positive")
            return ms
    raise ValueError(f"{phases}: no '{CALIBRATION_ROW}' row in the Budgets table")


def normalise(measured: float, calibration: float, reference: float) -> float:
    """`measured` on a machine whose calibration takes `reference`."""
    return measured * reference / calibration


def number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def check(results: Path, phases: Path = PHASES) -> int:
    limits = budgets(phases)
    reference = calibration_reference(phases)
    measured = json.loads(results.read_text(encoding="utf-8"))
    failed = False
    for label, key in ROWS.items():
        limit = limits[key]
        value = measured.get(key)
        cal_key = CALIBRATIONS[key]
        calibration = measured.get(cal_key)
        if not number(value):
            print(f"perf budget: no {key} in {results}; run scripts/web/perf.sh", file=sys.stderr)
            failed = True
            continue
        if not number(calibration) or calibration <= 0:
            print(f"perf budget: no {cal_key} in {results}; run scripts/web/perf.sh", file=sys.stderr)
            failed = True
            continue
        norm = normalise(value, calibration, reference)
        verdict = "ok" if norm <= limit else "OVER BUDGET"
        print(
            f"{label}: {value:g} ms raw ({key}), calibration {calibration:g} ms "
            f"(ratio {value / calibration:.4g}, reference {reference:g} ms), "
            f"{norm:.4g} ms normalised, budget {limit:g} ms ({norm / limit:.1%}) {verdict}"
        )
        if norm > limit:
            print(
                f"perf budget: {label} is over budget by {norm - limit:.2f} ms normalised "
                f"(site/content/plan/phases.md#budgets)",
                file=sys.stderr,
            )
            failed = True
    return 1 if failed else 0


def main(argv: list[str]) -> int:
    usage = "usage: perf_budget.py check RESULTS [--phases PAGE]"
    if len(argv) not in (2, 4) or argv[0] != "check":
        print(usage, file=sys.stderr)
        return 2
    phases = PHASES
    if len(argv) == 4:
        if argv[2] != "--phases":
            print(usage, file=sys.stderr)
            return 2
        phases = Path(argv[3])
    try:
        return check(Path(argv[1]), phases)
    except (OSError, ValueError) as e:
        print(f"perf budget: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
