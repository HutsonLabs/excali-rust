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
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);

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
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(
    first.equals(readFileSync(COMMITTED)),
    `${NAME} is stale: run node tools/goldens/restore-fixtures.mjs`,
  );
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
