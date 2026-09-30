// excali-editor's convert element type fixture is upstream's output:
// tools/goldens/convert-element-type-fixtures.mjs regenerates it from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "convert-element-type-fixtures.mjs");
const NAME = "convert-element-type.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "convert-element-type-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/convert-element-type-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("convert-element-type.json covers both cycles, labels and bindings", () => {
  const fixture = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(fixture.sessions.map((s) => [s.id, s]));
  const types = (id) => byId.get(id).steps.map((s) => s.elements[0].type);

  // convertElementType.test.tsx: the roundness type follows the shape
  const roundness = byId.get("roundness").steps.map((s) => s.elements[0].roundness.type);
  assert.deepEqual(roundness, [2, 3, 2]);
  // Tab cycles rectangle -> diamond -> ellipse -> rectangle, Shift+Tab back
  assert.deepEqual(types("cycle-right"), ["diamond", "ellipse", "rectangle", "diamond"]);
  assert.deepEqual(types("cycle-left"), ["ellipse", "diamond", "rectangle", "ellipse"]);
  // lines cycle line -> sharp -> curved -> elbow -> line
  const sub = (e) => (e.type === "line" ? "line" : e.elbowed ? "elbowArrow" : e.roundness ? "curvedArrow" : "sharpArrow");
  assert.deepEqual(
    byId.get("line-cycle-right").steps.map((s) => sub(s.elements[0])),
    ["sharpArrow", "curvedArrow", "elbowArrow", "line", "sharpArrow"],
  );
  // bound arrows follow the new outline
  for (const s of fixture.sessions.filter((c) => c.id.startsWith("binding-"))) {
    assert.ok(s.steps.every((st) => st.elements.some((e) => e.type === "arrow")));
  }
  // nothing convertible: convertElementTypes answers false
  assert.deepEqual(byId.get("none").steps.map((s) => s.result), [false, false]);
  assert.ok(fixture.lineToElbow.length > 60);
  assert.ok(fixture.conversionType.some((c) => c.result === null));
});
