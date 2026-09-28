// Acceptance: the generator runs headless (plain node, no browser), writes
// goldens/*.json, and its output is byte-stable across two runs. The
// committed goldens must be exactly what the generator produces today.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { GENERATE, GOLDENS_DIR } from "./helpers.mjs";

const scratch = mkdtempSync(join(tmpdir(), "goldens-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATE, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

const snapshot = (dir) =>
  Object.fromEntries(
    readdirSync(dir)
      .sort()
      .map((f) => [f, readFileSync(join(dir, f))]),
  );

test("two runs produce byte-identical goldens equal to the committed ones", () => {
  const a = join(scratch, "a");
  const b = join(scratch, "b");
  for (const out of [a, b]) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = snapshot(a);
  const second = snapshot(b);
  assert.ok(Object.keys(first).length > 5);
  assert.deepEqual(Object.keys(second), Object.keys(first));
  for (const f of Object.keys(first)) {
    assert.ok(first[f].equals(second[f]), `${f} differs between runs`);
  }
  const committed = snapshot(GOLDENS_DIR);
  assert.deepEqual(Object.keys(committed), Object.keys(first), "committed file set");
  for (const f of Object.keys(first)) {
    assert.ok(
      first[f].equals(committed[f]),
      `goldens/${f} is stale: run node tools/goldens/generate.mjs` +
        (process.arch === "arm64" ? "" : ` (goldens are generated on arm64; this is ${process.arch})`),
    );
  }
});

test("--check passes on the committed goldens", () => {
  const ok = run(["--check"]);
  assert.equal(ok.status, 0, ok.stderr);
});

test("--check reports stale files by name", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, "random.json");
  writeFileSync(file, readFileSync(file, "utf8").replace('"seed": 1,', '"seed": 2,'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /random\.json/);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
