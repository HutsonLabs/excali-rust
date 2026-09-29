// excali-editor's keyboard handling in Chromium (ex-515).
//
// Every row of the shortcuts page (site/content/design-system/shortcuts.md,
// the Tools, View and Editor tables) has a test here, and the first test
// fails when a row has none or a test has no row. Each test presses the
// row's keys with Playwright's keyboard (and mouse, for the pointer rows)
// on the page tools/keyboard serves: Chromium's own keydown, keyup,
// clipboard and pointer events go through excali_ui::keyboard into
// excali_editor::keyboard (App.onKeyDown, App.onKeyUp, the command
// palette's listener, onCopy / onCut / the paste gate, AppPan.start), and
// the test checks what upstream does at the pinned commit:
//
// - the keys App.onKeyDown owns change the editor state (active tool,
//   arrow type, tool lock, element positions, scroll, open dialog and
//   popup, binding, pan) or hand upstream's work to the host (text
//   editing, eyedropper, flowchart, element type conversion);
// - the keys an action owns reach the action manager, which names the
//   action whose keyTest took the key (App.tsx:5764); the action's perform
//   is its feature's.
//
// Ctrl is ControlOrMeta: Cmd where upstream's isDarwin holds.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const MOD = "ControlOrMeta";

/** The first cell of every row of the page's Tools, View and Editor tables. */
const shortcutRows = () => {
  const md = readFileSync(join(REPO_ROOT, "site/content/design-system/shortcuts.md"), "utf8");
  const rows = [];
  let section = null;
  for (const line of md.split("\n")) {
    const heading = /^## (.+)$/.exec(line);
    if (heading) {
      section = ["Tools", "View", "Editor"].includes(heading[1]) ? heading[1] : null;
      continue;
    }
    if (!section || !line.startsWith("|")) continue;
    const first = line.split("|")[1].trim();
    if (first === "Key" || /^-+$/.test(first)) continue;
    rows.push(`${section}: ${first}`);
  }
  return rows;
};

// -- elements (API.createElement's defaults) ---------------------------------

let seed = 1;
const base = (type, id, x = 0, y = 0, w = 100, h = 100) => ({
  id,
  type,
  x,
  y,
  width: w,
  height: h,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roundness: null,
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  index: null,
  seed: seed++,
  version: 1,
  versionNonce: 0,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
});
const rect = (id, x = 0, y = 0) => base("rectangle", id, x, y);
const text = (id, x = 0, y = 0) => ({
  ...base("text", id, x, y, 50, 25),
  text: "hello",
  originalText: "hello",
  fontSize: 20,
  fontFamily: 5,
  textAlign: "left",
  verticalAlign: "top",
  containerId: null,
  autoResize: true,
  lineHeight: 1.25,
});
const linear = (type, id) => ({
  ...base(type, id, 0, 0, 100, 0),
  points: [
    [0, 0],
    [100, 0],
  ],
  startBinding: null,
  endBinding: null,
  startArrowhead: null,
  endArrowhead: type === "arrow" ? "arrow" : null,
  ...(type === "arrow" ? { elbowed: false } : { polygon: false }),
});
const frame = (id) => ({ ...base("frame", id, 300, 300, 200, 200), name: null });

const TWO_RECTS = [rect("r1"), rect("r2", 200, 0)];

// -- page helpers -------------------------------------------------------------

const open = async (page, { elements = [], appState = {}, props = {}, focus = "#container" } = {}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  expect(await page.evaluate(() => window.harnessError || null), "the harness module loads").toBeNull();
  await load(page, elements, appState, props);
  await page.focus(focus);
  page.errors = errors;
  return page;
};

/** Loads a scene: elements, app state keys over the defaults, props. */
const load = (page, elements = [], appState = {}, props = {}) =>
  page.evaluate(
    ([e, a, p]) => window.keyboard.load(JSON.stringify(e), JSON.stringify(a), JSON.stringify(p)),
    [elements, appState, props],
  );

