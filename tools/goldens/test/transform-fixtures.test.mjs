// excali-editor's transform fixture is upstream's output:
// tools/goldens/transform-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "transform-fixtures.mjs");
const NAME = "transform.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "transform-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/transform-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("transform.json holds resize.test.tsx's answers", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  const last = (id) => {
    const c = byId.get(id);
    const state = new Map(c.elements.map((e) => [e.id, e]));
    for (const step of c.steps) for (const e of step.changed ?? []) state.set(e.id, e);
    return [...state.values()];
  };
  const close = (a, b) => Math.abs(a - b) < 0.005;
  const xywh = (e) => [e.x, e.y, e.width, e.height];
  const assertClose = (actual, expected, what) =>
    assert.ok(actual.every((v, i) => close(v, expected[i])), `${what}: ${actual} != ${expected}`);

  // "generic element resizes with handle $handle" (resize.test.tsx:87-114)
  for (const [handle, expected] of [
    ["n", [0, -27, 200, 127]],
    ["e", [0, 0, 267, 100]],
    ["s", [0, 0, 200, 61]],
    ["w", [20, 0, 180, 100]],
    ["ne", [0, -33, 205, 133]],
    ["se", [0, 0, 170, 19]],
    ["sw", [37, 0, 163, 125]],
    ["nw", [-34, 42, 234, 58]],
  ]) {
    assertClose(xywh(last(`upstream-generic-resize-${handle}`)[0]), expected, handle);
  }
  // "flips while resizing with handle $handle" (resize.test.tsx:116-143)
  for (const [handle, expected] of [
    ["n", [0, 100, 200, 39]],
    ["e", [-45, 0, 45, 100]],
    ["s", [0, -110, 200, 110]],
    ["w", [200, 0, 41, 100]],
    ["ne", [-50, 100, 50, 25]],
    ["se", [-83, 0, 83, 42]],
    ["sw", [40, -23, 160, 23]],
    ["nw", [200, 100, 70, 33]],
  ]) {
    assertClose(xywh(last(`upstream-generic-flip-${handle}`)[0]), expected, handle);
  }
  // "image element resizes from center" (resize.test.tsx:962-973)
  assertClose(xywh(last("upstream-image-center")[0]), [15, 15, 70, 70], "image center");
  // a sticky note's layout and a bound arrow's update are recorded
  const hooks = cases.flatMap((c) => (c.steps ?? []).flatMap((s) => s.hooks ?? []));
  assert.ok(hooks.some((h) => h.hook === "stickyNoteLayout"));
  assert.ok(hooks.some((h) => h.hook === "updateBoundElements" && h.changes.length > 0));
});
