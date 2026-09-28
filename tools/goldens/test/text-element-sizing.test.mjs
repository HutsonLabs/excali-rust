// excali-text's text element sizing goldens (ex-305) are upstream's output:
// tools/goldens/text-element-sizing.mjs regenerates them from the pinned
// checkout, byte-stable across runs, --check fails when the committed file
// differs, the file holds no invisible code point, and it pins what
// upstream's textWysiwyg.test.tsx asserts about editing and unwrapping.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "text-element-sizing.mjs");
const FILE = "text-element-sizing.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "text-element-sizing-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/text-element-sizing.mjs");
});

const golden = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

test("the file holds no invisible code point", () => {
  const text = readFileSync(COMMITTED, "utf8");
  for (const ch of text) {
    const cp = ch.codePointAt(0);
    assert.ok(cp === 0x0a || (cp >= 0x20 && cp !== 0x7f && !/[\p{Cf}\p{Zs}]/u.test(ch)) || ch === " ", `U+${cp.toString(16)}`);
  }
});

test("a fixed-width text keeps its width while editing (textWysiwyg.test.tsx:684-722)", () => {
  const fixed = golden().refresh.filter((c) => c.mode === "fixed");
  assert.ok(fixed.length > 0);
  for (const c of fixed) assert.equal(c.result.width, c.text.width);
});

test("a growing text stops at the view's width and wraps (textWysiwyg.test.tsx:1041-1063, 1139-1154)", () => {
  const g = golden();
  const session = g.sessions.find((s) => s.maxWidth === 150 && s.text.textAlign === undefined && s.text.angle === undefined);
  assert.ok(session);
  const last = session.steps.at(-1);
  assert.equal(last.autoResize, false);
  assert.equal(last.width, 150);
  assert.ok(last.text.includes("\n"));
  // right-aligned: the right edge stays where it stopped growing
  for (const s of g.sessions.filter((s) => s.text.textAlign === "right" && s.text.angle === undefined)) {
    const edges = s.steps.map((st) => st.x + st.width);
    for (const e of edges) assert.ok(Math.abs(e - edges[0]) < 1e-9, `${e} vs ${edges[0]}`);
  }
});

test("unwrapping keeps the edge the alignment pins (textWysiwyg.test.tsx:3139-3188)", () => {
  for (const c of golden().autoResize) {
    const align = c.text.textAlign ?? "left";
    const valign = c.text.verticalAlign ?? "top";
    const ax = { left: 0, center: 0.5, right: 1 }[align];
    const ay = { top: 0, middle: 0.5, bottom: 1 }[valign];
    assert.equal(c.result.autoResize, true);
    assert.ok(Math.abs(c.text.x + c.text.width * ax - (c.result.x + c.result.width * ax)) < 1e-4);
    assert.ok(Math.abs(c.text.y + c.text.height * ay - (c.result.y + c.result.height * ay)) < 1e-4);
  }
});

test("--check fails on a stale fixture and names it", () => {
  const out = join(scratch, "stale");
  assert.equal(run(["--out", out]).status, 0);
  writeFileSync(join(out, FILE), "{}\n");
  const r = run(["--check", "--out", out]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /text-element-sizing\.json/);
});
