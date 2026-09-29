// excali-editor's hit testing fixture is upstream's output:
// tools/goldens/collision-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "collision-fixtures.mjs");
const NAME = "collision.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "collision-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/collision-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("collision.json holds collision.test.tsx's answers", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  const probes = (id) => byId.get(id).probes;

  // "check rotated elements can be hit: arrow"
  assert.equal(probes("upstream-rotated-arrow")[0].hit, true);
  // "ellipse outline hit test"
  assert.equal(probes("upstream-ellipse-center-line")[0].hit, false);
  // "rechecks a bound label when only its arrow changes"
  assert.deepEqual(
    probes("upstream-cache-bound-label-before").map((p) => p.hit),
    [true, false],
  );
  assert.deepEqual(
    probes("upstream-cache-bound-label-after").map((p) => p.hit),
    [false, true],
  );
  // "hits thick ink outside the centerline bounds"
  assert.deepEqual(
    probes("upstream-freedraw-straight-variable").map((p) => p.hit),
    [true, true, true, true, false],
  );
  assert.deepEqual(
    probes("upstream-freedraw-straight-constant").map((p) => p.hit),
    [true, true, false, false, false],
  );
  // "contains nothing for ..." and "uses even-odd fill for a self-intersecting loop"
  for (const id of [
    "upstream-freedraw-retraced",
    "upstream-freedraw-two-identical-points",
    "upstream-freedraw-self-intersecting",
  ]) {
    assert.equal(probes(id)[0].inside, false, id);
  }
  // "binding hit tests"
  const binding = (id) => byId.get(id).points.map((p) => [p.hovered, p.all]);
  assert.deepEqual(binding("upstream-binding-zoom"), [
    [null, []],
    ["rect", ["rect"]],
  ]);
  assert.deepEqual(binding("upstream-binding-cover-transparent"), [["hidden", ["cover", "hidden"]]]);
  assert.deepEqual(binding("upstream-binding-locked-opaque"), [[null, []]]);
  // "hits a rounded diamond corner along a line through its apex"
  const diamond = byId.get("upstream-intersect-rounded-diamond");
  assert.ok(Math.abs(diamond.segments[0].result[0][0] - (diamond.leftMidpoint[0] - 6)) < 0.5);

  // every element type is probed
  const types = new Set(
    cases.filter((c) => c.kind === "probe").flatMap((c) => c.elements.map((e) => e.type)),
  );
  for (const type of [
    "rectangle",
    "diamond",
    "ellipse",
    "text",
    "image",
    "frame",
    "magicframe",
    "iframe",
    "embeddable",
    "stickynote",
    "line",
    "arrow",
    "freedraw",
  ]) {
    assert.ok(types.has(type), `no probe of a ${type}`);
  }
});
