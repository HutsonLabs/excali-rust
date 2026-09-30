// excali-editor's stats panel edit fixture (ex-539) is upstream's output:
// tools/goldens/stats-edits.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate, from the cited upstream files
// (components/Stats/*.tsx), what a few gestures must have done, so a
// harness that stopped driving the inputs would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "stats-edits.mjs");
const FIXTURE = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", "stats_edits.json");

const scratch = mkdtempSync(join(tmpdir(), "stats-edits-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });
const committed = () => JSON.parse(readFileSync(FIXTURE, "utf8"));
const byName = (name) => committed().cases.find((c) => c.name === name);
const changed = (name, id) => byName(name).result.changed.find((e) => e.id === id);
const scene = (id) => committed().scene.find((e) => e.id === id);

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], "stats_edits.json"));
  assert.ok(first.equals(readFileSync(join(outs[1], "stats_edits.json"))), "differs between runs");
  assert.ok(first.equals(readFileSync(FIXTURE)), "stale: run node tools/goldens/stats-edits.mjs");
});

test("every gesture is captured once, IMMEDIATELY (DragInput.tsx:167-169, 311-313)", () => {
  for (const c of committed().cases) assert.deepEqual(c.result.captures, ["IMMEDIATELY"], c.name);
});

test("a typed X moves the element, its label and its bound arrow (Position.tsx:131-142)", () => {
  assert.equal(changed("r-x", "r").x, 42.5);
  assert.deepEqual(byName("box-x").result.changed.map((e) => e.id).sort(), ["arr", "box", "lbl"]);
  assert.equal(changed("box-x", "lbl").x - changed("box-x", "box").x, scene("lbl").x - scene("box").x);
});

test("a drag: whole pixels, Shift on steps of 10 (Position.tsx:145-170)", () => {
  // pointer 100 -> 103 -> 110 -> 104: accumulated +4
  assert.equal(changed("r-x-drag", "r").x, scene("r").x + 4);
  // getStepSizedValue(10 + 18, 10) = 30
  assert.equal(changed("r-x-drag-shift", "r").x, 30);
});

test("an image keeps its aspect ratio; a frame takes the elements now inside it (Dimension.tsx)", () => {
  assert.equal(changed("img-w", "img").height, 160);
  assert.equal(changed("f-w", "out").frameId, "f");
  assert.deepEqual(byName("f-w-drag").result.patches.at(-1), { elementsToHighlight: null });
});

test("font sizes: at least 4, a sticky note's label takes a base font size (FontSize.tsx)", () => {
  assert.equal(changed("t-f-min", "t").fontSize, 4);
  assert.ok(changed("note-f", "noteText").baseFontSize !== scene("noteText").baseFontSize);
});
