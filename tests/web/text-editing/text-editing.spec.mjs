// The text editor in Chromium (ex-512).
//
// Every session of upstream's text editing fixture
// (crates/excali-editor/tests/fixtures/text-editing.json, recorded from
// textWysiwyg.tsx and App.startTextEditing at the pinned commit by
// tools/goldens/text-editing.mjs) is loaded into the wasm harness
// tools/text-editing, which starts editing as the case does and mounts
// excali_ui::text_editor's textarea in the page. The steps are then taken
// the way a user takes them: typed and pressed with the keyboard into the
// focused textarea, selections set on it, pastes dispatched to it, blurs.
// After the start and after every step the elements, the app state, the
// editor (value, selection, the style it assigns) and what it asked of the
// app must be upstream's, and the textarea in the page must carry that
// style as Chromium parses upstream's own assignments.
//
// Also: the textarea is dir=auto, wrap=off, data-type=wysiwyg in the
// editor's box, which covers the canvas, clips and passes the pointer
// through except over the textarea.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const FIXTURE = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", "text-editing.json"), "utf8"),
);

/** Sessions whose caret goes where the page's text layout puts it. */
const CARET_SESSIONS = FIXTURE.sessions.filter((s) => s.start.initialCaretSceneCoords);
const SESSIONS = FIXTURE.sessions.filter((s) => !s.start.initialCaretSceneCoords);

const open = async (page, session) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  expect(await page.evaluate(() => window.harnessError || null), "the harness module loads").toBeNull();
  const started = await page.evaluate((json) => {
    const box = document.querySelector(".excalidraw-textEditorContainer");
    window.editing = new window.harness.TextEditing(box, json);
    return window.editing.start();
  }, JSON.stringify(session));
  expect(started, "an editor opens").toBe(true);
  // focused after the pointer down that opened it (a timeout)
  await page.waitForFunction(() => document.activeElement?.tagName === "TEXTAREA");
  return errors;
};

/** Two frames: the editor's timeouts and animation frames have run. */
const settle = (page) =>
  page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 0)))));

const read = (page) =>
  page.evaluate((keys) => {
    const e = window.editing;
    return {
      elements: JSON.parse(e.elements()),
      state: JSON.parse(e.state(JSON.stringify(keys))),
      editor: JSON.parse(e.editor()),
      calls: JSON.parse(e.takeCalls()),
      errors: JSON.parse(e.errors()),
      cache: JSON.parse(e.containerCache()),
    };
  }, FIXTURE.stateKeys);

/**
 * The textarea's inline style against a scratch element given upstream's
 * assignments (`Object.assign(style, {...})`, in order): every property
 * Chromium parses them into, with its value.
 */
const styleAgainstUpstream = (page, assigned) =>
  page.evaluate((assigned) => {
    const textarea = document.querySelector("textarea.excalidraw-wysiwyg");
    if (!textarea) return { missing: true };
    const scratch = document.createElement("textarea");
    Object.assign(scratch.style, assigned);
    const props = (style) => Array.from({ length: style.length }, (_, i) => style[i]).sort();
    const want = Object.fromEntries(props(scratch.style).map((p) => [p, scratch.style.getPropertyValue(p)]));
    const got = Object.fromEntries(props(textarea.style).map((p) => [p, textarea.style.getPropertyValue(p)]));
    return { want, got };
  }, assigned);

const PLAYWRIGHT_KEY = (spec) => spec.replace("CtrlOrCmd", "ControlOrMeta");

/** A step, taken as a user (or the page around the editor) takes it. */
const step = async (page, s) => {
  if (s.type !== undefined) {
    await page.keyboard.type(s.type);
  } else if (s.press) {
    await page.keyboard.press(PLAYWRIGHT_KEY(s.press));
  } else if (s.insertText !== undefined) {
    await page.keyboard.insertText(s.insertText);
  } else if (s.select) {
    await page.evaluate(([a, b]) => document.querySelector("textarea").setSelectionRange(a, b), s.select);
  } else if (s.blur) {
    await page.evaluate(() => document.querySelector("textarea").blur());
  } else if (s.paste) {
    await page.evaluate(({ types, text }) => {
      const textarea = document.querySelector("textarea");
      const data = new DataTransfer();
      for (const type of types) data.setData(type, text);
      const event = new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true });
      textarea.dispatchEvent(event);
      // a synthetic paste has no default action: the browser's paste
      if (!event.defaultPrevented && text) document.execCommand("insertText", false, text);
    }, s.paste);
  } else if (s.boxScroll) {
    await page.evaluate(([l, t]) => window.editing.boxScrolled(l, t), s.boxScroll);
  } else if (s.theme) {
    await page.evaluate((t) => window.editing.setTheme(t), s.theme);
  } else if (s.resize) {
    await page.evaluate(([w, h]) => window.editing.resize(w, h), s.resize);
  } else if (s.mutate) {
    await page.evaluate(([id, updates]) => window.editing.mutate(id, JSON.stringify(updates)), s.mutate);
  } else {
    throw new Error(`unknown step ${JSON.stringify(s)}`);
  }
  await settle(page);
};

