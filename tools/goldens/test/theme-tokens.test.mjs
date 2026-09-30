// excali-ui's theme token fixture (ex-532) is upstream's output:
// tools/goldens/theme-tokens.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate the design system's tokens
// (research/ui-design-system.md 1.1, theme.scss:5-278) so a generator that
// lost rules would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { rules } from "../theme-tokens.mjs";
import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "theme-tokens.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FIXTURE = join("tests", "fixtures", "theme-tokens.json");

const scratch = mkdtempSync(join(tmpdir(), "theme-tokens-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FIXTURE), "utf8"));

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FIXTURE));
  assert.ok(first.equals(readFileSync(join(outs[1], FIXTURE))), "differs between runs");
  assert.ok(first.equals(readFileSync(join(CRATE, FIXTURE))), "stale: run node tools/goldens/theme-tokens.mjs");
  assert.equal(run(["--check"]).status, 0);
});

test("the primary colours, light and dark (theme.scss:86-91, 231-236)", () => {
  const { light, dark } = committed();
  const primary = (set) => Object.fromEntries(set.filter(([k]) => k.startsWith("--color-primary")));
  assert.deepEqual(primary(light), {
    "--color-primary": "#6965db",
    "--color-primary-darker": "#5b57d1",
    "--color-primary-darkest": "#4a47b1",
    "--color-primary-light": "#e3e2fe",
    "--color-primary-light-darker": "#d7d5ff",
    "--color-primary-hover": "#5753d0",
  });
  assert.deepEqual(primary(dark), {
    "--color-primary": "#a8a5ff",
    "--color-primary-darker": "#b2aeff",
    "--color-primary-darkest": "#beb9ff",
    "--color-primary-light": "#4f4d6f",
    "--color-primary-light-darker": "#43415e",
    "--color-primary-hover": "#bbb8ff",
  });
});

test("the other rules and the container's inline tokens", () => {
  const f = committed();
  assert.deepEqual(f.mobile, [["--editor-container-padding", "0.75rem"]]);
  assert.equal(f.largeScreen.length, 4);
  assert.deepEqual(f.container, {
    "--right-sidebar-width": "302px",
    "--ui-pointerEvents": "all",
    "--zen-mode-transition-duration": "250ms",
  });
  // every dark token overrides a light one
  const light = new Set(f.light.map(([k]) => k));
  for (const [k] of f.dark) assert.ok(light.has(k), k);
});

test("rules: selectors, @media nesting, multi-line and repeated declarations", () => {
  const css = [
    ".a {",
    "  --x: 1;",
    "  --y: 0 0 1px red,",
    "    0 0 2px blue;",
    "  --x: 2;",
    "  color: red;",
    "}",
    "@media screen {",
    "  .a {",
    "    --x: 3;",
    "  }",
    "}",
  ].join("\n");
  assert.deepEqual(rules(css), {
    ".a": [["--x", "1"], ["--y", "0 0 1px red, 0 0 2px blue"], ["--x", "2"]],
    "@media screen .a": [["--x", "3"]],
  });
});
