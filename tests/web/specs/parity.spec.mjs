// The parity suite (ex-531): one test per row of the parity checklist
// (site/content/plan/parity.md), named by the row's id, against the
// <excali-editor> element of the release scripts/web/build.sh builds,
// mounted from plain JS on the page the term.hut integration page
// documents (tests/web/page/editor.html). Each test drives the element
// with Chromium's own pointer and keyboard events and checks what upstream
// does at the pinned commit (the row's upstream column).
//
// A row marked `gap` is not wired into the element yet: its test runs as a
// Playwright expected failure (test.fail), so it still performs the
// gesture and checks upstream's result, and the suite fails once the
// behaviour lands until the row is marked `pass`. The first test fails
// when a row has no test, a test has no row, or a status is unknown.
//
// Coordinates are scene coordinates: the element fills #host from its top
// left corner, at scroll 0 and zoom 1 unless a test changes them.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const MOD = "ControlOrMeta";

/** The rows of the checklist's tables: `{ id, status }` in page order. */
const checklistRows = () => {
  const md = readFileSync(join(REPO_ROOT, "site/content/plan/parity.md"), "utf8");
  const rows = [];
  for (const line of md.split("\n")) {
    if (!line.startsWith("|")) continue;
    const cells = line.split("|").slice(1, -1).map((c) => c.trim());
    if (cells[0] === "Row" || /^-+$/.test(cells[0])) continue;
    rows.push({ id: cells[0], status: cells.at(-1), issue: cells.at(-2) });
  }
  return rows;
};

// -- scenes -------------------------------------------------------------------

let seed = 1;
/** An element as upstream's API.createElement leaves it. */
const element = (type, id, x, y, width = 100, height = 100, extra = {}) => ({
  id,
  type,
  x,
  y,
  width,
  height,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  roundness: null,
  seed: seed++,
  version: 1,
  versionNonce: 1,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
  ...extra,
});

// Filled: upstream hits a transparent shape on its outline only
// (hitElement, element/src/collision.ts), so a click inside needs a fill.
const rect = (id, x, y, w = 100, h = 100) =>
  element("rectangle", id, x, y, w, h, { backgroundColor: "#a5d8ff" });

const text = (id, x, y, value) =>
  element("text", id, x, y, 20, 25, {
    text: value,
    originalText: value,
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    autoResize: true,
    lineHeight: 1.25,
  });

const sceneText = (elements) =>
  JSON.stringify({
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements,
    appState: { gridSize: 20, viewBackgroundColor: "#ffffff" },
    files: {},
  });

const BOUND = readFileSync(
  join(REPO_ROOT, "crates", "excali-wasm", "tests", "fixtures", "bound.excalidraw"),
  "utf8",
);

const TOOLBAR = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures", "toolbar.json"), "utf8"),
);

// -- the page -----------------------------------------------------------------

/**
 * Opens the editor page and mounts an <excali-editor>, with `scene` loaded
 * when given. Returns the page's errors (uncaught exceptions and console
 * errors), which every test expects to stay empty.
 */
const mount = async (page, scene = null) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(async (text) => {
    const ed = document.createElement("excali-editor");
    window.events = [];
    for (const type of ["change", "save-request", "open-link"]) {
      ed.addEventListener(type, (e) => window.events.push({ type, detail: e.detail }));
    }
    document.getElementById("host").appendChild(ed);
    window.ed = ed;
    if (text !== null) await ed.load(text);
  }, scene);
  // upstream listens for keys on its container unless
  // handleKeyboardGlobally (App.tsx:4178), so a press on the canvas focuses it
  await click(page, EMPTY);
  return errors;
};

const state = (page) => page.evaluate(() => window.ed.getState());

/** The scene's non-deleted elements as save() writes them, in order. */
const saved = async (page) =>
  (await page.evaluate(() => JSON.parse(window.ed.save()).elements)).filter((e) => !e.isDeleted);

const byId = async (page) => Object.fromEntries((await saved(page)).map((e) => [e.id, e]));

/** Client coordinates of a scene point (scroll 0, zoom 1). */
const client = async (page, [x, y]) => {
  const box = await page.locator("excali-editor").boundingBox();
  return [box.x + x, box.y + y];
};

