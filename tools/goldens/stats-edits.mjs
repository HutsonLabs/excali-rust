#!/usr/bin/env node
// Stats panel edit fixtures for excali-editor (ex-539): upstream's own Stats
// (packages/excalidraw/components/Stats/index.tsx) over a real Scene,
// rendered by React into jsdom as tools/goldens/stats.mjs renders it, and
// driven the way a user drives it: a value typed into a DragInput and
// committed with Enter, or its label pressed, dragged along x (Shift held
// or not) and released. The callbacks that run are upstream's
// Position, Dimension, Angle, FontSize, MultiPosition, MultiDimension,
// MultiAngle and MultiFontSize, with DragInput's pointer arithmetic
// (sensitivity, SMALLEST_DELTA) and its snapshots of the scene.
//
//   node tools/goldens/stats-edits.mjs            write the fixture
//   node tools/goldens/stats-edits.mjs --check    exit 1 if stale
//   node tools/goldens/stats-edits.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/stats_edits.json:
//
//   { description, upstream, scene: [elements], cases: [...] }
//
// Each case is { name, appState, input, gesture, result }:
// - appState: the keys set over getDefaultAppState() (the selection, the
//   selected groups, croppingElementId);
// - input: the DragInput's data-testid ("X", "Y", "W", "H", "A", "F");
// - gesture: { typed: "text" } (focus, the text typed, Enter) or
//   { drag: [{ x, shift }] } (pointerdown on the label at the first x,
//   pointermove to each x in turn, pointerup);
// - result: { order (the scene's ids after), changed (every element whose
//   JSON differs from the scene before, in full), patches (what the
//   callbacks passed to setAppState, elementsToHighlight as ids), captures
//   (the captureUpdate of each app.syncActionResult call) }.
//
// Text is measured with upstream's test metric (10 px per UTF-16 code
// unit), and the character width cache starts every case empty, as in
// tools/goldens/transform-fixtures.mjs. Upstream runs in its test mode
// (updated timestamps 1), reseed(1) before the scene is built.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { installDom, SHIMS, STUBS } from "./stats.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "stats_edits.json";

