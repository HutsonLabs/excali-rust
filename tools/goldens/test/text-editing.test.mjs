// excali-editor's text editing fixture (ex-512) is upstream's output:
// tools/goldens/text-editing.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate the overlay's rules independently
// (textWysiwyg.tsx:93-104 getTransform, :408-460 the style, :487-493 the
// textarea) and hold the recorded values to them, so a generator that lost
// cases or recorded the wrong thing would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "text-editing.mjs");
const FILE = "text-editing.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "text-editing-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

/** Each record of a session with the scene, style and state it leaves. */
const replay = (session) => {
  let scene = session.initial.elements.map((e) => ({ ...e }));
  let state = { ...session.initial.state };
  let style = {};
  return session.records.map((record) => {
    for (const changed of record.changed) {
      const i = scene.findIndex((e) => e.id === changed.id);
      if (i === -1) scene.push(changed);
      else scene[i] = changed;
    }
    if (record.order) scene = record.order.map((id) => scene.find((e) => e.id === id));
    state = { ...state, ...record.state };
    style = { ...style, ...record.style };
    return { record, scene: [...scene], state, style };
  });
};

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/text-editing.mjs");
});

test("the textarea is dir=auto, wrap=off, in the editor's box", () => {
  const sessions = committed().sessions;
  assert.ok(sessions.length >= 30);
  for (const s of sessions) {
    assert.deepEqual(
      s.attributes,
      {
        tagName: "textarea",
        dir: "auto",
        tabIndex: 0,
        dataType: "wysiwyg",
        wrap: "off",
        className: "excalidraw-wysiwyg",
        parentClassName: "excalidraw-textEditorContainer",
      },
      s.name,
    );
  }
});

test("the editor sits on its text: 5% height buffer, half a pixel in a container, the transform formula", () => {
  let checked = 0;
  for (const s of committed().sessions) {
    for (const { record, scene, state, style } of replay(s)) {
      if (!record.open || !Object.keys(record.style).length) continue;
      const text = scene.find((e) => e.id === state.editingTextElement);
      const container = text.containerId ? scene.find((e) => e.id === text.containerId) : null;
      // the style's size is the size before the step's final move, which
      // is the text's own size (the move changes only x and y)
      assert.equal(style.height, `${text.height * 1.05}px`, `${s.name}: height`);
      assert.equal(style.width, `${text.width + (container ? 0.5 : 0)}px`, `${s.name}: width`);
      assert.equal(style.transformOrigin, `${text.width / 2}px ${text.height / 2}px`, s.name);
      const z = state.zoom;
      const angle = container ? (container.type === "arrow" ? 0 : container.angle) : text.angle;
      const deg = (180 * angle) / Math.PI;
      assert.equal(
        style.transform,
        `translate(${(text.width * (z - 1)) / 2}px, ${(text.height * (z - 1)) / 2}px) scale(${z}) rotate(${deg}deg)`,
        s.name,
      );
      // at the text's corner in viewport coordinates
      const sidebarLeft = s.sidebar.left;
      assert.equal(style.left, `${(text.x + state.scrollX) * z - sidebarLeft}px`, `${s.name}: left`);
      assert.equal(style.top, `${(text.y + state.scrollY) * z}px`, `${s.name}: top`);
      assert.equal(style.font, `${text.fontSize}px ${style.font.slice(style.font.indexOf(" ") + 1)}`, s.name);
      assert.equal(style.lineHeight, String(text.lineHeight), s.name);
      assert.equal(style.opacity, String(text.opacity / 100), s.name);
      checked++;
    }
  }
  assert.ok(checked >= 60, `${checked} styled records`);
});

test("a submit closes the editor and captures the edit unless an empty new text is dropped", () => {
  for (const s of committed().sessions) {
    const records = replay(s);
    const last = records.at(-1);
    if (last.record.open) continue;
    const text = last.scene.find((e) => e.id === (s.start.textElement ?? s.ids[0] ?? s.initial.state.editingTextElement));
    const existing = !!s.start.textElement || s.ids.length === 0;
    const deleted = !last.record.value.trim();
    assert.equal(last.state.editingTextElement, null, s.name);
    assert.equal(last.record.calls.includes("scheduleCapture"), !deleted || existing, s.name);
    if (text) assert.equal(text.isDeleted, deleted, s.name);
  }
});