const sameJson = (a, b) => {
  if (typeof a === "number" && typeof b === "number") return a === b;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => sameJson(x, b[i]));
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    return ka.length === kb.length && ka.every((k) => k in b && sameJson(a[k], b[k]));
  }
  return a === b;
};

const expectSame = (got, want, what) => {
  if (!sameJson(got, want)) expect(got, what).toEqual(want);
};

for (const session of SESSIONS) {
  test(`${session.name}: typing in Chromium gives upstream's elements, state and textarea`, async ({ page }) => {
    const errors = await open(page, session);

    let scene = session.initial.elements.map((e) => ({ ...e }));
    let state = { ...session.initial.state };
    let style = {};
    for (const [i, record] of session.records.entries()) {
      if (i > 0) await step(page, record.step);
      const at = `${session.name}, record ${i} (${JSON.stringify(record.step)})`;
      for (const changed of record.changed) {
        const j = scene.findIndex((e) => e.id === changed.id);
        if (j === -1) scene.push(changed);
        else scene[j] = changed;
      }
      if (record.order) scene = record.order.map((id) => scene.find((e) => e.id === id));
      state = { ...state, ...record.state };
      style = { ...style, ...record.style };

      const got = await read(page);
      expect(got.errors, `${at}: errors`).toEqual([]);
      expect(got.editor.open, `${at}: open`).toBe(record.open);
      expect(got.editor.value, `${at}: value`).toBe(record.value);
      expect(got.editor.selection, `${at}: selection`).toEqual(record.selection);
      expectSame(got.editor.style, style, `${at}: style`);
      expect(got.calls, `${at}: calls`).toEqual(record.calls);
      expect(got.elements.map((e) => e.id), `${at}: scene order`).toEqual(scene.map((e) => e.id));
      for (const [j, element] of scene.entries()) expectSame(got.elements[j], element, `${at}: element ${element.id}`);
      expectSame(got.state, state, `${at}: app state`);
      expectSame(got.cache, record.containerCache, `${at}: container cache`);

      // the textarea in the page, while open
      const dom = await styleAgainstUpstream(page, style);
      if (record.open) {
        expect(dom.missing, `${at}: the textarea is mounted`).toBeUndefined();
        expect(dom.got, `${at}: the textarea's style`).toEqual(dom.want);
        const value = await page.evaluate(() => {
          const t = document.querySelector("textarea");
          return { value: t.value, selection: [t.selectionStart, t.selectionEnd] };
        });
        expect(value, `${at}: the textarea`).toEqual({ value: record.value, selection: record.selection });
      } else {
        expect(dom.missing, `${at}: the textarea is removed`).toBe(true);
      }
    }
    expect(errors, "no page errors").toEqual([]);
  });
}

for (const session of CARET_SESSIONS) {
  test(`${session.name}: the caret goes on the line under the point`, async ({ page }) => {
    await open(page, session);
    await settle(page);
    const [first] = session.records;
    // upstream in jsdom measured every caret position at 0: the line's start
    const lineStart = first.selection[0];
    const got = await read(page);
    const value = got.editor.value;
    const lineEnd = value.indexOf("\n", lineStart) === -1 ? value.length : value.indexOf("\n", lineStart);
    const [start, end] = got.editor.selection;
    expect(start, "a caret, not a selection").toBe(end);
    expect(start >= lineStart && start <= lineEnd, `caret ${start} within [${lineStart}, ${lineEnd}]`).toBe(true);
    const dom = await page.evaluate(() => {
      const t = document.querySelector("textarea");
      return [t.selectionStart, t.selectionEnd];
    });
    expect(dom).toEqual([start, end]);
  });
}

