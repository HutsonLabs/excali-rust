// excali-editor's arrow endpoint text fixture is upstream's output:
// tools/goldens/arrow-endpoint-text-fixtures.mjs regenerates it from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "arrow-endpoint-text-fixtures.mjs");
const NAME = "arrow-endpoint-text.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "arrow-endpoint-text-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/arrow-endpoint-text-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("the fixture holds arrowEndpointTextBinding.test.tsx's answers", () => {
  const f = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const binding = (id) => f.bindings.find((c) => c.id === id).result;
  // :255-306, placement strategy
  assert.deepEqual(binding("up").fixedPoint, [0.5001, 1]);
  assert.deepEqual(binding("right").fixedPoint, [0, 0.5001]);
  assert.deepEqual(binding("down").fixedPoint, [0.5001, 0]);
  assert.deepEqual(binding("left").fixedPoint, [1, 0.5001]);
  // :308-321, the start point
  assert.equal(binding("start-of-right").textAlign, "right");
  // :420-432, left-bound drag
  const drag = f.drags.find((c) => c.id === "left-bound").result;
  assert.deepEqual(drag, { x: 306, width: 214, autoResize: false });
  assert.ok(f.endpointAt.some((c) => c.result === null) && f.endpointAt.some((c) => c.result?.startOrEnd === "start"));
});
