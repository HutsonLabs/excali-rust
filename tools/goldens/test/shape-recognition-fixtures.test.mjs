// excali-editor's shape recognition fixture is upstream's output:
// tools/goldens/shape-recognition-fixtures.mjs regenerates it from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "shape-recognition-fixtures.mjs");
const NAME = "shape-recognition.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "shape-recognition-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/shape-recognition-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("shape-recognition.json holds every call of recognizeShape.test.ts and every shape", () => {
  const { recognize, convert } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const upstream = recognize.filter((c) => c.id.startsWith("upstream-"));
  // it.each: 11 shapes over 4 (right) or 7 (any) angles x 4 scales x 2
  // noises = 400 calls; the other 11 tests make 96
  assert.equal(upstream.length, 496);
  const types = (list) => new Set(list.map((c) => c.type));
  for (const type of ["rectangle", "diamond", "ellipse", "line", "arrow", "freedraw"]) {
    assert.ok(types(upstream).has(type), `no upstream case recognizes ${type}`);
    assert.ok(types(recognize.filter((c) => c.id.startsWith("random-"))).has(type), `no random case recognizes ${type}`);
  }
  const elements = new Set(convert.map((c) => c.element?.type ?? null));
  for (const type of ["rectangle", "diamond", "ellipse", "line", "arrow", null]) {
    assert.ok(elements.has(type), `no convert case yields ${type}`);
  }
  // the < 60 arrow becomes a line; frames are found
  assert.ok(convert.some((c) => c.id.includes("-arrow-") && c.element?.type === "line"));
  assert.ok(convert.some((c) => c.element?.frameId));
  // the size gate skipped feature extraction
  assert.ok(recognize.some((c) => c.features === null) && recognize.some((c) => c.features !== null));
});
