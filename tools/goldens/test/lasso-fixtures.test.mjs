// excali-editor's lasso selection fixture is upstream's output:
// tools/goldens/lasso-fixtures.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "lasso-fixtures.mjs");
const NAME = "lasso.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "lasso-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/lasso-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("lasso.json holds every test of lasso.test.tsx and the seeded cases", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const upstream = cases.filter((c) => c.id.startsWith("upstream-"));
  // lasso.test.tsx has 17 tests (describe/it at the pinned commit)
  assert.equal(upstream.length, 17);
  assert.equal(cases.filter((c) => c.id.startsWith("random-")).length, 120);
  const last = (id) => {
    const ops = cases.find((c) => c.id === id).ops.filter((o) => o.state);
    return Object.keys(ops[ops.length - 1].state.selectedElementIds).sort();
  };
  // "selects only enclosed elements with the default mode ('contain')"
  assert.deepEqual(
    last(
      "upstream-box-selection-mode-through-the-lasso-tool-lasso-enclosing-one-element-and-cutting-through-another-selects-only-enclosed-elements-with-the-default-mode-contain",
    ),
    ["enclosed"],
  );
  // "selects intersected elements as well in 'overlap' mode"
  assert.deepEqual(
    last(
      "upstream-box-selection-mode-through-the-lasso-tool-lasso-enclosing-one-element-and-cutting-through-another-selects-intersected-elements-as-well-in-overlap-mode",
    ),
    ["crossed", "enclosed"],
  );
});
