// excali-core's AppState goldens (ex-106) are upstream's output:
// tools/goldens/app-state.mjs regenerates them from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "app-state.mjs");
const FILE = "app-state.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "app-state-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/app-state.mjs");
});

test("the fixture pins the acceptance of ex-106", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const exported = ["gridSize", "gridStep", "gridModeEnabled", "viewBackgroundColor", "lockedMultiSelections"];
  const every = g.storage.find((c) => c.id === "every-key");
  assert.deepEqual(every.export, exported);
  assert.deepEqual(every.server, exported);
  // Each environment is its own bundle: exportScale follows devicePixelRatio
  // (appState.ts:20-22) and test mode rounds nothing (appState.ts:45).
  assert.deepEqual(
    g.defaults.map((d) => [d.devicePixelRatio, d.mode, d.appState.exportScale, d.appState.currentItemRoundness]),
    [
      [1, "production", 1, "round"],
      [2, "production", 2, "round"],
      [3, "production", 3, "round"],
      [1.5, "production", 1, "round"],
      [1, "test", 1, "sharp"],
    ],
  );
  const byId = Object.fromEntries(g.restore.map((c) => [c.id, c]));
  assert.deepEqual(byId["upstream-zoom-number"].result.changed, { zoom: { value: 10 } });
  assert.deepEqual(byId["upstream-open-sidebar-other-string"].result.changed, {
    openSidebar: { name: "default" },
  });
  assert.equal(byId["active-tool-null"].error, "TypeError: Cannot read properties of null (reading 'type')");
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"gridSize": 20', '"gridSize": 21'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /app-state\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
