#!/usr/bin/env python3
"""Performance budgets of the web runtime (ex-710).

The budgets are the rows of the table under "## Budgets" on
site/content/plan/phases.md, the single source, as for the size gate
(scripts/gates/wasm_size.py):

    | First paint after module load | <= 300 ms |    -> firstPaintMs
    | Pan frame at 1,000 elements   | <= 16.7 ms |   -> panFrameP95Ms

The measurements are the JSON file tests/web/perf/perf.spec.mjs writes
(scripts/web/perf.sh runs it against the release in dist/). A measurement at
its budget passes; anything over fails, as does a budget or a measurement
missing.

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

DURATION = re.compile(r"^(?:≤\s*)?(\d+(?:\.\d+)?)\s*ms$")


def parse_ms(text: str) -> float:
    m = DURATION.match(text.strip())
    if not m:
        raise ValueError(f"'{text}' is not a duration in ms")
    return float(m.group(1))


def budgets(phases: Path = PHASES) -> dict[str, float]:
    """Measurement key -> budget in milliseconds, from the Budgets table."""
    text = phases.read_text(encoding="utf-8")
    section = text.split("\n## Budgets", 1)
    if len(section) != 2:
        raise ValueError(f"{phases}: no '## Budgets' section")
    body = section[1].split("\n## ", 1)[0]
    found: dict[str, float] = {}
    for line in body.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 2:
            continue
        for label, key in ROWS.items():
            if cells[0].startswith(label):
                found[key] = parse_ms(cells[1])
    for label, key in ROWS.items():
        if key not in found:
            raise ValueError(f"{phases}: no '{label}' row in the Budgets table")
    return found


def check(results: Path, phases: Path = PHASES) -> int:
    limits = budgets(phases)
    measured = json.loads(results.read_text(encoding="utf-8"))
    failed = False
    for label, key in ROWS.items():
        limit = limits[key]
        value = measured.get(key)
        if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value):
            print(f"perf budget: no {key} in {results}; run scripts/web/perf.sh", file=sys.stderr)
            failed = True
            continue
        verdict = "ok" if value <= limit else "OVER BUDGET"
        print(f"{label}: {value:g} ms ({key}), budget {limit:g} ms ({value / limit:.1%}) {verdict}")
        if value > limit:
            print(
                f"perf budget: {label} is over budget by {value - limit:.2f} ms "
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
