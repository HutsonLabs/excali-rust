// excali-scene's dark-mode goldens (ex-218) are upstream's output:
// tools/goldens/dark-mode.mjs regenerates them from the pinned checkout,
// byte-stable across runs, --check fails when the committed file differs,
// and the palette results equal upstream's own vitest snapshot.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "dark-mode.mjs");
const FILE = "dark-mode.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "dark-mode-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

/**
 * One vitest snapshot of packages/common/src/__snapshots__/colors.test.ts.snap,
 * read as JSON (vitest writes trailing commas and nothing else JSON lacks).
 */
const snapshot = (name) => {
  const text = readFileSync(
    join(upstreamDir(), "packages", "common", "src", "__snapshots__", "colors.test.ts.snap"),
    "utf8",
  );
  const start = text.indexOf(`exports[\`${name}\`] = \``);
  assert.notEqual(start, -1, `no snapshot ${name}`);
  const body = text.slice(text.indexOf("`", text.indexOf("=", start)) + 1);
  return JSON.parse(body.slice(0, body.indexOf("`")).replace(/,(\s*[}\]])/g, "$1"));
};

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/dark-mode.mjs");
});

test("the palette and its filtered colours are upstream's vitest snapshots", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  assert.deepEqual(g.palette, snapshot("COLOR_PALETTE > color palette doesn't regress 1"));
  const dark = {};
  for (const c of g.paletteFilter) {
    if (c.index === null) dark[c.name] = c.dark;
    else (dark[c.name] ??= [])[c.index] = c.dark;
  }
  assert.deepEqual(
    dark,
    snapshot(
      "applyDarkModeFilter > COLOR_PALETTE regression tests > matches snapshot for all palette colors 1",
    ),
  );
  // 3 single colours and 12 families of 5 shades
  assert.equal(g.paletteFilter.length, 63);
  assert.equal(g.filter, "invert(93%) hue-rotate(180deg)");
});

test("the fixture pins upstream's colors.test.ts expectations", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const apply = Object.fromEntries(g.apply.map((c) => [c.color, c.result]));
  assert.equal(apply["#000000"], "#ededed");
  assert.equal(apply["#ffffff"], "#121212");
  assert.equal(apply["#ff0000"], "#ff9090");
  assert.equal(apply["#00ff00"], "#008f00");
  assert.equal(apply["#0000ff"], "#cdcdff");
  assert.equal(apply["rgba(255, 0, 0, 0.5)"], "#ff909080");
  assert.equal(apply.transparent, "#ededed00");
  assert.ok(g.apply.every((c) => c.disabled === c.color));
  const remove = Object.fromEntries(g.remove.map((c) => [c.color, c.result]));
  assert.equal(remove["#ededed"], "#000000");
  assert.equal(remove["#121212"], "#ffffff");
  for (const color of ["#1e1e1e", "#ffffff", "#e03131", "#2f9e44", "#1971c2"]) {
    const c = g.paletteFilter.find((p) => p.color === color);
    assert.equal(c.redark, c.dark, color);
  }
  const hex = Object.fromEntries(g.rgbToHex.map((c) => [JSON.stringify(c.args), c.result]));
  assert.equal(hex["[255,0,0,0.05]"], "#ff00000d");
  assert.equal(hex["[255,0,0,0.99]"], "#ff0000fc");
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"dark":"#d3d3d3"', '"dark":"#d3d3d4"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /dark-mode\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
