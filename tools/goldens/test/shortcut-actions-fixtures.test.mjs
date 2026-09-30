// excali-editor's shortcut action fixture is upstream's output:
// tools/goldens/shortcut-actions-fixtures.mjs regenerates it from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "shortcut-actions-fixtures.mjs");
const NAME = "shortcut-actions.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "shortcut-actions-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/shortcut-actions-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("shortcut-actions.json holds upstream's answers", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = (id) => cases.find((c) => c.id === id);
  const element = (c, id) => c.result.elements.find((e) => e.id === id);

  // Esc on a selection clears it; in a nested group it steps out one level
  assert.deepEqual(byId("deselect-selected").result.appState, { selectedElementIds: {} });
  assert.equal(byId("deselect-editing-group-nested").result.appState.editingGroupId, "outer");

  // flipping two shapes swaps them; flipping only bound arrows swaps their heads
  const two = byId("flip-two-h");
  assert.equal(element(two, "ell").x, 560);
  const heads = element(byId("flip-bound-arrow-only-h"), "ab");
  assert.equal(heads.startArrowhead, "arrow");
  assert.equal(heads.endArrowhead, null);

  // locking several elements groups them under a fresh id
  const multi = byId("lock-multi").result;
  assert.deepEqual(multi.appState.lockedMultiSelections, { id0: true });
  assert.ok(multi.elements.filter((e) => e.id.startsWith("r")).every((e) => e.locked && e.groupIds.includes("id0")));
  assert.equal(byId("lock-nothing").result, null);

  // copy then paste styles
  assert.equal(byId("copy-shape").result.appState.toast.message, "Copied styles.");
  assert.equal(element(byId("paste-shape"), "e").strokeStyle, "dashed");
  assert.equal(byId("paste-nothing-copied").copied, "{}");

  // view mode and theme; a host with onThemeChange takes the theme
  assert.equal(byId("view-mode-on").result.appState.viewModeEnabled, true);
  assert.equal(byId("theme-dark").result.appState.theme, "dark");
  assert.equal(byId("theme-host").result, null);
});