const click = async (page, at, options = {}) => {
  const [x, y] = await client(page, at);
  await page.mouse.click(x, y, options);
};

const drag = async (page, from, to, steps = 4) => {
  const [x0, y0] = await client(page, from);
  const [x1, y1] = await client(page, to);
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x1, y1, { steps });
  await page.mouse.up();
};

const press = (page, key) => page.keyboard.press(key);

/** Draws with the tool of `key` from `from` to `to`; the new element. */
const draw = async (page, key, from, to) => {
  const before = new Set((await saved(page)).map((e) => e.id));
  await press(page, key);
  await drag(page, from, to);
  const created = (await saved(page)).filter((e) => !before.has(e.id));
  expect(created).toHaveLength(1);
  return created[0];
};

/** Whether a click at `at` selects something (then clears it again). */
const hits = async (page, at) => {
  await click(page, at);
  const hit = (await state(page)).selectionCount > 0;
  await click(page, EMPTY);
  return hit;
};

const SHORT = { timeout: 2_000 };

/** A point of the canvas no test scene covers. */
const EMPTY = [850, 300];

// -- the rows -----------------------------------------------------------------

const ROWS = {
  // File and host API

  "file-roundtrip": async ({ page }) => {
    const errors = await mount(page, BOUND);
    const once = await page.evaluate(() => window.ed.save());
    await page.evaluate((t) => window.ed.load(t), once);
    const twice = await page.evaluate(() => window.ed.save());
    expect(twice).toBe(once);
    const input = JSON.parse(BOUND).elements.map((e) => [e.id, e.type, e.x, e.y]);
    expect(JSON.parse(once).elements.map((e) => [e.id, e.type, e.x, e.y])).toEqual(input);
    expect(errors).toEqual([]);
  },

  "file-envelope": async ({ page }) => {
    await mount(page, BOUND);
    const text = await page.evaluate(() => window.ed.save());
    const data = JSON.parse(text);
    expect(Object.keys(data)).toEqual(["type", "version", "source", "elements", "appState", "files"]);
    expect([data.type, data.version, typeof data.source]).toEqual(["excalidraw", 2, "string"]);
    const exported = ["gridSize", "gridStep", "gridModeEnabled", "viewBackgroundColor", "lockedMultiSelections"];
    for (const key of Object.keys(data.appState)) expect(exported).toContain(key);
    expect(data.files).toEqual({});
    expect(text).toBe(JSON.stringify(data, null, 2));
  },

  "file-legacy-indices": async ({ page }) => {
    const legacy = [rect("one", 0, 0), rect("two", 200, 0), rect("three", 400, 0)];
    await mount(page, sceneText(legacy));
    expect((await saved(page)).map((e) => [e.id, e.index])).toEqual([
      ["one", "a0"],
      ["two", "a1"],
      ["three", "a2"],
    ]);
  },

  "library-v1": async ({ page }) => {
    await mount(page);
    const v1 = JSON.stringify({
      type: "excalidrawlib",
      version: 1,
      source: "https://excalidraw.com",
      library: [[rect("lib-a", 0, 0)], [rect("lib-b", 0, 0), rect("lib-c", 20, 20)]],
    });
    expect(await page.evaluate((t) => window.ed.importLibrary(t), v1)).toBe(2);
    const out = JSON.parse(await page.evaluate(() => window.ed.exportLibrary()));
    expect([out.type, out.version]).toEqual(["excalidrawlib", 2]);
    const ids = out.libraryItems.map((item) => item.elements.map((e) => e.id).sort());
    expect(ids.sort()).toEqual([["lib-a"], ["lib-b", "lib-c"]]);
  },

  "export-svg": async ({ page }) => {
    await mount(page, sceneText([rect("r", 0, 0)]));
    const plain = await page.evaluate(() => window.ed.export("svg"));
    // SVG_DOCUMENT_PREAMBLE + svg.outerHTML (common/src/constants.ts:410,
    // excalidraw/data/index.ts:143)
    expect(plain.startsWith('<?xml version="1.0" standalone="no"?>')).toBe(true);
    expect(plain).toMatch(/^<\?xml[^>]*>\n<!DOCTYPE svg [^>]*>\n<svg /);
    expect(plain).toContain("<!-- svg-source:excalidraw -->");
    expect(plain).not.toContain("payload-type:");
    const embedded = await page.evaluate(() => window.ed.export("svg", { embedScene: true }));
    expect(embedded).toContain("<!-- payload-type:application/vnd.excalidraw+json -->");
  },

  "export-png-size": async ({ page }) => {
    await mount(page, sceneText([rect("r", 0, 0)]));
    const header = await page.evaluate(async () => {
      const blob = await window.ed.export("png");
      const bytes = new Uint8Array(await blob.arrayBuffer());
      const view = new DataView(bytes.buffer);
      return {
        type: blob.type,
        signature: [...bytes.slice(0, 8)],
        width: view.getUint32(16),
        height: view.getUint32(20),
      };
    });
    expect(header).toEqual({
      type: "image/png",
      signature: [137, 80, 78, 71, 13, 10, 26, 10],
      width: 120,
      height: 120,
    });
  },

  "host-events": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    await page.evaluate(() => (window.events = []));
    await drag(page, [150, 150], [190, 150]);
    expect(await page.evaluate(() => window.events.at(-1))).toEqual({
      type: "change",
      detail: { dirty: true },
    });
    await press(page, `${MOD}+s`);
    expect(await page.evaluate(() => window.events.at(-1))).toEqual({
      type: "save-request",
      detail: {},
    });
  },

  // Canvas and view

  "view-theme": async ({ page }) => {
    await mount(page);
    const dark = () =>
      page.evaluate(() => window.ed.querySelector(".excalidraw").classList.contains("theme--dark"));
    expect(await dark()).toBe(false);
    await page.evaluate(() => window.ed.setAttribute("theme", "dark"));
    expect(await dark()).toBe(true);
    await page.evaluate(() => window.ed.setAttribute("theme", "light"));
    expect(await dark()).toBe(false);
  },

  "view-device-pixels": async ({ browser }) => {
    const context = await browser.newContext({ deviceScaleFactor: 2 });
    const page = await context.newPage();
    await mount(page);
    const sizes = await page.evaluate(() =>
      [...window.ed.querySelectorAll("canvas")].map((c) => [
        c.width,
        c.height,
        c.getBoundingClientRect().width,
        c.getBoundingClientRect().height,
      ]),
    );
    expect(sizes.length).toBeGreaterThan(0);
    for (const [w, h, cssW, cssH] of sizes) expect([w, h]).toEqual([cssW * 2, cssH * 2]);
    await context.close();
  },

  "view-zoom-keys": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    await press(page, `${MOD}+Equal`);
    expect((await state(page)).zoom).toBeCloseTo(1.1, 9);
    await press(page, `${MOD}+Digit0`);
    expect((await state(page)).zoom).toBe(1);
  },

  "view-wheel-zoom": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    const [x, y] = await client(page, [500, 350]);
    await page.mouse.move(x, y);
    await page.keyboard.down("Control");
    await page.mouse.wheel(0, -100);
    await page.keyboard.up("Control");
    await expect.poll(async () => (await state(page)).zoom, SHORT).toBeGreaterThan(1);
  },

  "view-wheel-scroll": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    expect(await hits(page, [150, 50])).toBe(false);
    const [x, y] = await client(page, [500, 350]);
    await page.mouse.move(x, y);
    await page.mouse.wheel(0, 100);
    await expect.poll(() => hits(page, [150, 50]), SHORT).toBe(true);
  },

  "view-hand-pan": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    await press(page, "h");
    expect((await state(page)).activeTool).toBe("hand");
    await drag(page, [500, 500], [500, 400]);
    await press(page, "v");
    expect(await hits(page, [150, 50])).toBe(true);
  },

  "view-space-pan": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100)]));
    await page.keyboard.down("Space");
    await drag(page, [500, 500], [500, 400]);
    await page.keyboard.up("Space");
    expect(await hits(page, [150, 50])).toBe(true);
  },

  // renderInteractiveScene: the selection's outline and handles are drawn
  // on the interactive canvas, which is clear with nothing selected
  "view-interactive": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    const painted = () =>
      page.evaluate(() => {
        const canvas = document.querySelector("excali-editor canvas.interactive");
        const scale = canvas.width / canvas.getBoundingClientRect().width;
        const ctx = canvas.getContext("2d");
        // the south-east handle's outline, (202, 202)-(210, 210)
        const data = ctx.getImageData(
          Math.round(201 * scale),
          Math.round(201 * scale),
          Math.round(10 * scale),
          Math.round(10 * scale),
        ).data;
        let alpha = 0;
        for (let i = 3; i < data.length; i += 4) alpha = Math.max(alpha, data[i]);
        return alpha;
      });
    expect(await painted()).toBe(0);
    await click(page, [150, 150]);
    expect(await painted()).toBeGreaterThan(0);
  },

  // Tools

  "tool-toolbar-order": async ({ page }) => {
    await mount(page);
    const ids = [];
    const walk = (node) => {
      if (Array.isArray(node)) return node.forEach(walk);
      if (!node || typeof node !== "object") return;
      if (node.attrs?.["data-testid"]) ids.push(node.attrs["data-testid"]);
      (node.children || []).forEach(walk);
    };
    walk(TOOLBAR.cases.find((c) => c.name === "default").dom);
    const dom = await page.evaluate(() =>
      [...window.ed.querySelectorAll(".App-toolbar [data-testid]")].map((e) => e.dataset.testid),
    );
    expect(dom).toEqual(ids);
  },

  "tool-toolbar-click": async ({ page }) => {
    await mount(page);
    await page.locator('[data-testid="toolbar-rectangle"]').click();
    expect((await state(page)).activeTool).toBe("rectangle");
    await page.locator('[data-testid="toolbar-ellipse"]').click();
    expect((await state(page)).activeTool).toBe("ellipse");
  },

  "tool-letters": async ({ page }) => {
    await mount(page);
    const letters = {
      r: "rectangle",
      d: "diamond",
      o: "ellipse",
      a: "arrow",
      l: "line",
      p: "freedraw",
      t: "text",
      e: "eraser",
      h: "hand",
      v: "selection",
    };
    for (const [key, tool] of Object.entries(letters)) {
      await press(page, key);
      expect([key, (await state(page)).activeTool]).toEqual([key, tool]);
    }
  },

  "tool-rectangle": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "r", [100, 100], [250, 200]);
    expect(e).toMatchObject({
      type: "rectangle",
      x: 100,
      y: 100,
      width: 150,
      height: 100,
      angle: 0,
      strokeColor: "#1e1e1e",
      backgroundColor: "transparent",
      fillStyle: "solid",
      strokeWidth: 2,
      strokeStyle: "solid",
      roughness: 1,
      opacity: 100,
      roundness: { type: 3 },
      groupIds: [],
      frameId: null,
      locked: false,
    });
    expect(await state(page)).toMatchObject({ activeTool: "selection", selectionCount: 1 });
  },

  "tool-diamond": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "d", [100, 100], [250, 200]);
    expect(e).toMatchObject({ type: "diamond", x: 100, y: 100, width: 150, height: 100, roundness: { type: 2 } });
  },

  "tool-ellipse": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "o", [100, 100], [250, 200]);
    // getCurrentItemRoundness("ellipse") outside upstream's test mode
    expect(e).toMatchObject({ type: "ellipse", x: 100, y: 100, width: 150, height: 100, roundness: { type: 2 } });
  },

  "tool-arrow": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "a", [100, 100], [250, 200]);
    expect(e).toMatchObject({
      type: "arrow",
      x: 100,
      y: 100,
      points: [[0, 0], [150, 100]],
      startArrowhead: null,
      endArrowhead: "arrow",
    });
  },

  "tool-line": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "l", [100, 100], [250, 200]);
    expect(e).toMatchObject({
      type: "line",
      x: 100,
      y: 100,
      points: [[0, 0], [150, 100]],
      startArrowhead: null,
      endArrowhead: null,
    });
  },

  // multiPointCreate.test.tsx:88-128, moved by (300, 300) off the menu
  "tool-arrow-points": async ({ page }) => {
    await mount(page);
    await press(page, "a");
    for (const p of [[330, 330], [350, 360], [400, 440]]) {
      const [x, y] = await client(page, p);
      await page.mouse.move(x, y);
      await page.mouse.down();
      await page.mouse.up();
    }
    await press(page, "Enter");
    const arrows = (await saved(page)).filter((e) => e.type === "arrow");
    expect(arrows).toHaveLength(1);
    expect(arrows[0]).toMatchObject({ x: 330, y: 330, points: [[0, 0], [20, 30], [70, 110]] });
    expect((await state(page)).activeTool).toBe("selection");
  },

  "tool-freedraw": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "p", [100, 100], [250, 200]);
    expect(e).toMatchObject({ type: "freedraw", x: 100, y: 100 });
    expect(e.points.length).toBeGreaterThan(2);
    expect(e.points.at(-1)).toEqual([150, 100]);
  },

  "tool-text": async ({ page }) => {
    await mount(page);
    await press(page, "t");
    await click(page, [300, 300]);
    const editor = page.locator("excali-editor textarea");
    await expect(editor).toBeVisible(SHORT);
    await page.keyboard.type("hi");
    await press(page, "Escape");
    const texts = (await saved(page)).filter((e) => e.type === "text");
    expect(texts).toHaveLength(1);
    expect(texts[0]).toMatchObject({ text: "hi", fontSize: 20, fontFamily: 5 });
  },

  "tool-eraser": async ({ page }) => {
    await mount(page, sceneText([rect("r", 100, 100), rect("keep", 400, 100)]));
    await press(page, "e");
    await drag(page, [80, 150], [220, 150], 8);
    expect((await saved(page)).map((e) => e.id)).toEqual(["keep"]);
  },

  "tool-frame": async ({ page }) => {
    await mount(page);
    const e = await draw(page, "f", [100, 100], [400, 300]);
    expect(e).toMatchObject({ type: "frame", x: 100, y: 100, width: 300, height: 200, name: null });
  },

  "tool-lock": async ({ page }) => {
    await mount(page);
    await press(page, "q");
    await draw(page, "r", [100, 100], [200, 200]);
    expect((await state(page)).activeTool).toBe("rectangle");
    await drag(page, [300, 100], [400, 200]);
    expect((await saved(page)).filter((e) => e.type === "rectangle")).toHaveLength(2);
  },

  // Selection and transforms

  "select-click": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100)]));
    await click(page, [150, 150]);
    expect((await state(page)).selectionCount).toBe(1);
    await click(page, [350, 150]);
    expect((await state(page)).selectionCount).toBe(1);
    await click(page, [700, 500]);
    expect((await state(page)).selectionCount).toBe(0);
  },

  "select-shift": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100)]));
    await click(page, [150, 150]);
    await page.keyboard.down("Shift");
    await click(page, [350, 150]);
    await page.keyboard.up("Shift");
    expect((await state(page)).selectionCount).toBe(2);
  },

  "select-box": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100), rect("c", 600, 400)]));
    // from below the main menu's trigger, which covers the top-left corner
    await drag(page, [60, 80], [450, 250]);
    expect((await state(page)).selectionCount).toBe(2);
  },

  "select-all": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100), rect("c", 600, 400)]));
    await press(page, `${MOD}+a`);
    expect((await state(page)).selectionCount).toBe(3);
  },

  "move-drag": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await drag(page, [150, 150], [190, 170]);
    expect((await byId(page)).a).toMatchObject({ x: 140, y: 120 });
  },

  "move-nudge": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await press(page, "ArrowRight");
    expect((await byId(page)).a).toMatchObject({ x: 101, y: 100 });
    await press(page, "Shift+ArrowDown");
    expect((await byId(page)).a).toMatchObject({ x: 101, y: 105 });
  },

  // move.test.tsx:147-195
  "move-alt-duplicate": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await page.keyboard.down("Alt");
    await drag(page, [150, 150], [190, 170]);
    await page.keyboard.up("Alt");
    const elements = await saved(page);
    expect(elements).toHaveLength(2);
    expect(elements[0]).toMatchObject({ id: "a", x: 100, y: 100 });
    expect(elements[1]).toMatchObject({ x: 140, y: 120 });
  },

  // snapDraggedElements: b's left edge, 3 px right of a's right edge, snaps
  "move-snap": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 300)]));
    await press(page, "Alt+KeyS");
    await click(page, [350, 350]);
    await drag(page, [350, 350], [253, 350]);
    expect((await byId(page)).b).toMatchObject({ x: 200, y: 300 });
  },

  // frame.test.tsx:689-695
  "move-into-frame": async ({ page }) => {
    const frame = element("frame", "f", 0, 0, 150, 150, { name: null });
    await mount(page, sceneText([rect("r", 200, 0, 50, 50), frame]));
    await click(page, [225, 25]);
    await drag(page, [225, 25], [75, 75]);
    expect((await byId(page)).r.frameId).toBe("f");
  },

  // linearElementEditor.test.tsx:411-418, 500-530, moved by (300, 300)
  // off the menu
  "linear-editor": async ({ page }) => {
    const line = element("line", "l", 320, 320, 40, 0, {
      roughness: 0,
      points: [[0, 0], [40, 0]],
    });
    await mount(page, sceneText([line]));
    await click(page, [320, 320]);
    await click(page, [320, 320], { clickCount: 2 });
    expect(
      await page.evaluate(() => JSON.parse(window.ed.save()).elements.length),
    ).toBe(1);
    await drag(page, [340, 320], [390, 370]);
    expect((await byId(page)).l.points).toEqual([[0, 0], [70, 50], [40, 0]]);
  },

  // getTransformHandlesFromCoords at zoom 1 for the mouse (size 8, margin 4,
  // spacing 2): the south-east handle of (100, 100)–(200, 200) spans
  // (202, 202)–(210, 210) and the rotation handle (146, 74)–(154, 82).
  "resize-handle": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await drag(page, [206, 206], [256, 236]);
    expect((await byId(page)).a).toMatchObject({ x: 100, y: 100, width: 150, height: 130 });
  },

  "rotate-handle": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await drag(page, [150, 78], [300, 150]);
    const a = (await byId(page)).a;
    expect(a.angle).toBeGreaterThan(0);
    expect([a.width, a.height]).toEqual([100, 100]);
  },

  // Editing

  "edit-delete": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100)]));
    await click(page, [150, 150]);
    await press(page, "Delete");
    expect((await saved(page)).map((e) => e.id)).toEqual(["b"]);
  },

  "edit-duplicate": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await press(page, `${MOD}+d`);
    const all = await saved(page);
    expect(all).toHaveLength(2);
    const copy = all.find((e) => e.id !== "a");
    expect(copy).toMatchObject({ type: "rectangle", x: 110, y: 110, width: 100, height: 100 });
  },

  "edit-group": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100)]));
    await press(page, `${MOD}+a`);
    await press(page, `${MOD}+g`);
    const { a, b } = await byId(page);
    expect(a.groupIds).toHaveLength(1);
    expect(b.groupIds).toEqual(a.groupIds);
  },

  "edit-zorder": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 300, 100)]));
    await click(page, [150, 150]);
    const mac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    if (mac) {
      // Cmd+Alt+] passes both actionBringToFront's and actionBringForward's
      // key tests, and the ActionManager cancels an ambiguous key
      await press(page, "Meta+Alt+BracketRight");
      expect((await saved(page)).map((e) => e.id)).toEqual(["a", "b"]);
      await press(page, "Meta+BracketRight");
    } else {
      await press(page, "Control+Shift+BracketRight");
    }
    expect((await saved(page)).map((e) => e.id)).toEqual(["b", "a"]);
  },

  "edit-undo-redo": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await drag(page, [150, 150], [190, 150]);
    expect((await byId(page)).a.x).toBe(140);
    await press(page, `${MOD}+z`);
    expect((await byId(page)).a.x).toBe(100);
    await press(page, `${MOD}+Shift+z`);
    expect((await byId(page)).a.x).toBe(140);
  },

  "edit-copy-paste": async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    await press(page, `${MOD}+c`);
    await page.mouse.move(...(await client(page, [500, 400])));
    await press(page, `${MOD}+v`);
    await expect.poll(async () => (await saved(page)).length, SHORT).toBe(2);
  },

  // Bound text and arrows (crates/excali-wasm/tests/fixtures/bound.excalidraw:
  // "box" at (60, 60) 200 × 100 with the label "label"; "a" and "b", 100 ×
  // 100 at (60, 300) and (400, 300), joined by the arrow "link")

  "bound-label-follows": async ({ page }) => {
    await mount(page, BOUND);
    const before = await byId(page);
    await drag(page, [160, 110], [200, 130]);
    const after = await byId(page);
    expect([after.box.x - before.box.x, after.box.y - before.box.y]).toEqual([40, 20]);
    expect([after.label.x - before.label.x, after.label.y - before.label.y]).toEqual([40, 20]);
  },

  "bound-arrow-follows": async ({ page }) => {
    await mount(page, BOUND);
    const before = await byId(page);
    const end = (e) => [e.x + e.points.at(-1)[0], e.y + e.points.at(-1)[1]];
    await drag(page, [450, 350], [450, 450]);
    const after = await byId(page);
    expect(after.b.y).toBe(400);
    expect(end(after.link)[1]).toBeGreaterThan(end(before.link)[1] + 50);
  },

  "bound-arrow-create": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100), rect("b", 400, 100)]));
    const e = await draw(page, "a", [150, 150], [450, 150]);
    expect(e.startBinding?.elementId).toBe("a");
    expect(e.endBinding?.elementId).toBe("b");
  },

  "text-dblclick-edit": async ({ page }) => {
    await mount(page, sceneText([text("t", 100, 100, "hello")]));
    await click(page, [110, 110], { clickCount: 2 });
    const editor = page.locator("excali-editor textarea");
    await expect(editor).toBeVisible(SHORT);
    expect([await editor.getAttribute("dir"), await editor.getAttribute("wrap")]).toEqual([
      "auto",
      "off",
    ]);
    expect(await editor.inputValue()).toBe("hello");
  },

  "text-dblclick-label": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150], { clickCount: 2 });
    await expect(page.locator("excali-editor textarea")).toBeVisible(SHORT);
  },

  // Chrome

  "ui-main-menu": async ({ page }) => {
    await mount(page);
    await page.locator('excali-editor [data-testid="main-menu-trigger"]').click(SHORT);
    await expect(page.locator("excali-editor .dropdown-menu")).toBeVisible(SHORT);
  },

  "ui-styles-panel": async ({ page }) => {
    await mount(page, sceneText([rect("a", 100, 100)]));
    await click(page, [150, 150]);
    const panel = page.locator("excali-editor .App-menu__left");
    await expect(panel).toBeVisible(SHORT);
    await expect(panel.locator('button.color-picker__button[aria-label="Stroke"]')).toBeVisible(SHORT);
  },

  "ui-footer-zoom": async ({ page }) => {
    await mount(page);
    const zoom = page.locator("excali-editor .zoom-actions");
    await expect(zoom).toBeVisible(SHORT);
    await expect(zoom).toContainText("100%");
  },

  "ui-help-dialog": async ({ page }) => {
    await mount(page);
    await press(page, "Shift+Slash");
    // Dialog's Modal portals to the body (hooks/useCreatePortalContainer.ts)
    const dialog = page.locator("body > .excalidraw-modal-container .HelpDialog");
    await expect(dialog).toBeVisible(SHORT);
    await expect(dialog.locator(".HelpDialog__island-title")).toHaveText(["Tools", "View", "Editor"]);
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0, SHORT);
  },

  "ui-context-menu": async ({ page }) => {
    await mount(page);
    await click(page, [500, 350], { button: "right" });
    await expect(page.locator("excali-editor .context-menu")).toBeVisible(SHORT);
  },

  "ui-library": async ({ page }) => {
    await mount(page);
    await page.locator("excali-editor .default-sidebar-trigger").click(SHORT);
    await expect(page.locator("excali-editor .default-sidebar")).toBeVisible(SHORT);
    await expect(page.locator("excali-editor .layer-ui__library")).toBeVisible(SHORT);
  },

  "ui-command-palette": async ({ page }) => {
    const errors = await mount(page);
    await press(page, `${MOD}+Slash`);
    // Dialog's Modal portals to the body (hooks/useCreatePortalContainer.ts)
    const palette = page.locator("body > .excalidraw-modal-container .command-palette-dialog");
    await expect(palette).toBeVisible(SHORT);
    // getCategoryOrder (CommandPalette.tsx:97-114); Links are the hosted app's
    await expect(palette.locator(".command-category-title")).toHaveText([
      "App",
      "Export",
      "Editor",
      "Tools",
      "Elements",
      "Links",
    ]);
    // the search field has the focus; the best match is selected and Enter runs it
    await page.keyboard.type("ellipse");
    await expect(palette.locator(".command-item.item-selected .name")).toHaveText("Ellipse", SHORT);
    await page.keyboard.press("Enter");
    await expect(palette).toHaveCount(0, SHORT);
    expect((await state(page)).activeTool).toBe("ellipse");
    // Ctrl/Cmd+Shift+P opens it again with the last command under "Recently used"
    await press(page, `${MOD}+Shift+KeyP`);
    await expect(palette).toBeVisible(SHORT);
    await expect(palette.locator(".command-category-title").first()).toHaveText("Recently used");
    await page.keyboard.press("Escape");
    await expect(palette).toHaveCount(0, SHORT);
    expect(errors).toEqual([]);
  },

  "ui-search": async ({ page }) => {
    const errors = await mount(
      page,
      sceneText([
        text("t1", 100, 300, "say hello"),
        text("t2", 100, 100, "hello world"),
        element("frame", "f1", 400, 100, 200, 150, { name: "hello frame" }),
      ]),
    );
    await press(page, `${MOD}+KeyF`);
    const search = page.locator("excali-editor .default-sidebar .layer-ui__search");
    await expect(search).toBeVisible(SHORT);
    await expect(search.locator(".layer-ui__search-header input")).toBeFocused(SHORT);
    await page.keyboard.type("hello");
    // handleSearch is debounced (350 ms); frames first, texts by y
    await expect(search.locator(".layer-ui__search-count")).toContainText("1 / 3 results", SHORT);
    await expect(search.locator(".layer-ui__search-result-title")).toHaveText(["Frames", "Texts"]);
    await expect(search.locator(".layer-ui__result-item")).toHaveText([
      "hello  frame",
      "hello  world",
      "say hello",
    ]);
    await expect(search.locator(".layer-ui__result-item.active")).toHaveText("hello  frame");
    await page.keyboard.press("Enter");
    await expect(search.locator(".layer-ui__result-item.active")).toHaveText("hello  world", SHORT);
    await expect(search.locator(".layer-ui__search-count")).toContainText("2 / 3 results");
    await page.keyboard.press("Escape");
    await expect(page.locator("excali-editor .default-sidebar")).toHaveCount(0, SHORT);
    expect(errors).toEqual([]);
  },

  "ui-welcome-screen": async ({ page }) => {
    await mount(page);
    await expect(page.locator("excali-editor .welcome-screen-center")).toBeVisible(SHORT);
  },

  // en.json hints.freeDraw: "Click and drag, release when you're finished"
  "ui-hints": async ({ page }) => {
    await mount(page);
    await press(page, "p");
    await expect(page.locator("excali-editor .HintViewer")).toContainText(
      "Click and drag, release when you're finished",
      SHORT,
    );
  },

  "ui-stats": async ({ page }) => {
    await mount(page);
    await press(page, "Alt+Slash");
    await expect(page.locator("excali-editor .exc-stats")).toBeVisible(SHORT);
  },
};

// -- the suite ----------------------------------------------------------------

const rows = checklistRows();

test("every checklist row has a test, and every test a row", () => {
  const ids = rows.map((r) => r.id);
  expect(ids.length).toBeGreaterThan(0);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids.filter((id) => !(id in ROWS))).toEqual([]);
  expect(Object.keys(ROWS).filter((id) => !ids.includes(id))).toEqual([]);
  expect(rows.filter((r) => !["pass", "gap"].includes(r.status))).toEqual([]);
  expect(rows.filter((r) => !/^ex-\d+$/.test(r.issue))).toEqual([]);
});

for (const row of rows) {
  const body = ROWS[row.id];
  if (!body) continue;
  if (row.status === "gap") test.fail(row.id, body);
  else test(row.id, body);
}
