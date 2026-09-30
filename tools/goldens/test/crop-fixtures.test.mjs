// excali-editor's crop fixture is upstream's output:
// tools/goldens/crop-fixtures.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "crop-fixtures.mjs");
const NAME = "crop.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "crop-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/crop-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("crop.json covers every handle, flip and the minimal size", () => {
  const f = JSON.parse(readFileSync(COMMITTED, "utf8"));
  assert.equal(f.minimalCropSize, 10);
  const handles = new Set(f.cases.map((c) => c.handle));
  assert.deepEqual([...handles].sort(), ["e", "n", "ne", "nw", "rotation", "s", "se", "sw", "w"]);
  const scales = new Set(f.images.map((i) => i.element.scale.join(",")));
  assert.deepEqual([...scales].sort(), ["-1,-1", "-1,1", "1,-1", "1,1"]);
  assert.ok(f.cases.some((c) => c.result.crop === null), "a crop back at the natural size resets to null");
  assert.ok(f.cases.some((c) => c.result.width === 10 || c.result.height === 10), "the minimal size is reached");
});
