// excali-ui's hyperlink editor fixture and stylesheet (ex-543) are
// upstream's output: tools/goldens/hyperlink.mjs regenerates both from the
// pinned checkout, byte-stable across runs, and --check fails when a
// committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "hyperlink.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "hyperlink.json"), join("src", "hyperlink", "hyperlink.css")];

const scratch = mkdtempSync(join(tmpdir(), "hyperlink-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `${f} is stale: run node tools/goldens/hyperlink.mjs`);
  }
});

test("--check passes on the committed files", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("Enter and Escape submit the normalized link and show the info popup", () => {
  const fixture = JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
  const byName = new Map(fixture.cases.map((c) => [c.name, c]));
  const last = (name) => byName.get(name).steps.at(-1);
  assert.equal(last("create").link, "https://example.com/a?b=c");
  assert.equal(last("create").showHyperlinkPopup, "info");
  assert.equal(last("edit-existing").link, "https://docs.excalidraw.com");
  assert.equal(last("clear-by-empty-input").link, null);
  assert.equal(last("remove").link, null);
  assert.equal(last("remove").showHyperlinkPopup, false);
  // closing while editing submits what was typed, after Remove too
  assert.equal(last("remove-while-editing").link, "https://docs.excalidraw.com");
  assert.equal(last("info-without-link").link, "example.com");
  assert.equal(byName.get("sanitized").steps[2].link, "about:blank");
  // Ctrl+K in the input is kept from the editor's shortcut
  assert.equal(byName.get("edit-existing").steps[1].defaultPrevented, true);
  for (const c of fixture.cases) {
    const shown = c.steps[0].dom;
    if (!shown.length) continue;
    assert.equal(shown[0].attrs.class, "excalidraw-hyperlinkContainer", c.name);
    assert.equal(shown[0].style.width, "380px", c.name);
  }
});
