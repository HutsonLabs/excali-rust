// excali-ui's primitive fixture and stylesheet (ex-516) are upstream's
// output: tools/goldens/ui-primitives.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for (the
// design system's sizes, research/ui-design-system.md 1.1) and upstream's
// rules (Dialog.tsx:34-47, Tooltip.tsx:20-60, Popover.tsx:85-130) and hold
// the recorded values to them, so a generator that lost cases would be
// noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "ui-primitives.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "ui-primitives.json"), join("src", "primitives", "primitives.css")];

const scratch = mkdtempSync(join(tmpdir(), "ui-primitives-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/ui-primitives.mjs`);
  }
});

test("the design system's sizes: 2rem buttons, 1rem icons, 0.25rem space factor, radii 0.375/0.5rem", () => {
  const { tokens, largeScreenTokens } = committed();
  assert.deepEqual(tokens, {
    "--default-button-size": "2rem",
    "--default-icon-size": "1rem",
    "--lg-button-size": "2.25rem",
    "--lg-icon-size": "1rem",
    "--space-factor": "0.25rem",
    "--border-radius-md": "0.375rem",
    "--border-radius-lg": "0.5rem",
  });
  assert.deepEqual(largeScreenTokens, {
    "--default-button-size": "2.25rem",
    "--default-icon-size": "1.25rem",
    "--lg-button-size": "2.375rem",
    "--lg-icon-size": "1.25rem",
  });
  const css = readFileSync(join(CRATE, FILES[1]), "utf8");
  for (const rule of [
    "padding: calc(var(--padding) * var(--space-factor));",
    "gap: calc(var(--space-factor) * var(--gap));",
    "border-radius: var(--border-radius-lg);",
    "width: var(--default-button-size);",
    "width: var(--default-icon-size);",
  ]) {
    assert.ok(css.includes(rule), rule);
  }
});

test("every primitive is rendered", () => {
  const components = new Set(committed().render.map((c) => c.component));
  for (const c of ["Island", "Stack.Row", "Stack.Col", "Button", "IconButton", "RadioGroup", "Range", "TextField", "Popover", "Modal", "Dialog", "Tooltip"]) {
    assert.ok(components.has(c), c);
  }
});

test("dialog widths: small 550, regular 800, wide 1024, a number as given", () => {
  const widths = { small: 550, regular: 800, wide: 1024, undefined: 800 };
  for (const c of committed().render.filter((r) => r.component === "Dialog")) {
    const want = typeof c.props.size === "number" ? c.props.size : widths[c.props.size];
    const content = c.dom[0].children[0].children[1];
    assert.equal(content.attrs.class, "Modal__content");
    assert.equal(content.style["--max-width"], `${want}px`, c.name);
  }
});

test("tooltips centre under the item, 5px from it and the viewport edges", () => {
  for (const c of committed().tooltipPosition) {
    let left = c.item.left + c.item.width / 2 - c.tooltip.width / 2;
    if (left < 0) left = 5;
    else if (left + c.tooltip.width >= c.viewport.width) left = c.viewport.width - c.tooltip.width - 5;
    let top;
    if (c.position === "bottom") {
      top = c.item.top + c.item.height + 5;
      if (top + c.tooltip.height >= c.viewport.height) top = c.item.top - c.tooltip.height - 5;
    } else {
      top = c.item.top - c.tooltip.height - 5;
      if (top < 0) top = c.item.top + c.item.height + 5;
    }
    assert.equal(c.left, `${left}px`);
    assert.equal(c.top, `${top}px`);
  }
});

test("popovers are clamped 10px inside the viewport", () => {
  for (const c of committed().popoverFit) {
    const maxWidth = Math.max(0, c.viewport.width - 20);
    if (c.content.width >= maxWidth) assert.equal(c.style.width, `${maxWidth}px`);
    else assert.equal(c.style.left, `${Math.min(Math.max(c.left, 10), c.viewport.width - 10 - c.content.width)}px`);
  }
});
