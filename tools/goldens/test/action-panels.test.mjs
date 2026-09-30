// excali-ui's action panels fixture (ex-540) is upstream's output:
// tools/goldens/action-panels.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate a few of actionProperties.tsx's rules
// independently (the fill's Alt-click zigzag, :683-693; the arrowhead
// pickers' labels, :1905-1935) and hold the recorded output to them.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "action-panels.mjs");
const FILE = "action-panels.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "action-panels-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/action-panels.mjs");
});

const panel = (fixture, id, mode, column) => {
  const c = fixture.cases.find((x) => x.id === id);
  assert.ok(c, id);
  return fixture.trees[c.panels[mode][fixture.columns.indexOf(column)]];
};

const walk = (nodes, out = []) => {
  for (const n of nodes) {
    if (typeof n !== "object") continue;
    out.push(n);
    walk(n.children ?? [], out);
  }
  return out;
};

test("Alt-click on hachure turns an all-hachure selection zigzag", () => {
  const fixture = committed();
  const hachure = (id) => walk(panel(fixture, id, "full", "changeFillStyle")).find((n) => n.attrs?.["data-testid"] === "fill-hachure");
  // all hachure: zigzag
  assert.deepEqual(hachure("x-select-h1").on.altClick, [{ update: "zigzag" }]);
  // mixed: hachure either way
  assert.deepEqual(hachure("x-select-z1-h1").on.click, [{ update: "hachure" }]);
  assert.equal(hachure("x-select-z1-h1").on.altClick, undefined);
  // all zigzag: the button shows zigzag, active
  const z = hachure("x-select-z1-z2");
  assert.equal(z.class, "active");
  assert.deepEqual(z.children, [{ icon: "FillZigZagIcon" }]);
});

test("the arrowhead pickers are labelled by position", () => {
  const fixture = committed();
  const triggers = walk(panel(fixture, "x-select-ar1", "full", "changeArrowhead")).filter((n) => n.attrs?.["aria-haspopup"] === "dialog");
  assert.deepEqual(
    triggers.map((n) => n.attrs["aria-label"]),
    ["arrowhead_start", "arrowhead_end"],
  );
});
