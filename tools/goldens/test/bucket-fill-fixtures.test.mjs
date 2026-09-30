// excali-editor's bucket fill fixture is upstream's output:
// tools/goldens/bucket-fill-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "bucket-fill-fixtures.mjs");
const NAME = "bucket-fill.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "bucket-fill-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/bucket-fill-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("bucket-fill.json has a case for every test of bucketFill.test.ts", () => {
  const source = readFileSync(join(upstreamDir(), "packages", "element", "tests", "bucketFill.test.ts"), "utf8");
  // plain `it("...")` plus the two rows of the one `it.each`
  const plain = [...source.matchAll(/^\s*it\(\s*"([^"]+)"/gm)].length;
  const each = [...source.matchAll(/^\s*it\.each\(/gm)].length;
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const upstream = cases.filter((c) => c.id.startsWith("upstream-"));
  assert.equal(each, 1);
  assert.equal(upstream.length, plain + 2);
  assert.ok(upstream.every((c) => c.calls.length > 0));
  const random = cases.filter((c) => c.id.startsWith("random-"));
  assert.equal(random.length, 64);
});
