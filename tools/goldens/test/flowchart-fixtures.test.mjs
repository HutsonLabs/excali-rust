// excali-editor's flowchart fixture is upstream's output:
// tools/goldens/flowchart-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "flowchart-fixtures.mjs");
const NAME = "flowchart.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "flowchart-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/flowchart-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("flowchart.json covers creation, the frame rule and the walk", () => {
  const fixture = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(fixture.create.map((c) => [c.id, c]));

  // one Ctrl+Arrow: a node 100 px to the right of a 100 x 60 node, then
  // its elbow arrow bound at both ends
  const [node, arrow] = byId.get("single-right").steps[0].pending;
  assert.deepEqual([node.x, node.y, node.width, node.height], [200, 0, 100, 60]);
  assert.equal(arrow.elbowed, true);
  assert.equal(arrow.startBinding.elementId, "a");
  assert.equal(arrow.endBinding.elementId, node.id);

  // pressing on in the same direction grows the cluster: 1, 2, 3 nodes
  const grow = byId.get("grow-right").steps.map((s) => s.pending.length / 2);
  assert.deepEqual(grow, [1, 2, 3]);

  // the pending nodes join the frame only when each overlaps it
  const frames = (id) => byId.get(id).steps.map((s) => s.pending.every((e) => e.frameId === "f"));
  assert.deepEqual(frames("frame-inside")[0], true);
  assert.deepEqual(frames("frame-outside"), [false, false, false]);

  // the walk finds nodes, cycles, and runs dry
  const explores = fixture.navigate.flatMap((c) => c.steps.filter((s) => s.op === "explore"));
  assert.ok(explores.filter((s) => s.result !== null).length > 100);
  assert.ok(explores.some((s) => s.result === null));
  assert.ok(fixture.inFlowchart.some((c) => c.result) && fixture.inFlowchart.some((c) => !c.result));
});
