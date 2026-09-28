// The library fixtures of excali-core's .excalidrawlib tests are upstream's
// output: tools/goldens/library-fixtures.mjs regenerates them from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "library-fixtures.mjs");
const NAME = "library.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "library-fixtures-test-"));
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
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(
    first.equals(readFileSync(COMMITTED)),
    `${NAME} is stale: run node tools/goldens/library-fixtures.mjs`,
  );
});

test("output is upstream's library codec in test mode", () => {
  const out = join(scratch, "c");
  assert.equal(run(["--out", out]).status, 0);
  const { parse, catalogue, merge, hash } = JSON.parse(readFileSync(join(out, NAME), "utf8"));
  const par = new Map(parse.map((c) => [c.id, c]));
  const items = (id) => JSON.parse(par.get(id).output).libraryItems;

  // library.test.tsx "import library via drag&drop": the v1 fixture gives
  // one unpublished item holding element "A".
  const [v1] = items("upstream-fixture-library-v1");
  assert.deepEqual(Object.keys(v1), ["status", "elements", "id", "created"]);
  assert.equal(v1.status, "unpublished");
  assert.equal(v1.elements[0].id, "A");
  assert.ok(!("strokeSharpness" in v1.elements[0]) && !("boundElementIds" in v1.elements[0]));
  assert.equal(items("upstream-fixture-library-v1-published")[0].status, "published");
  // restore.test.ts "restoreLibraryItems creation timestamps"
  for (const v of ["v1", "v2"]) {
    assert.deepEqual(
      items(`upstream-creation-timestamps-${v}`)[0].elements.map((e) => e.created),
      [null, 123, null],
    );
  }
  assert.equal(items("upstream-creation-timestamps-v2")[0].created, 456);

  assert.equal(par.get("version-3-invalid").error, "Invalid library");
  assert.equal(par.get("library-null-throws").error, "libraryItems is not iterable");
  assert.equal(par.get("item-elements-string-throws").error, "items.reduce is not a function");
  assert.deepEqual(items("item-some-deleted-filtered")[0].elements.map((e) => e.id), ["a", "c"]);
  assert.deepEqual(items("elements-duplicate-ids-renamed")[0].elements.map((e) => e.id), ["x", "id0", "id1"]);
  assert.equal(items("item-odd-values-kept")[0].status, "draft");

  assert.equal(catalogue.length, 232);
  const byId = new Map(catalogue.map((c) => [c.id, c]));
  assert.equal(byId.get("jumpingrivers/r").version, 1);
  assert.equal(byId.get("youritjang/stick-figures").version, 2);
  for (const c of catalogue) assert.match(c.output_sha256, /^[0-9a-f]{64}$/, c.id);

  const merged = (id) => JSON.parse(merge.find((c) => c.id === id).output).libraryItems.map((i) => i.id);
  assert.deepEqual(merged("identical-not-added"), ["A", "B"]);
  assert.deepEqual(merged("several-new-prepended-in-order"), ["C", "D", "A", "B"]);
  const hashes = new Map(hash.map((c) => [c.id, c.hash]));
  assert.equal(hashes.get("empty"), 5381);
  assert.equal(hashes.get("two-order-independent-1"), hashes.get("two-order-independent-2"));
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace('"hash": 5381', '"hash": 5382'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /library\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
