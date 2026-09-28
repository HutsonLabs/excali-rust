// The scene-level restore fixture of excali-core is upstream's output:
// tools/goldens/restore-elements-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "restore-elements-fixtures.mjs");
const NAME = "restore-elements.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "restore-elements-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

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
    "stale: run node tools/goldens/restore-elements-fixtures.mjs",
  );
});

test("restore-elements.json is upstream's restoreElements in test mode", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  const result = (id) => byId.get(id).result;
  const ids = (id) => result(id).map((e) => e.id);
  // restore.test.ts expectations
  assert.equal(result("upstream-basic").length, 2);
  assert.deepEqual(
    result("upstream-duplicate-ids-created").map((e) => e.created),
    [123, null],
  );
  assert.equal(result("upstream-sticky-transparent-stroke")[0].strokeColor, "#1e1e1e");
  assert.deepEqual(result("upstream-selection"), []);
  assert.deepEqual(result("upstream-not-supported"), []);
  assert.equal(result("upstream-invisibly-small-rectangle")[0].isDeleted, true);
  assert.equal(result("upstream-text-font-ceiling-clamped")[0].baseFontSize, 512);
  assert.equal(result("upstream-text-font-ceiling-cleared")[0].baseFontSize, null);
  const seeded = result("upstream-sticky-label-seeded").find((e) => e.id === "label");
  assert.deepEqual([seeded.baseFontSize, seeded.strokeColor], [20, "#1e1e1e"]);
  assert.ok(result("upstream-sticky-color-drift").every((e) => e.strokeColor === "#e03131"));
  const note = result("upstream-sticky-refit").find((e) => e.id === "sticky");
  assert.equal(note.baseHeight, 250);
  assert.ok(note.height > 250);
  assert.equal(result("upstream-strip-failed-element").length, 1);
  assert.equal(result("upstream-imperceptibly-small-arrow")[0].isDeleted, true);
  assert.equal(result("upstream-keep-small-freedraw-line").length, 2);
  for (const type of ["arrow", "rectangle"]) {
    const restored = result(`upstream-repair-label-order-${type}`);
    assert.deepEqual(
      restored.map((e) => e.id),
      ["container", "label"],
    );
    assert.equal(restored[0].index, "b2f");
    assert.ok(restored[1].index > restored[0].index);
  }
  assert.equal(result("upstream-strip-arrow-binding")[0].endBinding, null);
  assert.deepEqual(result("upstream-remove-deleted-bindings-repair")[0].boundElements, []);
  assert.ok(result("upstream-missing-container-repair").every((e) => e.containerId === null));
  assert.deepEqual(ids("ids-three-duplicates"), ["r", "id0", "id1"]);
  assert.deepEqual(ids("order-label-before-container"), ["r", "t", "s"]);
  // the three probed calls are recorded with their results
  const hooks = cases.flatMap((c) => c.hooks ?? []).map((h) => h.hook);
  for (const hook of ["updateElbowArrowPoints", "refreshTextDimensions", "getStickyNoteLayout"]) {
    assert.ok(hooks.includes(hook), hook);
  }
  assert.match(byId.get("throws-null-element").error, /reading 'id'/);
  for (const c of cases) assert.ok("result" in c !== "error" in c, c.id);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace('"id": "id0"', '"id": "id9"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /restore-elements\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
