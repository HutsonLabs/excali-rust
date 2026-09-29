// excali-editor's editing fixture is upstream's output:
// tools/goldens/editing-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "editing-fixtures.mjs");
const NAME = "editing.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "editing-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/editing-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("editing.json holds upstream's answers for the parity rows", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = (id) => cases.find((c) => c.id === id);

  // select-box: a box from (50, 50) to (450, 250) takes a and b
  const box = byId("within-three").selections[0];
  assert.deepEqual(box.box, [50, 50, 400, 200]);
  for (const r of box.results) assert.deepEqual(r.ids, ["a", "b"]);

  // tool-rectangle: adaptive roundness when the current item is round;
  // tool-ellipse: getCurrentItemRoundness gives it proportional roundness
  assert.deepEqual(byId("new-rectangle-round").element.roundness, { type: 3 });
  assert.deepEqual(byId("new-ellipse-round").element.roundness, { type: 2 });
  assert.equal(byId("new-arrow-default").element.endArrowhead, "arrow");
  assert.equal(byId("new-frame-default").element.name, null);

  // dragNewElement from (100, 100) to (250, 200): 150 x 100 at the press
  const drag = byId("drag-rectangle").drags[0];
  assert.deepEqual(drag.result, { x: 100, y: 100, width: 150, height: 100 });

  // tool-eraser: a stroke across the filled rectangle erases it alone
  assert.deepEqual(byId("eraser-parity").paths[0].steps.at(-1), ["r"]);
});
