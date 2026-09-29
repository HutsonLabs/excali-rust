// excali-ui's styles panel fixture (ex-519) is upstream's output:
// tools/goldens/styles-panel.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate Actions.tsx's layout independently
// (components/Actions.tsx:63-217) and hold the recorded trees to it, so a
// generator that lost cases or controls would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "styles-panel.mjs");
const FILE = "styles-panel.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "styles-panel-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/styles-panel.mjs");
});

const actions = (node) =>
  typeof node === "string" ? [] : node.action ? [node.action] : (node.children ?? []).flatMap(actions);

// Actions.tsx:150-216, the order the controls render in; [action, gates].
const FULL = [
  ["changeStrokeColor", ["strokeColor"]],
  ["changeBackgroundColor", ["backgroundColor"]],
  ["changeFillStyle", ["fill"]],
  ["changeStrokeWidth", ["strokeWidth"]],
  ["changeStrokeStyle", ["strokeStyle"]],
  ["changeFreedrawMode", ["freedrawMode"]],
  ["changeSloppiness", ["sloppiness"]],
  ["changeRoundness", ["roundness"]],
  ["changeArrowType", ["arrowType"]],
  ["changeFontFamily", ["text"]],
  ["changeFontSize", ["text"]],
  ["changeTextAlign", ["text", "textAlign"]],
  ["changeVerticalAlign", ["verticalAlign"]],
  ["changeArrowhead", ["arrowheads"]],
  ["changeOpacity", ["opacity"]],
  ["sendToBack", ["layers"]],
  ["sendBackward", ["layers"]],
  ["bringForward", ["layers"]],
  ["bringToFront", ["layers"]],
  ["alignLeft", ["align"]],
  ["alignHorizontallyCentered", ["align"]],
  ["alignRight", ["align"]],
  ["distributeHorizontally", ["align", "distribute"]],
  ["alignTop", ["align"]],
  ["alignVerticallyCentered", ["align"]],
  ["alignBottom", ["align"]],
  ["distributeVertically", ["align", "distribute"]],
  ["duplicateSelection", ["showExtraActions"]],
  ["deleteSelectedElements", ["showExtraActions"]],
  ["group", ["showExtraActions"]],
  ["ungroup", ["showExtraActions"]],
  ["hyperlink", ["showExtraActions", "link"]],
  ["cropEditor", ["showExtraActions", "cropEditor"]],
  ["toggleLinearEditor", ["showExtraActions", "lineEditor"]],
];

test("each tree renders the gated controls in Actions.tsx's order", () => {
  const { cases } = committed();
  for (const c of cases) {
    let expected;
    if (c.appState.activeTool.type === "bucketfill") {
      expected = ["changeBucketFillBackgroundColor", "changeFillStyle", "changeOpacity"];
    } else {
      expected = FULL.filter(([, gates]) => gates.every((g) => c.predicates[g])).map(([a]) => a);
      if (c.rtl) {
        const l = expected.indexOf("alignLeft");
        const r = expected.indexOf("alignRight");
        if (l >= 0) [expected[l], expected[r]] = [expected[r], expected[l]];
      }
    }
    assert.deepEqual(actions(c.tree[0]), expected, c.id);
  }
});

test("the panel root, its fieldsets and legends", () => {
  const { cases, locale } = committed();
  for (const c of cases) {
    const [root] = c.tree;
    assert.equal(root.tag, "div", c.id);
    assert.equal(root.class, "selected-shape-actions", c.id);
    const legends = root.children
      .filter((n) => n.tag === "fieldset" && n.children[0].tag === "legend")
      .map((n) => n.children[0].children[0]);
    const want = [
      c.predicates.layers && locale["labels.layers"],
      c.predicates.align && locale["labels.align"],
      c.predicates.showExtraActions && locale["labels.actions"],
    ].filter(Boolean);
    if (c.appState.activeTool.type !== "bucketfill") assert.deepEqual(legends, want, c.id);
  }
});

test("the island's max height is the app height less 166 px", () => {
  for (const w of committed().wrapper) {
    const island = w.tree[0].children[1];
    assert.equal(island.class, "Island App-menu__left");
    assert.equal(island.style.maxHeight, `${w.height - 166}px`);
    assert.equal(island.style["--padding"], 2);
  }
});
