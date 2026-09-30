// excali-ui's convert element type popup fixture and stylesheet (ex-535)
// are upstream's output: tools/goldens/convert-popup.mjs regenerates both
// from the pinned checkout, byte-stable across runs, and --check fails when
// a committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "convert-popup.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "convert-popup.json"), join("src", "convert_popup", "convert_popup.css")];

const scratch = mkdtempSync(join(tmpdir(), "convert-popup-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `${f} is stale: run node tools/goldens/convert-popup.mjs`);
  }
});

test("--check passes on the committed files", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("the popup lists the types of the selection's kind, the current one pressed", () => {
  const fixture = JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
  const byName = new Map(fixture.cases.map((c) => [c.name, c]));
  const buttons = (name) => byName.get(name).dom[0].children;
  const ids = (name) => buttons(name).map((b) => b.attrs["data-testid"]);
  const pressed = (name) => buttons(name).map((b) => b.attrs["aria-pressed"] === "true");
  assert.deepEqual(ids("rectangle"), ["toolbar-rectangle", "toolbar-diamond", "toolbar-ellipse"]);
  assert.deepEqual(ids("line"), ["toolbar-line", "toolbar-sharpArrow", "toolbar-curvedArrow", "toolbar-elbowArrow"]);
  assert.deepEqual(pressed("diamond"), [false, true, false]);
  assert.deepEqual(pressed("mixed-generic"), [false, false, false]);
  assert.deepEqual(pressed("elbow-arrow"), [false, false, false, true]);
  for (const c of fixture.cases) {
    const root = c.dom[0];
    assert.equal(root.attrs.class, "ConvertElementTypePopup");
    assert.equal(root.attrs.tabindex, "-1");
    assert.ok(root.style.top.endsWith("px") && root.style.left.endsWith("px"), c.name);
  }
});
