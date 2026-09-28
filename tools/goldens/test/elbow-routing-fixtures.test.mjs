// excali-editor's elbow routing fixture is upstream's output:
// tools/goldens/elbow-routing-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "elbow-routing-fixtures.mjs");
const NAME = "elbow-routing.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "elbow-routing-fixtures-test-"));
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
  assert.ok(
    first.equals(readFileSync(COMMITTED)),
    "stale: run node tools/goldens/elbow-routing-fixtures.mjs",
  );
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("elbow-routing.json is upstream's updateElbowArrowPoints", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  const result = (id) => byId.get(id).result;

  // elbowArrow.test.tsx "can properly generate orthogonal arrow points"
  const orthogonal = result("upstream-orthogonal-points");
  assert.deepEqual(orthogonal.points, [
    [0, 0],
    [0, 100],
    [90, 100],
    [90, 200],
  ]);
  assert.deepEqual([orthogonal.x, orthogonal.y, orthogonal.width, orthogonal.height], [-45, -100.1, 90, 200]);

  // the no-op short circuit returns an empty update (elbowArrow.ts:1084-1096)
  assert.deepEqual(result("edge-noop-short-circuit"), {});
  // fewer than two points: the points pass through (elbowArrow.ts:922-924)
  assert.deepEqual(Object.keys(result("edge-one-point")), ["points"]);
  // a resize with fixed segments returns the updates (elbowArrow.ts:1145-1147)
  const resize = byId.get("fixed-stair-unbound-resize");
  assert.deepEqual(resize.result, resize.updates);
  // coordinates are clamped to 1e6 (elbowArrow.ts:2140-2146)
  assert.ok(result("unbound-clamped").x <= 1e6);

  // coverage: bound scenes of every bindable type, and special endpoints
  const types = new Set(
    cases.filter((c) => c.id.startsWith("bound-")).flatMap((c) => c.elements.map((e) => e.type)),
  );
  for (const type of ["rectangle", "diamond", "ellipse", "text", "image", "frame", "stickynote", "iframe", "embeddable"]) {
    assert.ok(types.has(type), `no bound case with a ${type}`);
  }
  assert.ok(cases.some((c) => c.result?.startIsSpecial === true));
  assert.ok(cases.some((c) => c.result?.endIsSpecial === true));

  // where upstream throws, the case records its message instead of a result
  // (handleEndpointDrag, elbowArrow.ts:752-757)
  const throwing = cases.filter((c) => "error" in c);
  assert.deepEqual(
    throwing.map((c) => [c.id, c.error, "result" in c]),
    ["null", "false", "true"].map((flag) => [
      `throw-drag-start-special-${flag}`,
      `Second and third points must exist when handling endpoint drag (${flag})`,
      false,
    ]),
  );
  for (const c of cases) assert.equal(c.id.startsWith("throw-"), "error" in c, c.id);
});
