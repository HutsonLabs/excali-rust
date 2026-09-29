// excali-editor's binding fixture is upstream's output:
// tools/goldens/binding-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "binding-fixtures.mjs");
const NAME = "binding.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "binding-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) =>
  spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8", env: process.env });

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/binding-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("binding.json holds binding.ts's constants and history.test.tsx's answer", () => {
  const fixture = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const { constants, normalizeFixedPoint, cases } = fixture;
  // BASE_BINDING_GAP 5 + strokeWidth / 2; the binding distance 15, up to 30
  assert.equal(constants.BASE_BINDING_GAP, 5);
  assert.deepEqual(
    constants.gaps.find((g) => g.strokeWidth === 4),
    { strokeWidth: 4, gap: 7 },
  );
  assert.deepEqual(
    constants.distances.map((d) => d.distance),
    [15, 30, 30, 30, 20, 15, 15, 15, 15, 15, 15, 15, 15],
  );
  // an exact 0.5 becomes 0.5001
  assert.deepEqual(normalizeFixedPoint[0], { input: [0.5, 0.5], isFixedPoint: true, result: [0.5001, 0.5001] });

  // "should update bound element points when rectangle was remotely moved
  // and arrow is added back through the history" (history.test.tsx:5107):
  // the arrow's second point, rounded to the nearest hundred
  const remote = cases.find((c) => c.id === "history-remote-move");
  const arrow = remote.ops[0].changes.find((e) => e.id === "arrow");
  const round = (n) => Math.round(n / 100) * 100;
  assert.deepEqual(arrow.points[1].map(round), [500, -400]);
});
