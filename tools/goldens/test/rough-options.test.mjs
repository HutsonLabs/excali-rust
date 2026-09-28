// excali-scene's rough option goldens (ex-207) are upstream's output:
// tools/goldens/rough-options.mjs regenerates them from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "rough-options.mjs");
const FILE = "rough-options.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "rough-options-test-"));
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
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/rough-options.mjs");
});

test("the fixture pins the option table of research/rendering.md section 1", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = Object.fromEntries(g.options.map((c) => [c.id, c]));
  // strokeLineDash, disableMultiStroke, strokeWidth, fillWeight, hachureGap
  assert.deepEqual(byId["stroke/dashed/sw2"].options.strokeLineDash, [8, 10]);
  assert.deepEqual(byId["stroke/dotted/sw2"].options.strokeLineDash, [1.5, 8]);
  assert.equal(byId["stroke/solid/sw2"].options.strokeLineDash, undefined);
  assert.equal(byId["stroke/dotted/sw4"].options.strokeWidth, 4.5);
  assert.equal(byId["stroke/dotted/sw4"].options.disableMultiStroke, true);
  assert.equal(byId["stroke/dotted/sw4"].options.fillWeight, 2);
  assert.equal(byId["stroke/dotted/sw4"].options.hachureGap, 16);
  // preserveVertices: continuousPath || roughness < 2
  assert.equal(byId["roughness/2/continuous-false"].options.preserveVertices, false);
  assert.equal(byId["roughness/2/continuous-true"].options.preserveVertices, true);
  // fill: isTransparent for shapes, a literal "transparent" for lines
  assert.equal(byId['fill/rectangle/"rgba(0,0,0,0)"/dark-false'].options.fill, undefined);
  assert.equal(
    byId['linear-fill/line/closed/"rgba(0,0,0,0)"/dark-false'].options.fill,
    "rgba(0,0,0,0)",
  );
  assert.equal(byId['linear-fill/arrow/closed/"#b2f2bb"/dark-false'].options.fill, undefined);
  assert.equal(byId['fill/ellipse/"#a5d8ff"/dark-false'].options.curveFitting, 1);
  assert.equal(byId["unimplemented/text"].error, "Error: Unimplemented type text");
  // adjustRoughness: the reduction and its 2.5 cap
  const small = g.adjustRoughness.find(
    (c) => c.type === "rectangle" && c.roundness === null && c.width === 0 && c.height === 0,
  );
  assert.equal(small.result, 2 / 3);
  assert.ok(g.adjustRoughness.some((c) => c.result === 2.5));
  assert.equal(g.darkMode.find((c) => c.color === "#1e1e1e").result, "#d3d3d3");
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"result":"#d3d3d3"', '"result":"#d3d3d4"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /rough-options\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
