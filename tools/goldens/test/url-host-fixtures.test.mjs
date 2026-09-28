// The URL host fixtures of excali-core's `whatwg_url` tests are Node's own
// `new URL` output: tools/goldens/url-host-fixtures.mjs regenerates them,
// byte-stable across runs, refuses a Node release other than the pinned one,
// and --check fails when the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "url-host-fixtures.mjs");
const NAME = "url-hosts.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);
const PINNED = readFileSync(join(TOOL_DIR, ".node-version"), "utf8").trim();

const scratch = mkdtempSync(join(tmpdir(), "url-host-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

test("runs under the pinned Node release", () => {
  assert.equal(process.version, `v${PINNED}`, "tools/goldens/.node-version");
});

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), `${NAME} is stale: run node tools/goldens/url-host-fixtures.mjs`);
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("records ada's IDNA where it differs from UTS 46 with Unicode 17 data", () => {
  const fixture = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byInput = new Map(fixture.cases.map((c) => [c.input, c]));
  // U+1AD3 (Unicode 17, a combining mark) starting a label: ada's validity
  // table does not have it.
  assert.equal(byInput.get("https://᫓/").hostname, "xn--trf");
  // U+10D50 (Garay, right-to-left) after `a`.
  assert.equal(byInput.get("https://a\u{10D50}/").hostname, "xn--a-ho6i");
  // A Hebrew letter with U+1AD3, which ada does not take for an NSM.
  assert.equal(byInput.get("https://א᫓/").error, true);
  assert.equal(fixture.sweep.blocks.length, 0x110000 / fixture.sweep.blockSize);
  const inputs = fixture.sweep.blocks.reduce((n, b) => n + b.inputs, 0);
  assert.equal(inputs, 2 * (0x110000 - 0x80 - 0x800));
  assert.equal(fixture.random.chunks.length, fixture.random.count / fixture.random.chunkSize);
});