const ENTRY = `
export { Stats } from "./packages/excalidraw/components/Stats/index";
export { Scene } from "./packages/element/src/Scene";
export { redrawTextBoundingBox } from "./packages/element/src/textElement";
export { reseed } from "./packages/common/src/random";
export {
  DEFAULT_VERTICAL_ALIGN,
  ROUNDNESS,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
} from "./packages/common/src/index";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider, charWidth } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

const PATCH = {
  // charWidth.reset() forgets every font, as a fresh page starts (see
  // transform-fixtures.mjs)
  "packages/element/src/textMeasurements": (source) => {
    const from = "    clearCache,\n  };";
    if (source.split(from).length !== 2) throw new Error("textMeasurements: charWidth shape changed");
    return source.replace(
      from,
      "    clearCache,\n    reset: () => {\n      for (const font of Object.keys(cachedCharWidth)) delete cachedCharWidth[font];\n    },\n  };",
    );
  },
};

const usage = () => {
  process.stderr.write("usage: stats-edits.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: FIXTURES_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

const clone = (value) => (value === undefined ? null : JSON.parse(JSON.stringify(value)));

// -- the scene ------------------------------------------------------------------

const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

/** A text element measured as newTextElement measures it. */
const text = (up, opts) =>
  up.newTextElement({
    x: 0,
    y: 0,
    strokeColor: "#1e1e1e",
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 0,
    opacity: 100,
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: up.DEFAULT_VERTICAL_ALIGN,
    ...opts,
  });

/** `label` bound to `container`, laid out as the text editor commits it. */
const bindLabel = (up, elements, container, id, labelText, opts = {}) => {
  const label = text(up, {
    id,
    text: labelText,
    originalText: labelText,
    containerId: container.id,
    textAlign: "center",
    verticalAlign: "middle",
    ...opts,
  });
  container.boundElements = [...(container.boundElements ?? []), { id: label.id, type: "text" }];
  elements.push(label);
  const scene = new up.Scene(elements, { skipValidation: true });
  up.redrawTextBoundingBox(label, container, scene);
  return label;
};

const bindArrow = (arrow, startOrEnd, target, fixedPoint) => {
  arrow[startOrEnd === "start" ? "startBinding" : "endBinding"] = {
    elementId: target.id,
    fixedPoint,
    mode: "orbit",
  };
  target.boundElements = [...(target.boundElements ?? []), { id: arrow.id, type: "arrow" }];
};

const buildScene = (up) => {
  up.reseed(1);
  up.charWidth.reset();
  const elements = [];
  const push = (e) => {
    elements.push(e);
    return e;
  };
  const r = push(el(up, { id: "r", x: 10, y: 20, width: 100, height: 50 }));
  push(el(up, { id: "rot", x: 200, y: 40, width: 80, height: 60, angle: 0.5 }));
  push(text(up, { id: "t", text: "hello", originalText: "hello", x: 0, y: 100 }));
  const box = push(el(up, { id: "box", x: 0, y: 200, width: 120, height: 60 }));
  bindLabel(up, elements, box, "lbl", "label");
  const arr = push(
    el(up, {
      id: "arr",
      type: "arrow",
      x: 60,
      y: 75,
      width: 0,
      height: 120,
      roundness: null,
      points: [
        [0, 0],
        [0, 120],
      ],
    }),
  );
  bindArrow(arr, "start", r, [0.5, 1.1]);
  bindArrow(arr, "end", box, [0.5, -0.0834]);
  push({ ...el(up, { id: "f", type: "frame", x: 500, y: 0, width: 300, height: 200 }), name: null });
  push(el(up, { id: "c", x: 520, y: 20, width: 50, height: 50, frameId: "f" }));
  push(el(up, { id: "out", x: 850, y: 50, width: 40, height: 40 }));
  push(el(up, { id: "g1", x: 0, y: 400, width: 40, height: 40, groupIds: ["grp"] }));
  push(el(up, { id: "g2", x: 60, y: 420, width: 40, height: 40, angle: 0.25, groupIds: ["grp"] }));
  push(el(up, { id: "img", type: "image", x: 0, y: 600, width: 100, height: 80, fileId: "file" }));
  push({
    ...el(up, { id: "crp", type: "image", x: 200, y: 600, width: 100, height: 80, fileId: "file", scale: [-1, 1] }),
    crop: { x: 40, y: 30, width: 200, height: 160, naturalWidth: 400, naturalHeight: 300 },
  });
  push(
    el(up, {
      id: "ln",
      type: "line",
      x: 400,
      y: 600,
      width: 100,
      height: 90,
      points: [
        [0, 0],
        [100, 40],
        [30, 90],
      ],
    }),
  );
  push(
    el(up, {
      id: "elb",
      type: "arrow",
      x: 600,
      y: 600,
      width: 100,
      height: 50,
      roundness: null,
      elbowed: true,
      points: [
        [0, 0],
        [100, 0],
        [100, 50],
      ],
    }),
  );
  const note = push(el(up, { id: "note", type: "stickynote", x: 0, y: 800, width: 200, height: 200 }));
  bindLabel(up, elements, note, "noteText", "a sticky note", { fontSize: 28, baseFontSize: 28 });
  // the scene syncs fractional indices as it takes the elements
  return clone(new up.Scene(elements, { skipValidation: true }).getElementsIncludingDeleted());
};

// -- cases ----------------------------------------------------------------------

const sel = (...ids) => ({ selectedElementIds: Object.fromEntries(ids.map((id) => [id, true])) });
const group = { ...sel("g1", "g2"), selectedGroupIds: { grp: true } };
const groupAndRect = { ...sel("g1", "g2", "r"), selectedGroupIds: { grp: true } };
const cropping = { ...sel("crp"), croppingElementId: "crp" };
const typed = (value) => ({ typed: value });
const drag = (...xs) => ({ drag: xs.map((x) => (typeof x === "number" ? { x, shift: false } : x)) });
const shift = (x) => ({ x, shift: true });

const CASES = [
  // Position
  ["r-x", sel("r"), "X", typed("42.5")],
  ["r-y", sel("r"), "Y", typed("-10")],
  ["rot-x", sel("rot"), "X", typed("250")],
  ["rot-y", sel("rot"), "Y", typed("12.345")],
  ["box-x", sel("box"), "X", typed("30")],
  ["arr-x", sel("arr"), "X", typed("300")],
  ["arr-x-within-threshold", sel("arr"), "X", typed("62")],
  ["f-x", sel("f"), "X", typed("450")],
  ["elb-y", sel("elb"), "Y", typed("650")],
  ["crop-x", cropping, "X", typed("10")],
  ["crop-y", cropping, "Y", typed("500")],
  ["r-x-drag", sel("r"), "X", drag(100, 103, 110, 104)],
  ["r-x-drag-shift", sel("r"), "X", drag(100, shift(107), shift(118))],
  ["rot-y-drag", sel("rot"), "Y", drag(100, 97, 80)],
  ["f-y-drag", sel("f"), "Y", drag(100, 120)],
  ["crop-x-drag", cropping, "X", drag(100, 90, 60)],
  ["crop-y-drag", cropping, "Y", drag(100, 110, 300)],
  // Dimension
  ["r-w", sel("r"), "W", typed("150")],
  ["r-h-min", sel("r"), "H", typed("0.5")],
  ["rot-h", sel("rot"), "H", typed("100")],
  ["img-w", sel("img"), "W", typed("200")],
  ["img-h", sel("img"), "H", typed("40")],
  ["box-w", sel("box"), "W", typed("40")],
  ["t-w", sel("t"), "W", typed("30")],
  ["f-w", sel("f"), "W", typed("400")],
  ["f-h", sel("f"), "H", typed("30")],
  ["ln-w", sel("ln"), "W", typed("50")],
  ["arr-h", sel("arr"), "H", typed("60")],
  ["note-w", sel("note"), "W", typed("300")],
  ["note-h", sel("note"), "H", typed("50")],
  ["crop-w", cropping, "W", typed("50")],
  ["crop-h", cropping, "H", typed("300")],
  ["r-w-drag", sel("r"), "W", drag(100, 120, 90)],
  ["r-h-drag-shift", sel("r"), "H", drag(100, shift(113))],
  ["img-w-drag", sel("img"), "W", drag(100, 133)],
  ["box-h-drag", sel("box"), "H", drag(100, 50)],
  ["f-w-drag", sel("f"), "W", drag(100, 150, 450)],
  ["crop-w-drag", cropping, "W", drag(100, 80, 60)],
  ["crop-h-drag", cropping, "H", drag(100, 140)],
  // Angle
  ["r-a", sel("r"), "A", typed("45")],
  ["box-a", sel("box"), "A", typed("90")],
  ["arr-a", sel("arr"), "A", typed("30")],
  ["elb-a", sel("elb"), "A", typed("30")],
  ["r-a-drag", sel("r"), "A", drag(100, 130)],
  ["r-a-drag-shift", sel("r"), "A", drag(100, shift(112))],
  ["rot-a-drag-negative", sel("rot"), "A", drag(100, 60)],
  // FontSize
  ["t-f", sel("t"), "F", typed("36")],
  ["t-f-min", sel("t"), "F", typed("1")],
  ["box-f", sel("box"), "F", typed("10")],
  ["note-f", sel("note"), "F", typed("40")],
  ["t-f-drag", sel("t"), "F", drag(100, 110)],
  ["t-f-drag-shift", sel("t"), "F", drag(100, shift(105))],
  ["note-f-drag", sel("note"), "F", drag(100, 90)],
  // MultiPosition
  ["multi-x", sel("r", "rot"), "X", typed("100")],
  ["multi-y-group", groupAndRect, "Y", typed("0")],
  ["multi-x-box-label", sel("box", "r"), "X", typed("-20")],
  ["multi-x-drag", sel("r", "rot"), "X", drag(100, 110, 107)],
  ["multi-y-drag-shift-group", groupAndRect, "Y", drag(100, shift(117))],
  // MultiDimension
  ["multi-w", sel("r", "rot"), "W", typed("80")],
  ["multi-w-group", groupAndRect, "W", typed("200")],
  ["multi-h-group", group, "H", typed("10")],
  ["multi-w-frame", sel("f", "r"), "W", typed("400")],
  ["multi-w-box-note", sel("box", "note"), "W", typed("250")],
  ["multi-w-drag", sel("r", "rot"), "W", drag(100, 130)],
  ["multi-h-drag-shift-group", groupAndRect, "H", drag(100, shift(133))],
  ["multi-w-drag-frame", sel("f", "r"), "W", drag(100, 150, 460)],
  // MultiAngle
  ["multi-a", sel("r", "rot"), "A", typed("30")],
  ["multi-a-group", groupAndRect, "A", typed("90")],
  ["multi-a-box-arrow", sel("box", "arr"), "A", typed("15")],
  ["multi-a-drag", sel("r", "rot"), "A", drag(100, 120)],
  ["multi-a-drag-shift", sel("r", "box"), "A", drag(100, shift(140))],
  // MultiFontSize
  ["multi-f", sel("t", "box"), "F", typed("30")],
  ["multi-f-note", sel("t", "note"), "F", typed("50")],
  ["multi-f-drag", sel("t", "box"), "F", drag(100, 107)],
  ["multi-f-drag-shift", sel("t", "note"), "F", drag(100, shift(95))],
];

// -- the harness ----------------------------------------------------------------

const idsOf = (value) => (Array.isArray(value) ? value.map((e) => e.id) : value);

const run = async (up, window, sceneElements, [name, appState, input, gesture]) => {
  up.charWidth.reset();
  const scene = new up.Scene(clone(sceneElements), { skipValidation: true });
  const state = { ...up.getDefaultAppState(), width: 1440, height: 900, ...appState };
  const patches = [];
  const captures = [];
  const app = {
    scene,
    props: {},
    state,
    ownerDocument: window.document,
    ownerWindow: window,
    focusContainer: () => {},
    syncActionResult: (result) => captures.push(result.captureUpdate),
  };
  const setAppState = (u) => {
    const patch = typeof u === "function" ? u(app.state) : u;
    if (!patch) return;
    patches.push(Object.fromEntries(Object.entries(patch).map(([k, v]) => [k, k === "elementsToHighlight" ? idsOf(v) : v])));
    app.state = { ...app.state, ...patch };
  };
  globalThis.__ui = { app, appState: state, setAppState };
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const root = up.createRoot(container);
  await up.act(async () =>
    root.render(up.React.createElement(up.Stats, { app, onClose: () => {}, renderCustomStats: undefined })),
  );
  const field = container.querySelector(`#elementStats [data-testid="${input}"]`);
  if (!field) throw new Error(`${name}: no ${input} input`);
  const act = (fn) => up.act(async () => fn());
  if (gesture.typed !== undefined) {
    const el = field.querySelector("input");
    await act(() => el.focus());
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    await act(() => {
      setter.call(el, gesture.typed);
      el.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
    await act(() => el.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  } else {
    const label = field.querySelector(".drag-input-label");
    const [first, ...rest] = gesture.drag;
    const pointer = (type, target, { x, shift }) =>
      target.dispatchEvent(new window.MouseEvent(type, { bubbles: true, clientX: x, clientY: 0, shiftKey: shift }));
    await act(() => pointer("pointerdown", label, first));
    // DragInput's first pointermove only records where the pointer is
    await act(() => pointer("pointermove", window, first));
    for (const step of rest) await act(() => pointer("pointermove", window, step));
    await act(() => pointer("pointerup", window, rest.at(-1) ?? first));
  }
  await up.act(async () => root.unmount());
  const before = new Map(sceneElements.map((e) => [e.id, JSON.stringify(e)]));
  const after = clone(scene.getElementsIncludingDeleted());
  return {
    name,
    appState,
    input,
    gesture,
    result: {
      order: after.map((e) => e.id),
      changed: after.filter((e) => before.get(e.id) !== JSON.stringify(e)),
      patches,
      captures,
    },
  };
};

export const build = async (upstream) => {
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    patch: PATCH,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (value) => value.length * 10 });
  const scene = buildScene(up);
  const cases = [];
  for (const c of CASES) cases.push(await run(up, window, scene, c));
  window.close();
  return format({
    description:
      "Stats panel edits (ex-539): upstream's Stats over the scene, a value typed and committed with Enter or a label dragged, and what the scene became",
    upstream: upstream.commit,
    scene,
    cases,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`stats-edits: ${error.message}\n`);
    process.exit(1);
  }
  const text = await build(upstream);
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("stats edits fixture is out of date: run node tools/goldens/stats-edits.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`stats edits fixture up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
  process.exit(0);
}
