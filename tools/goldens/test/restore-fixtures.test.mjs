// The restore fixtures of excali-core's base normalisation tests are
// upstream's output: tools/goldens/restore-fixtures.mjs regenerates them
// from the pinned checkout, byte-stable across runs, and --check fails when
// the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "restore-fixtures.mjs");
const NAME = "restore-base.json";
const ELEMENT_NAME = "restore-element.json";
const FIXTURES = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
const COMMITTED = join(FIXTURES, NAME);

const scratch = mkdtempSync(join(tmpdir(), "restore-fixtures-test-"));
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
  for (const name of [NAME, ELEMENT_NAME]) {
    const first = readFileSync(join(outs[0], name));
    assert.ok(first.equals(readFileSync(join(outs[1], name))), `${name} differs between runs`);
    assert.ok(
      first.equals(readFileSync(join(FIXTURES, name))),
      `${name} is stale: run node tools/goldens/restore-fixtures.mjs`,
    );
  }
});

test("restore-element.json is upstream's restoreElement in test mode", () => {
  const out = join(scratch, "e");
  assert.equal(run(["--out", out]).status, 0);
  const { cases } = JSON.parse(readFileSync(join(out, ELEMENT_NAME), "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  const result = (id) => byId.get(id).result;
  // restore.test.ts expectations, per element
  assert.equal(result("upstream-draw").type, "line");
  assert.equal(result("upstream-arrow-crowfoot").startArrowhead, "cardinality_one");
  assert.equal(result("upstream-arrow-crowfoot").endArrowhead, "cardinality_one_or_many");
  assert.equal(result("upstream-arrow-end-arrowhead-undefined").endArrowhead, "arrow");
  assert.equal(result("upstream-text-unknown-font-null-text").fontFamily, 5);
  assert.equal(result("upstream-text-unknown-font-null-text").isDeleted, true);
  assert.equal(result("upstream-text-non-finite-font-size").fontSize, 20);
  assert.equal(result("upstream-text-base-font-size-clamped").baseFontSize, 512);
  assert.deepEqual(result("upstream-freedraw-invalid-points").pressures, [0.1, 0.4]);
  const line = result("upstream-line-valid-points-only");
  assert.deepEqual([line.x, line.y, line.width, line.height], [12, 23, 3, 4]);
  const rebased = result("upstream-line-points-rebased");
  assert.deepEqual([rebased.x, rebased.y, rebased.points], [33, 44, [[0, 0], [2, 2]]]);
  for (const id of ["upstream-huge-line", "upstream-huge-arrow"]) {
    assert.equal(result(id).isDeleted, true, id);
    assert.deepEqual([result(id).width, result(id).height], [100, 100], id);
  }
  assert.equal(result("upstream-huge-normal-line").isDeleted, false);
  assert.equal(result("upstream-sticky-transparent-stroke").strokeColor, "#1e1e1e");
  assert.equal(result("upstream-arrow-binding-invalid-reference").endBinding, null);
  assert.equal(result("elbow-fixed-segments-three-points").fixedSegments, null);
  assert.equal(result("upstream-selection"), null);
  assert.deepEqual(byId.get("arrow-binding-legacy-migrated-inside").geometry, ["start", "end"]);
  assert.ok(cases.every((c) => c.call === "restoreElement"));
  assert.match(byId.get("text-font-null-throws").error, /reading 'split'/);
  for (const c of cases) assert.ok("result" in c !== "error" in c, c.id);
});

test("output is upstream's restore in test mode", () => {
  const out = join(scratch, "c");
  assert.equal(run(["--out", out]).status, 0);
  const { cases } = JSON.parse(readFileSync(join(out, NAME), "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));
  // randomId() is id0 after reseed, getUpdatedTimestamp() is 1.
  const bare = byId.get("minimal-type-only").result;
  assert.equal(bare.id, "id0");
  assert.equal(bare.updated, 1);
  assert.deepEqual(byId.get("strokeSharpness-round-rectangle").result.roundness, { type: 1 });
  assert.deepEqual(byId.get("strokeSharpness-round-diamond").result.roundness, { type: 2 });
  assert.deepEqual(byId.get("boundElementIds-to-boundElements").result.boundElements, [
    { type: "arrow", id: "a1" },
    { type: "arrow", id: "a2" },
  ]);
  const flipped = byId.get("negative-both").result;
  assert.deepEqual([flipped.x, flipped.y, flipped.width, flipped.height], [-30, -10, 40, 30]);
  assert.equal(byId.get("link-javascript").result.link, "about:blank");
  assert.match(byId.get("link-number-throws").error, /is not a function/);
  for (const c of cases) assert.ok("result" in c !== "error" in c, c.id);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace('"id": "id0"', '"id": "id9"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /restore-base\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
