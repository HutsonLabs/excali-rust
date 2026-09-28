// excali-text's text-wrapping goldens (ex-303) are upstream's output:
// tools/goldens/text-wrapping.mjs regenerates them from the pinned checkout,
// byte-stable across runs, --check fails when the committed file differs,
// the file holds no invisible code point, and it pins upstream's own
// textWrapping.test.ts expectations.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";
import { escapeInvisible, METRICS } from "../text-wrapping.mjs";

const GENERATOR = join(TOOL_DIR, "text-wrapping.mjs");
const FILE = "text-wrapping.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "text-wrapping-test-"));
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
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/text-wrapping.mjs");
});

const golden = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

const wrapped = (g, metric, text, maxWidth) => {
  const entry = g.wraps.find((w) => w.metric === metric && w.text === text);
  assert.ok(entry, `no ${metric} entry for ${JSON.stringify(text)}`);
  const c = entry.cases.find((x) => x.maxWidth === maxWidth);
  assert.ok(c, `no width ${maxWidth} for ${JSON.stringify(text)}`);
  return c.lines.map((l) => l[0]).join("\n");
};

test("the fixture pins upstream's textWrapping.test.ts expectations", () => {
  const g = golden();
  // textWrapping.test.ts:14-19, 42-54, 84-94, 108-121, 211-220
  assert.equal(wrapped(g, "chars10", "Hello Excalidraw", 100), "Hello\nExcalidraw");
  assert.equal(wrapped(g, "chars10", "Hello Excalidraw", "NaN"), "Hello Excalidraw");
  assert.equal(wrapped(g, "chars10", "Hello     ", 50), "Hello");
  assert.equal(wrapped(g, "chars10", "Hello     ", 60), "Hello ");
  assert.equal(wrapped(g, "chars10", "Hello thereusing-now", 100), "Hello\nthereusing\n-now");
  assert.equal(wrapped(g, "chars10", "HelloたWorld", 50), "Hello\nた\nWorld");
  const offsets = g.wraps
    .find((w) => w.metric === "chars10" && w.text === "Hello World!")
    .cases.find((c) => c.maxWidth === 60).lines;
  assert.deepEqual(offsets, [["Hello", 0, 5], ["World!", 6, 12]]);
  // textWrapping.test.ts:517-521
  const number = g.tokens.find((t) => t.line === "99,100.99");
  assert.deepEqual(number.tokens, ["99,100.99"]);
});

test("the metrics are the ones the Rust test restates", () => {
  assert.equal(METRICS.chars10("a😀"), 30);
  // 'a' = 97: 3 + 679 % 11 = 3 + 8; 'b' = 98: 3 + 686 % 11 = 3 + 4;
  // 97 + 98 = 195 is a multiple of 5.
  assert.equal(METRICS.varied("ab"), 11 + 7 - 0.5);
});

test("the file holds only visible characters; escapes parse back", () => {
  const text = readFileSync(COMMITTED, "utf8");
  assert.doesNotMatch(text, /[\u200b-\u200f\u2028-\u202e\ufe00-\ufe0f\ufeff\u{e0000}-\u{e007f}]/u);
  const s = "a\u200d\ufe0f\u2028\u{e0067}\u3000b";
  assert.equal(JSON.parse(escapeInvisible(JSON.stringify(s))), s);
  assert.equal(escapeInvisible(JSON.stringify(s)), '"a\\u200d\\ufe0f\\u2028\\udb40\\udc67\\u3000b"');
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('["Hello",0,5]', '["Hello",0,6]'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /text-wrapping\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
