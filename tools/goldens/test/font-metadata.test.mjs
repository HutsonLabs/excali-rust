// excali-text's font metadata goldens (ex-301) are upstream's output:
// tools/goldens/font-metadata.mjs regenerates them from the pinned checkout,
// byte-stable across runs, --check fails when the committed file differs,
// and the fixture pins upstream's own textElement.test.ts expectations.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "font-metadata.mjs");
const FILE = "font-metadata.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "font-metadata-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/font-metadata.mjs");
});

test("the fixture pins upstream's textElement.test.ts expectations", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  // textElement.test.ts:188-210
  const px = g.lineHeightInPx.find((c) => c.fontSize === 20 && c.lineHeight === 1.25);
  assert.equal(px.px, 25);
  const lh = Object.fromEntries(g.families.map((f) => [f.id, f.lineHeight]));
  assert.equal(lh[5], 1.25);
  assert.equal(lh[3], 1.2);
  // ten families plus the two fallbacks with metrics, in JS key order
  assert.deepEqual(
    g.metadata.map((m) => m.id),
    [1, 2, 3, 5, 6, 7, 8, 9, 10, 100, 1000],
  );
  const excalifont = g.verticalOffset.find((c) => c.fontFamily === 5 && c.fontSize === 20);
  assert.equal(excalifont.lineHeightPx[0], 25);
  assert.ok(Math.abs(excalifont.offset[0] - 17.62) < 1e-12);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"ascender":1011', '"ascender":1012'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /font-metadata\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
