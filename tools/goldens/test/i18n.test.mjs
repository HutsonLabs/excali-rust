// excali-ui's locale fixture and locale files (ex-711) are upstream's:
// tools/goldens/i18n.mjs regenerates them from the pinned checkout,
// byte-stable across runs, and --check fails when a committed file differs.
// The checks below restate what the issue asks for
// (research/ui-design-system.md section 7) from the cited upstream files
// and hold the recorded output to them.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "i18n.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FIXTURE = join("tests", "fixtures", "i18n.json");
const LOCALES = join("assets", "locales");

const scratch = mkdtempSync(join(tmpdir(), "i18n-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });
const committed = () => JSON.parse(readFileSync(join(CRATE, FIXTURE), "utf8"));

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const files = [FIXTURE, ...readdirSync(join(outs[0], LOCALES)).map((f) => join(LOCALES, f))];
  for (const f of files) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/i18n.mjs`);
  }
  assert.deepEqual(readdirSync(join(CRATE, LOCALES)).sort(), readdirSync(join(outs[0], LOCALES)).sort());
});

test("58 locale files and 633 English keys (research section 7)", () => {
  const f = committed();
  assert.equal(f.files.length, 58);
  assert.ok(f.files.includes("en.json") && f.files.includes("percentages.json"));
  assert.equal(f.keys, 633);
  const dir = join(upstreamDir(), "packages", "excalidraw", "locales");
  assert.deepEqual(f.files, readdirSync(dir).filter((n) => n.endsWith(".json")).sort());
  // the copies escape the gated invisible code points and parse to upstream's values
  for (const file of f.files) {
    const copy = readFileSync(join(CRATE, LOCALES, file), "utf8");
    assert.deepEqual(JSON.parse(copy), JSON.parse(readFileSync(join(dir, file), "utf8")), file);
    assert.ok(!/[\u200b-\u200f\u2028-\u202e\ufe00-\ufe0f]/u.test(copy), file);
  }
});

test("only languages at 85% or more are listed, RTL ones set dir (i18n.ts:7-9, 24-36, 71-72, 94)", () => {
  const f = committed();
  const percentages = JSON.parse(readFileSync(join(CRATE, LOCALES, "percentages.json"), "utf8"));
  assert.equal(f.threshold, 85);
  assert.deepEqual(f.languages[0], f.defaultLang);
  for (const l of f.languages.slice(1)) assert.ok(percentages[l.code] >= 85, l.code);
  for (const l of f.candidates.filter((c) => percentages[c.code] < 85)) {
    assert.ok(!f.languages.some((x) => x.code === l.code), l.code);
  }
  assert.deepEqual(
    f.candidates.filter((l) => l.rtl).map((l) => l.code).sort(),
    ["ar-SA", "fa-IR", "he-IL"],
  );
  for (const c of f.cases) {
    assert.equal(c.dir, c.lang.rtl ? "rtl" : "ltr", c.lang.code);
    assert.equal(c.htmlLang, c.lang.code);
  }
});

test("t falls back to English where a language is empty or lacks a key (i18n.ts:127-142)", () => {
  const en = JSON.parse(readFileSync(join(CRATE, LOCALES, "en.json"), "utf8"));
  const lookup = (d, p) => p.split(".").reduce((x, k) => (x == null ? undefined : x[k]), d);
  let fellBack = 0;
  for (const c of committed().cases) {
    for (const [path, text] of Object.entries(c.t)) {
      const english = lookup(en, path);
      if (typeof english !== "string") {
        assert.equal(text, "", `${c.lang.code} ${path}`);
        continue;
      }
      if (c.lang.code !== "en" && text === english) fellBack++;
    }
  }
  assert.ok(fellBack > 50, `${fellBack} English fallbacks`);
});
