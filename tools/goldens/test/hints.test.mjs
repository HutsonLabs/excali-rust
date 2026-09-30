// excali-ui's hints and welcome screen fixture, stylesheets and logo
// (ex-528) are upstream's output: tools/goldens/hints.mjs regenerates them
// from the pinned checkout, byte-stable across runs, and --check fails when
// a committed file differs. The checks below restate what the issue asks
// for (research/ui-design-system.md 3.8 and 3.11) from the cited upstream
// files and hold the recorded output to them, so a generator that lost
// cases or platforms would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "hints.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [
  join("tests", "fixtures", "hints.json"),
  join("src", "hints", "hints.css"),
  join("src", "welcome_screen", "welcome_screen.css"),
  join("src", "welcome_screen", "excalidraw_logo.html"),
];

const scratch = mkdtempSync(join(tmpdir(), "hints-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const text = (n) => (n == null ? "" : typeof n === "string" ? n : (n.children ?? []).map(text).join(""));

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/hints.mjs`);
  }
});

test("each labelled hint renders its en.json string (HintViewer.tsx:53-246)", () => {
  const en = JSON.parse(readFileSync(join(upstreamDir(), "packages", "excalidraw", "locales", "en.json"), "utf8"));
  const linux = committed().hintViewer.find((p) => p.platform === "linux").cases;
  assert.ok(linux.length >= 60);
  for (const c of linux) {
    if (!c.hints.length) {
      assert.equal(c.dom, null, c.name);
      continue;
    }
    // the template's literal text around the {{shortcut}} slots
    const pieces = c.hints.flatMap((k) => en.hints[k].replace(/\. ?$/, "").split(/\{\{[^}]+\}\}/));
    const shown = text(c.dom);
    for (const piece of pieces) assert.ok(shown.includes(piece), `${c.name}: ${JSON.stringify(piece)} in ${JSON.stringify(shown)}`);
    assert.equal(c.dom.attrs.class, "HintViewer");
  }
});

test("platform labels: Ctrl and Alt on Linux, Cmd and Option on a Mac", () => {
  const find = (p, name) => committed().hintViewer.find((x) => x.platform === p).cases.find((c) => c.name === name);
  assert.match(text(find("linux", "box-select").dom), /Hold Ctrl to deep select/);
  assert.match(text(find("darwin", "box-select").dom), /Hold Cmd to deep select/);
  assert.match(text(find("darwin", "eraser").dom), /Hold Option to revert/);
});

test("the welcome screen has the logo, heading, Open and Help, and three hints (WelcomeScreen.*.tsx)", () => {
  const desktop = committed().welcome.find((p) => p.platform === "linux").cases.find((c) => c.name === "desktop");
  assert.deepEqual(
    desktop.center.children.map((c) => c.attrs.class),
    ["welcome-screen-center__logo excalifont welcome-screen-decor", "welcome-screen-center__heading welcome-screen-decor excalifont", "welcome-screen-menu"],
  );
  assert.equal(text(desktop.center.children[1]), "Diagrams. Made. Simple.");
  assert.deepEqual(desktop.clicks.map((c) => c.executed[0]), ["loadScene", "toggleShortcuts"]);
  assert.equal(text(desktop.menuHint), "Export, preferences, and more...");
  assert.equal(text(desktop.toolbarHint), "Pick a tool & Start drawing!");
  assert.equal(text(desktop.helpHint), "Shortcuts & help");
  const phone = committed().welcome[0].cases.find((c) => c.name === "phone");
  assert.ok(!JSON.stringify(phone.center).includes("welcome-screen-menu-item__shortcut"));
});