/** The key CTRL_OR_CMD reads: Meta where upstream's isDarwin holds. */
const ctrlKey = async (page) => ((await page.evaluate(() => window.isDarwin)) ? "Meta" : "Control");

/** Presses `combo` (Playwright syntax) and returns the events it logged. */
const press = async (page, combo) => {
  await page.evaluate(() => (window.log = []));
  await page.keyboard.press(combo);
  return page.evaluate(() => window.log);
};

const state = async (page) => JSON.parse(await page.evaluate(() => window.keyboard.state()));

const keydown = (log) => log.find((e) => e.type === "keydown" && !/^(Control|Meta|Shift|Alt)$/.test(e.key));
const effects = (log) => keydown(log)?.outcome.effects ?? [];
const action = (log) => effects(log).find((e) => e.type === "action")?.action ?? null;
const effect = (log, type) => effects(log).find((e) => e.type === type);

/** Each [combo, action name] reaches the action manager. */
const expectActions = async (page, cases) => {
  for (const [combo, name] of cases) {
    const log = await press(page, combo);
    expect(action(log), combo).toBe(name);
    expect(keydown(log).outcome.preventDefault, combo).toBe(true);
  }
};

const tool = async (page) => (await state(page)).activeTool.type;

const expectTools = async (page, keys, name) => {
  for (const key of keys) {
    await load(page);
    const log = await press(page, key);
    expect(await tool(page), key).toBe(name);
    expect(effect(log, "tool")?.tool, key).toBe(name);
    expect(keydown(log).outcome.stopPropagation, key).toBe(true);
  }
};

const position = async (page, id) => {
  const e = (await state(page)).elements.find((e) => e.id === id);
  return [e.x, e.y];
};

/** A press on the canvas at (x, y) with `button`, and what it read. */
const pointerDown = async (page, { button = "left", x = 400, y = 250 } = {}) => {
  await page.evaluate(() => (window.log = []));
  await page.mouse.move(x, y);
  await page.mouse.down({ button });
  await page.mouse.up({ button });
  return page.evaluate(() => window.log.find((e) => e.type === "pointerdown").outcome);
};

// -- the rows -------------------------------------------------------------------