test("the textarea: dir=auto, wrap=off, in the editor's box, which passes the pointer through", async ({ page }) => {
  await open(page, SESSIONS[0]);
  const d = await page.evaluate(() => {
    const t = document.querySelector("textarea");
    const box = t.parentElement;
    const cs = getComputedStyle(t);
    const bs = getComputedStyle(box);
    const editor = document.getElementById("editor").getBoundingClientRect();
    const b = box.getBoundingClientRect();
    return {
      dir: t.getAttribute("dir"),
      wrap: t.getAttribute("wrap"),
      type: t.dataset.type,
      tabIndex: t.tabIndex,
      className: t.className,
      boxClass: box.className,
      position: cs.position,
      whiteSpace: cs.whiteSpace,
      zIndex: cs.zIndex,
      boxSizing: cs.boxSizing,
      pointer: cs.pointerEvents,
      boxPointer: bs.pointerEvents,
      boxOverflow: bs.overflow,
      boxPosition: bs.position,
      boxCovers: [b.left - editor.left, b.top - editor.top, b.width, b.height],
    };
  });
  expect(d).toEqual({
    dir: "auto",
    wrap: "off",
    type: "wysiwyg",
    tabIndex: 0,
    className: "excalidraw-wysiwyg",
    boxClass: "excalidraw-textEditorContainer",
    position: "absolute",
    whiteSpace: "pre",
    zIndex: "3",
    boxSizing: "content-box",
    pointer: "auto",
    boxPointer: "none",
    boxOverflow: "hidden",
    boxPosition: "absolute",
    boxCovers: [0, 0, 1000, 800],
  });
});

test("a pointer down on the canvas submits the edit", async ({ page }) => {
  await open(page, SESSIONS[0]);
  await settle(page);
  await page.mouse.click(900, 700);
  await settle(page);
  const got = await read(page);
  expect(got.editor.open).toBe(false);
  expect(await page.locator("textarea").count()).toBe(0);
});

// app.scene.onUpdate (textWysiwyg.tsx:1053-1061): after a scene change the
// textarea is restyled and focused again (preventScroll), unless the focus
// is inside a properties popover (.properties-content).
test("a scene update takes the focus back to the textarea, but not from a properties popover", async ({ page }) => {
  const session = FIXTURE.sessions.find((s) => s.name === "container-moved-while-editing");
  await open(page, session);
  await settle(page);
  await page.evaluate(() => {
    const panel = document.createElement("div");
    panel.innerHTML =
      '<div class="App-menu__left"><button id="panel-button">stroke</button></div>' +
      '<div class="properties-content"><input id="popover-input" type="text"></div>';
    document.body.append(panel);
  });
  const active = () => page.evaluate(() => document.activeElement?.id || document.activeElement?.tagName);
  const textareaLeft = () => page.evaluate(() => document.querySelector("textarea")?.style.left);

  // the styles panel: the blur submit is suspended and the focus leaves
  await page.click("#panel-button");
  await page.evaluate(() => document.getElementById("panel-button").focus());
  await settle(page);
  expect(await active()).toBe("panel-button");
  expect(await page.locator("textarea").count(), "the edit is not submitted").toBe(1);
  const before = await textareaLeft();
  await page.evaluate(() => window.editing.mutate("r", JSON.stringify({ x: 300, y: 200 })));
  await settle(page);
  expect(await textareaLeft(), "restyled").not.toBe(before);
  expect(await active(), "focused again").toBe("TEXTAREA");
  const box = await page.evaluate(() => {
    const b = document.querySelector(".excalidraw-textEditorContainer");
    return [b.scrollLeft, b.scrollTop];
  });
  expect(box, "without scrolling the editor's box").toEqual([0, 0]);
  // the selection the textarea had (the whole label) is typed over
  const want = await page.evaluate(() => {
    const t = document.querySelector("textarea");
    return `${t.value.slice(0, t.selectionStart)}z${t.value.slice(t.selectionEnd)}`;
  });
  await page.keyboard.type("z");
  await settle(page);
  expect(await page.evaluate(() => document.querySelector("textarea").value), "typing reaches the textarea").toBe(want);
  expect((await read(page)).editor.value, "and the editor").toBe(want);

  // a properties popover keeps the focus
  await page.click("#popover-input");
  await settle(page);
  expect(await active()).toBe("popover-input");
  const moved = await textareaLeft();
  await page.evaluate(() => window.editing.mutate("r", JSON.stringify({ x: 400, y: 250 })));
  await settle(page);
  expect(await textareaLeft(), "restyled").not.toBe(moved);
  expect(await active(), "not focused again").toBe("popover-input");
  expect(await page.locator("textarea").count(), "the edit is not submitted").toBe(1);
});
