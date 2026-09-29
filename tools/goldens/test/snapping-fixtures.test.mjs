// excali-editor's snapping fixture is upstream's output:
// tools/goldens/snapping-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "snapping-fixtures.mjs");
const NAME = "snapping.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "snapping-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/snapping-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("snapping.json covers 8/zoom, every snap line kind and the snap colours", () => {
  const f = JSON.parse(readFileSync(COMMITTED, "utf8"));
  for (const { zoom, result } of f.snapDistance) assert.equal(result, 8 / zoom);
  const kinds = new Set();
  for (const c of f.cases) {
    for (const item of [...c.drags, ...c.resizes, ...c.news, ...c.pointers]) {
      for (const line of item.result.snapLines) kinds.add(`${line.type}/${line.direction ?? ""}`);
    }
  }
  assert.deepEqual([...kinds].sort(), ["gap/horizontal", "gap/vertical", "pointer/horizontal", "pointer/vertical", "points/"]);
  const colours = new Set(f.renders.flatMap((r) => r.calls.filter((c) => c[0] === "strokeStyle").map((c) => c[1])));
  assert.deepEqual([...colours].sort(), ["#da5b5b", "#ff6b6b", "#ff9090"]);
});