const ROWS = {
  // Tools
  "Tools: H": async (page) => {
    await expectTools(page, ["h"], "hand");
    await press(page, "h");
    expect(await tool(page), "H again leaves the hand tool").toBe("selection");
  },
  "Tools: V or 1": async (page) => {
    await expectTools(page, ["v", "1"], "selection");
  },
  "Tools: R or 2": (page) => expectTools(page, ["r", "2"], "rectangle"),
  "Tools: D or 3": (page) => expectTools(page, ["d", "3"], "diamond"),
  "Tools: O or 4": (page) => expectTools(page, ["o", "4"], "ellipse"),
  "Tools: A or 5": async (page) => {
    await expectTools(page, ["a", "5"], "arrow");
    expect((await state(page)).appState.currentItemArrowType).toBe("round");
    const cycle = [];
    for (let i = 0; i < 3; i++) {
      await press(page, "a");
      cycle.push((await state(page)).appState.currentItemArrowType);
    }
    expect(cycle, "A again: sharp → round → elbow").toEqual(["elbow", "sharp", "round"]);
  },
  "Tools: L or 6": (page) => expectTools(page, ["l", "6"], "line"),
  "Tools: P, X or 7": (page) => expectTools(page, ["p", "x", "7"], "freedraw"),
  "Tools: T or 8": (page) => expectTools(page, ["t", "8"], "text"),
  "Tools: N": (page) => expectTools(page, ["n"], "stickynote"),
  "Tools: 9": (page) => expectTools(page, ["9"], "image"),
  "Tools: E or 0": async (page) => {
    await expectTools(page, ["e", "0"], "eraser");
    await load(page);
    await press(page, "r");
    await press(page, "e");
    await press(page, "e");
    expect(await tool(page), "E again goes back to the last tool").toBe("rectangle");
  },
  "Tools: F": (page) => expectTools(page, ["f"], "frame"),
  "Tools: Shift+X": (page) => expectTools(page, ["Shift+X"], "autoshape"),
  "Tools: K": (page) => expectTools(page, ["k"], "laser"),
  "Tools: B": async (page) => {
    await expectTools(page, ["b"], "bucketfill");
    const log = await press(page, "b");
    expect(effect(log, "tool").cycleBucketFillColor, "B again cycles the colour").toBe(true);
    expect(await tool(page)).toBe("bucketfill");
  },
  "Tools: Q": async (page) => {
    await press(page, "r");
    const log = await press(page, "q");
    expect(effect(log, "toolLock")).toEqual({ type: "toolLock", changed: true });
    expect((await state(page)).activeTool.locked).toBe(true);
    await press(page, "q");
    expect((await state(page)).activeTool.locked).toBe(false);
  },
  "Tools: I, Shift+S, Shift+G": async (page) => {
    for (const [combo, kind] of [
      ["i", "background"],
      ["Shift+S", "stroke"],
      ["Shift+G", "background"],
    ]) {
      const log = await press(page, combo);
      expect(effect(log, "openEyeDropper")?.kind, combo).toBe(kind);
    }
  },
  "Tools: S / G": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    let log = await press(page, "s");
    expect((await state(page)).appState.openPopup).toBe("elementStroke");
    expect(keydown(log).outcome.stopPropagation).toBe(true);
    log = await press(page, "g");
    expect((await state(page)).appState.openPopup).toBe("elementBackground");
  },
  "Tools: Shift+F": async (page) => {
    await load(page, [text("t")], { selectedElementIds: { t: true } });
    const log = await press(page, "Shift+F");
    expect(keydown(log).outcome.preventDefault).toBe(true);
    expect((await state(page)).appState.openPopup).toBe("fontFamily");
  },
  "Tools: Ctrl+Enter": async (page) => {
    await load(page, [linear("arrow", "a")], { selectedElementIds: { a: true } });
    let log = await press(page, "Enter");
    expect(effect(log, "executeAction"), "Enter alone does not edit an arrow's points").toBeUndefined();
    log = await press(page, `${MOD}+Enter`);
    expect(effect(log, "executeAction")?.action).toBe("toggleLinearEditor");
    expect(effect(log, "scheduleCapture")).toBeTruthy();
  },
  "Tools: Enter": async (page) => {
    await load(page, [rect("r1", 10, 20), text("t", 0, 0), frame("f")], { selectedElementIds: { r1: true } });
    let log = await press(page, "Enter");
    expect(effect(log, "startTextEditing")).toEqual({
      type: "startTextEditing",
      sceneX: 60,
      sceneY: 70,
      container: "r1",
    });
    expect(keydown(log).outcome.preventDefault).toBe(true);
    await load(page, [text("t", 0, 0)], { selectedElementIds: { t: true } });
    log = await press(page, "Enter");
    expect(effect(log, "startTextEditing")).toEqual({
      type: "startTextEditing",
      sceneX: 25,
      sceneY: 12.5,
      container: null,
    });
    await load(page, [frame("f")], { selectedElementIds: { f: true } });
    await press(page, "Enter");
    expect((await state(page)).appState.editingFrame, "Enter on a frame enters it").toBe("f");
  },
  "Tools: Esc or Ctrl+Enter": async (page) => {
    // In the text editor these keys are the editor's (textWysiwyg.tsx:714-732,
    // ex-512): App.onKeyDown bails on a writable target (App.tsx:5710-5721)
    // and no action's keyTest takes them there (actionDeselect.ts:130-136),
    // so nothing prevents them before the editor's own listener.
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await page.focus("#wysiwyg");
    for (const combo of [`${MOD}+Enter`, "Escape"]) {
      const log = await press(page, combo);
      expect(effects(log), combo).toEqual([]);
      expect(keydown(log).outcome.preventDefault, combo).toBe(false);
    }
    await press(page, "r");
    expect(await tool(page), "letters typed in the editor are text").toBe("selection");
    // on the canvas, Escape deselects
    await page.focus("#container");
    expect(action(await press(page, "Escape"))).toBe("deselect");
  },
  "Tools: Tab / Shift+Tab": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    let log = await press(page, "Tab");
    expect(keydown(log).outcome.preventDefault, "Tab stays in the editor").toBe(true);
    expect((await state(page)).keyboard.convertPopupOpen).toBe(true);
    await page.focus("#convert");
    log = await press(page, "Shift+Tab");
    expect(effect(log, "convertElementType")).toEqual({
      type: "convertElementType",
      conversion: "generic",
      direction: "left",
    });
    log = await press(page, "Tab");
    expect(effect(log, "convertElementType").direction).toBe("right");
  },
  "Tools: Ctrl (held)": async (page) => {
    await load(page, [], { bindingPreference: "enabled", isBindingEnabled: true });
    const ctrl = await ctrlKey(page);
    await page.keyboard.down(ctrl);
    expect((await state(page)).appState.isBindingEnabled, "binding is off while Ctrl is held").toBe(false);
    await page.keyboard.up(ctrl);
    expect((await state(page)).appState.isBindingEnabled, "and back on release").toBe(true);
  },
  "Tools: Ctrl+K": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [[`${MOD}+k`, "hyperlink"]]);
  },

  // View
  "View: Ctrl + `+` / `−` / `0`": (page) =>
    expectActions(page, [
      [`${MOD}+Equal`, "zoomIn"],
      [`${MOD}+Shift+Equal`, "zoomIn"],
      [`${MOD}+NumpadAdd`, "zoomIn"],
      [`${MOD}+Minus`, "zoomOut"],
      [`${MOD}+Shift+Minus`, "zoomOut"],
      [`${MOD}+NumpadSubtract`, "zoomOut"],
      [`${MOD}+Digit0`, "resetZoom"],
      [`${MOD}+Numpad0`, "resetZoom"],
    ]),
  "View: Shift+1": (page) => expectActions(page, [["Shift+Digit1", "zoomToFit"]]),
  "View: Shift+2": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [["Shift+Digit2", "zoomToFitSelectionInViewport"]]);
  },
  "View: Shift+3": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [["Shift+Digit3", "zoomToFitSelection"]]);
  },
  "View: PgUp / PgDn (Shift: horizontal)": async (page) => {
    await load(page, [], { width: 800, height: 500, zoom: { value: 1.25 } });
    let log = await press(page, "PageUp");
    expect(keydown(log).outcome.preventDefault).toBe(true);
    expect((await state(page)).appState.scrollY).toBe(400);
    await press(page, "PageDown");
    await press(page, "PageDown");
    expect((await state(page)).appState.scrollY).toBe(-400);
    await press(page, "Shift+PageUp");
    expect((await state(page)).appState.scrollX).toBe(640);
    await press(page, "Shift+PageDown");
    expect((await state(page)).appState.scrollX).toBe(0);
  },
  "View: Space + drag, wheel + drag": async (page) => {
    await page.keyboard.down("Space");
    expect((await state(page)).spaceHeld).toBe(true);
    expect((await pointerDown(page)).pan, "Space + drag pans").toBe(true);
    await page.keyboard.up("Space");
    expect((await state(page)).spaceHeld).toBe(false);
    expect((await pointerDown(page)).pan, "a plain drag does not").toBe(false);
    expect((await pointerDown(page, { button: "middle" })).pan, "wheel + drag pans").toBe(true);
  },
  "View: Alt+Z": (page) => expectActions(page, [["Alt+KeyZ", "zenMode"]]),
  "View: Alt+S": (page) => expectActions(page, [["Alt+KeyS", "objectsSnapMode"]]),
  "View: Ctrl+'": (page) => expectActions(page, [[`${MOD}+Quote`, "gridMode"]]),
  "View: Alt+R": (page) => expectActions(page, [["Alt+KeyR", "viewMode"]]),
  "View: Alt+Shift+D": (page) => expectActions(page, [["Alt+Shift+KeyD", "toggleTheme"]]),
  "View: Alt+/": (page) => expectActions(page, [["Alt+Slash", "stats"]]),
  "View: Ctrl+F": (page) => expectActions(page, [[`${MOD}+f`, "searchMenu"]]),
  "View: Ctrl+/ or Ctrl+Shift+P": async (page) => {
    const palette = (log, key) => log.find((e) => e.type === "palette" && e.key === key).outcome;
    let log = await press(page, `${MOD}+Slash`);
    expect(palette(log, "/").preventDefault).toBe(true);
    expect(log.some((e) => e.type === "keydown" && e.key === "/"), "the palette listener stops it").toBe(false);
    expect((await state(page)).appState.openDialog).toEqual({ name: "commandPalette" });
    log = await press(page, `${MOD}+Shift+KeyP`);
    expect(palette(log, "P").stopPropagation).toBe(true);
    expect((await state(page)).appState.openDialog, "and closes it").toBeNull();
    log = await press(page, `${MOD}+p`);
    expect(effect(log, "commandPaletteHint"), "Ctrl+P hints at the shortcut").toBeTruthy();
  },

  // Editor
  "Editor: Ctrl+Arrow": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    const ctrl = await ctrlKey(page);
    await page.evaluate(() => (window.log = []));
    await page.keyboard.down(ctrl);
    await page.keyboard.press("ArrowRight");
    let log = await page.evaluate(() => window.log);
    expect(effect(log, "flowchartCreate")).toEqual({ type: "flowchartCreate", start: "r1", direction: "right" });
    expect(await position(page, "r1"), "the node does not move").toEqual([0, 0]);
    await page.evaluate(() => (window.log = []));
    await page.keyboard.up(ctrl);
    log = await page.evaluate(() => window.log);
    expect(log.find((e) => e.type === "keyup").outcome.effects.map((e) => e.type)).toContain("flowchartCommit");
    log = await press(page, "Alt+ArrowDown");
    expect(effect(log, "flowchartNavigate")).toEqual({ type: "flowchartNavigate", from: "r1", direction: "down" });
  },
  "Editor: Arrow keys": async (page) => {
    await load(page, [rect("r1", 10, 10), rect("r2", 200, 0)], { selectedElementIds: { r1: true } });
    // regressionTests.test.tsx "arrow keys"
    for (const k of ["ArrowLeft", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowUp", "ArrowDown"]) {
      const log = await press(page, k);
      expect(keydown(log).outcome.preventDefault).toBe(true);
    }
    expect(await position(page, "r1")).toEqual([9, 9]);
    await press(page, "Shift+ArrowRight");
    expect(await position(page, "r1"), "Shift: 5 px").toEqual([14, 9]);
    await load(page, [rect("r1", 0, 0)], { selectedElementIds: { r1: true }, gridModeEnabled: true, gridSize: 20 });
    await press(page, "ArrowDown");
    expect(await position(page, "r1"), "grid on: the grid size").toEqual([0, 20]);
    await press(page, "Shift+ArrowDown");
    expect(await position(page, "r1"), "grid on, Shift: 1 px").toEqual([0, 21]);
  },
  "Editor: Delete / Backspace": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [
      ["Delete", "deleteSelectedElements"],
      ["Backspace", "deleteSelectedElements"],
    ]);
  },
  "Editor: Ctrl+Backspace / Ctrl+Delete": async (page) => {
    for (const k of ["Backspace", "Delete"]) {
      await load(page, TWO_RECTS);
      await press(page, `${MOD}+${k}`);
      expect((await state(page)).keyboard.activeConfirmDialog, k).toBe("clearCanvas");
    }
  },
  "Editor: Ctrl+X / C / V": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [[`${MOD}+x`, "cut"]]);
    // Chromium turns Ctrl+C and Ctrl+V into copy and paste events
    await page.mouse.move(400, 250);
    let log = await press(page, `${MOD}+c`);
    expect(log.find((e) => e.type === "copy")?.outcome).toEqual({ type: "action", action: "copy" });
    log = await press(page, `${MOD}+v`);
    expect(log.find((e) => e.type === "paste")?.outcome).toEqual({ type: "paste", plain: false });
    // Chromium runs no paste command for Ctrl+Shift+V outside a text
    // field; the paste event that follows the keydown is dispatched here
    // (IS_PLAIN_PASTE holds for 100 ms, App.tsx:5692-5700).
    await page.evaluate(() => (window.log = []));
    await page.keyboard.press(`${MOD}+Shift+v`);
    await page.evaluate(() => document.dispatchEvent(new ClipboardEvent("paste", { bubbles: true })));
    log = await page.evaluate(() => window.log);
    expect((await state(page)).keyboard.isPlainPaste).toBe(true);
    expect(log.find((e) => e.type === "paste")?.outcome, "Ctrl+Shift+V pastes plain").toEqual({
      type: "paste",
      plain: true,
    });
    // in a text field the browser keeps them
    await page.focus("#textarea");
    log = await press(page, `${MOD}+c`);
    expect(log.find((e) => e.type === "copy")?.outcome).toEqual({ type: "ignored" });
  },
  "Editor: Ctrl+A": (page) => expectActions(page, [[`${MOD}+a`, "selectAll"]]),
  "Editor: Shift+click": async (page) => {
    await page.keyboard.down("Shift");
    let read = await pointerDown(page);
    await page.keyboard.up("Shift");
    expect(read.shiftKey, "Shift+click adds to the selection").toBe(true);
    const ctrl = await ctrlKey(page);
    await page.keyboard.down(ctrl);
    read = await pointerDown(page);
    await page.keyboard.up(ctrl);
    expect(read.ctrlOrCmd, "Ctrl+click and Ctrl+drag select deep").toBe(true);
  },
  "Editor: Alt+Shift+C": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [["Alt+Shift+KeyC", "copyAsPng"]]);
  },
  "Editor: Ctrl+Alt+C / V": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [
      [`${MOD}+Alt+KeyC`, "copyStyles"],
      [`${MOD}+Alt+KeyV`, "pasteStyles"],
    ]);
  },
  "Editor: Ctrl+[ / ]": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [
      [`${MOD}+BracketLeft`, "sendBackward"],
      [`${MOD}+BracketRight`, "bringForward"],
    ]);
  },
  "Editor: Ctrl+Alt+[ / ] (Mac) or Ctrl+Shift+[ / ]": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await page.evaluate(() => window.keyboard.setDarwin(false));
    await expectActions(page, [
      ["Control+Shift+BracketLeft", "sendToBack"],
      ["Control+Shift+BracketRight", "bringToFront"],
    ]);
    // On a Mac, sendBackward's and bringForward's keyTests (Cmd, no Shift,
    // actionZindex.tsx:17-21, 47-51) take Cmd+Alt+[ / ] too, so the
    // manager cancels as upstream does (manager.tsx:113-118).
    await page.evaluate(() => window.keyboard.setDarwin(true));
    for (const [code, both] of [
      ["BracketLeft", ["sendBackward", "sendToBack"]],
      ["BracketRight", ["bringForward", "bringToFront"]],
    ]) {
      const log = await press(page, `Meta+Alt+${code}`);
      expect(effect(log, "ambiguous")?.actions, code).toEqual(both);
      expect(action(log), code).toBeNull();
    }
  },
  "Editor: Ctrl+Shift+↑ ↓ ← →": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true, r2: true } });
    await expectActions(page, [
      [`${MOD}+Shift+ArrowUp`, "alignTop"],
      [`${MOD}+Shift+ArrowDown`, "alignBottom"],
      [`${MOD}+Shift+ArrowLeft`, "alignLeft"],
      [`${MOD}+Shift+ArrowRight`, "alignRight"],
    ]);
  },
  "Editor: Alt+H / Alt+V": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true, r2: true } });
    await expectActions(page, [
      ["Alt+KeyH", "distributeHorizontally"],
      ["Alt+KeyV", "distributeVertically"],
    ]);
  },
  "Editor: Ctrl+D or Alt+drag": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [[`${MOD}+d`, "duplicateSelection"]]);
    await page.keyboard.down("Alt");
    const read = await pointerDown(page);
    await page.keyboard.up("Alt");
    expect(read.altKey, "Alt+drag duplicates").toBe(true);
  },
  "Editor: Ctrl+Shift+L": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [[`${MOD}+Shift+KeyL`, "toggleElementLock"]]);
  },
  "Editor: Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y": (page) =>
    expectActions(page, [
      [`${MOD}+z`, "undo"],
      [`${MOD}+Shift+z`, "redo"],
      [`${MOD}+y`, "redo"],
    ]),
  "Editor: Ctrl+G / Ctrl+Shift+G": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true, r2: true } });
    await expectActions(page, [
      [`${MOD}+g`, "group"],
      [`${MOD}+Shift+g`, "ungroup"],
    ]);
  },
  "Editor: Shift+H / Shift+V": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [
      ["Shift+H", "flipHorizontal"],
      ["Shift+V", "flipVertical"],
    ]);
  },
  "Editor: Ctrl+Shift+< / >": async (page) => {
    await load(page, [text("t")], { selectedElementIds: { t: true } });
    await expectActions(page, [
      [`${MOD}+Shift+Comma`, "decreaseFontSize"],
      [`${MOD}+Shift+Period`, "increaseFontSize"],
    ]);
  },
  "Editor: Shift (while resizing)": async (page) => {
    await page.keyboard.down("Shift");
    let read = await pointerDown(page);
    await page.keyboard.up("Shift");
    expect(read.maintainAspectRatio, "Shift keeps the aspect ratio").toBe(true);
    expect(read.rotateWithDiscreteAngle, "and snaps the angle while rotating").toBe(true);
    expect(read.resizeFromCenter).toBe(false);
    await page.keyboard.down("Alt");
    read = await pointerDown(page);
    await page.keyboard.up("Alt");
    expect(read.resizeFromCenter, "Alt resizes from the centre").toBe(true);
    expect(read.maintainAspectRatio).toBe(false);
  },
  "Editor: Esc": async (page) => {
    await load(page, TWO_RECTS, { selectedElementIds: { r1: true } });
    await expectActions(page, [["Escape", "deselect"]]);
  },
  "Editor: ?": async (page) => {
    await press(page, "?");
    expect((await state(page)).appState.openDialog).toEqual({ name: "help" });
  },
  "Editor: Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+Shift+E": async (page) => {
    await expectActions(page, [
      [`${MOD}+o`, "loadScene"],
      [`${MOD}+s`, "saveToActiveFile"],
      [`${MOD}+Shift+s`, "saveFileToDisk"],
    ]);
    const log = await press(page, `${MOD}+Shift+e`);
    expect(keydown(log).outcome.preventDefault).toBe(true);
    expect((await state(page)).appState.openDialog).toEqual({ name: "imageExport" });
  },
};

test("every row of the shortcuts page has a test here, and no test lacks a row", () => {
  expect(Object.keys(ROWS).sort()).toEqual(shortcutRows().sort());
});

for (const [row, check] of Object.entries(ROWS)) {
  test(row, async ({ page }) => {
    await open(page);
    await check(page);
    expect(page.errors, "no page errors").toEqual([]);
  });
}
