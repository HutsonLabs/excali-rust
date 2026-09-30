#!/usr/bin/env node
// Stats panel goldens for excali-ui (ex-529): upstream's own Stats
// (packages/excalidraw/components/Stats/index.tsx with Collapsible,
// CanvasGrid, Position, Dimension, Angle, FontSize, MultiPosition,
// MultiDimension, MultiAngle, MultiFontSize and DragInput) over a real
// Scene (packages/element/src/Scene.ts), rendered by React 19.0.0 into
// jsdom 22.1.0, and Stats.scss and DragInput.scss compiled with sass
// 1.51.0.
//
//   node tools/goldens/stats.mjs            write the fixture and CSS
//   node tools/goldens/stats.mjs --check    exit 1 if either is stale
//   node tools/goldens/stats.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/stats.json:
//
// - `locale`: the English strings the panel reads (`stats.*`,
//   `element.*`, `labels.unCroppedDimension`, `labels.imageCropping`);
// - `panels`: STATS_PANELS (common/src/constants.ts);
// - `elements`: the scene every case draws from, built by upstream's
//   newElement functions with fixed ids and seeds;
// - `cases`: per case the scene's element ids, the app state keys set over
//   getDefaultAppState(), the host's gridModeEnabled prop, and the DOM
//   Stats renders once its throttled scene size has settled:
//   `{tag, attrs, style, children}` with attributes and inline style sorted
//   by name, text as a string and an icons.tsx export as `{icon: name}`;
// - `clicks`: per case and control (the close button, each section's
//   header) what the click did: `onClose` called, or the app state patch
//   the collapsible's openTrigger passed to setAppState.
//
// and crates/excali-ui/src/stats/stats.css (Stats.scss and DragInput.scss).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "stats.json");
export const STYLESHEET = join("src", "stats", "stats.css");

const ENTRY = `
export { Stats } from "./packages/excalidraw/components/Stats/index";
export { Scene } from "./packages/element/src/Scene";
export { STATS_PANELS } from "./packages/common/src/constants";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export {
  newElement,
  newArrowElement,
  newImageElement,
  newLinearElement,
  newStickyNoteElement,
  newFreeDrawElement,
} from "./packages/element/src/newElement";
export { reseed } from "./packages/common/src/random";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks; the shim answers them from
// globalThis.__ui.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useExcalidrawAppState: () => globalThis.__ui.appState,
      useExcalidrawSetAppState: () => globalThis.__ui.setAppState,
    };`,
  "packages/excalidraw/editor-jotai": `
    const atom = (init) => ({ init });
    module.exports = {
      atom,
      useSetAtom: () => () => {},
      useAtomValue: (a) => a.init,
      useAtom: (a) => [a.init, () => {}],
      editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
    };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
};

const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: stats.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// -- the scene ------------------------------------------------------------------

// newTextElement measures its text on a canvas; the panel reads only the
// box and the font size, so a rectangle's props with the text fields
const text = (up, id, props, textProps) => ({
  ...up.newElement({ type: "rectangle", id, ...props }),
  type: "text",
  fontSize: 20,
  fontFamily: 5,
  text: "hi",
  textAlign: "left",
  verticalAlign: "top",
  containerId: null,
  originalText: "hi",
  autoResize: true,
  lineHeight: 1.25,
  ...textProps,
});

/** The scene's elements, in scene order. */
const ELEMENTS = (up) => [
  up.newElement({ type: "rectangle", id: "rect", x: 10, y: 20, width: 100, height: 50, seed: 1 }),
  up.newElement({ type: "rectangle", id: "rect2", x: 10, y: 20, width: 100, height: 50, seed: 2 }),
  up.newElement({ type: "rectangle", id: "rotated", x: 200, y: 40, width: 80, height: 60, angle: 0.5, seed: 3 }),
  up.newElement({ type: "rectangle", id: "turned", x: 300, y: 40, width: 80, height: 60, angle: 7, seed: 4 }),
  up.newElement({
    type: "ellipse",
    id: "ellipse",
    x: -30.456,
    y: 12.345,
    width: 40.123,
    height: 30.987,
    angle: 0.1234,
    seed: 5,
  }),
  // newElement leaves isDeleted false whatever it is given
  { ...up.newElement({ type: "diamond", id: "deleted", x: 900, y: 900, width: 60, height: 60, seed: 6 }), isDeleted: true },
  text(up, "text", { x: 0, y: 100, width: 40, height: 25, seed: 7 }, { fontSize: 20.04 }),
  {
    ...up.newElement({ type: "rectangle", id: "box", x: 0, y: 200, width: 120, height: 60, seed: 8 }),
    boundElements: [{ type: "text", id: "label" }],
  },
  text(up, "label", { x: 30, y: 217.5, width: 60, height: 25, seed: 9 }, { containerId: "box", fontSize: 16.33 }),
  { ...up.newElement({ type: "frame", id: "frame", x: 500, y: 0, width: 300, height: 200, seed: 10 }), name: null },
  up.newElement({ type: "rectangle", id: "child", x: 520, y: 20, width: 50, height: 50, seed: 11, frameId: "frame" }),
  up.newElement({ type: "rectangle", id: "g1", x: 0, y: 400, width: 40, height: 40, seed: 12, groupIds: ["group"] }),
  up.newElement({ type: "rectangle", id: "g2", x: 60, y: 420, width: 40, height: 40, seed: 13, groupIds: ["group"] }),
  up.newElement({
    type: "rectangle",
    id: "g3",
    x: 200,
    y: 400,
    width: 30,
    height: 30,
    angle: 0.3,
    seed: 14,
    groupIds: ["inner", "outer"],
  }),
  up.newElement({
    type: "rectangle",
    id: "g4",
    x: 240,
    y: 400,
    width: 30,
    height: 30,
    seed: 15,
    groupIds: ["outer"],
  }),
  up.newImageElement({
    type: "image",
    id: "image",
    x: 0,
    y: 600,
    width: 100,
    height: 80,
    seed: 16,
    status: "saved",
    fileId: "file",
  }),
  up.newImageElement({
    type: "image",
    id: "cropped",
    x: 200,
    y: 600,
    width: 100,
    height: 80,
    seed: 17,
    status: "saved",
    fileId: "file",
    scale: [-1, 1],
    crop: { x: 40, y: 30, width: 200, height: 160, naturalWidth: 400, naturalHeight: 300 },
  }),
  // the linear elements' boxes as the editor leaves them after drawing
  up.newLinearElement({
    type: "line",
    id: "line",
    x: 400,
    y: 600,
    width: 100,
    height: 90,
    seed: 18,
    points: [[0, 0], [100, 40], [30, 90]],
  }),
  up.newArrowElement({
    type: "arrow",
    id: "arrow",
    x: 600,
    y: 600,
    width: 120,
    height: 40,
    seed: 19,
    points: [[0, 0], [120, -40]],
    elbowed: false,
  }),
  up.newFreeDrawElement({
    type: "freedraw",
    id: "draw",
    x: 800,
    y: 600,
    width: 20,
    height: 5,
    seed: 20,
    points: [[0, 0], [5, 5], [20, 3]],
    simulatePressure: true,
  }),
  { ...up.newStickyNoteElement({ type: "stickynote", id: "note", x: 0, y: 800, width: 200, height: 200, seed: 21 }), boundElements: [{ type: "text", id: "noteText" }] },
  text(up, "noteText", { x: 20, y: 880, width: 160, height: 25, seed: 22 }, { containerId: "note", fontSize: 18, baseFontSize: 24 }),
];

const ALL = null;

const sel = (...ids) => ({ selectedElementIds: Object.fromEntries(ids.map((id) => [id, true])) });

/** `{name, elements (ids, ALL for the whole scene), appState,
 * gridModeEnabled (the host prop)}`. */
const CASES = [
  { name: "empty-scene", elements: [] },
  { name: "nothing-selected", elements: ALL },
  { name: "general-collapsed", elements: ALL, appState: { stats: { open: true, panels: 2 }, ...sel("rect") } },
  { name: "element-collapsed", elements: ALL, appState: { stats: { open: true, panels: 1 }, ...sel("rect") } },
  { name: "both-collapsed", elements: ALL, appState: { stats: { open: true, panels: 0 }, ...sel("rect", "rotated") } },
  { name: "grid-mode", elements: ALL, appState: { gridModeEnabled: true, gridStep: 20 } },
  { name: "grid-prop-on", elements: ALL, appState: { gridStep: 35 }, gridModeEnabled: true },
  { name: "grid-prop-off", elements: ALL, appState: { gridModeEnabled: true }, gridModeEnabled: false },
  { name: "rectangle", elements: ALL, appState: sel("rect") },
  { name: "rotated", elements: ALL, appState: sel("rotated") },
  { name: "turned-past-360", elements: ALL, appState: sel("turned") },
  { name: "ellipse-rounding", elements: ALL, appState: sel("ellipse") },
  { name: "text", elements: ALL, appState: sel("text") },
  { name: "container", elements: ALL, appState: sel("box") },
  { name: "container-and-label", elements: ALL, appState: sel("box", "label") },
  { name: "frame", elements: ALL, appState: sel("frame") },
  { name: "image", elements: ALL, appState: sel("image") },
  { name: "cropped-image", elements: ALL, appState: sel("cropped") },
  { name: "cropping", elements: ALL, appState: { ...sel("cropped"), croppingElementId: "cropped" } },
  { name: "cropping-uncropped-image", elements: ALL, appState: { ...sel("image"), croppingElementId: "image" } },
  { name: "cropping-other", elements: ALL, appState: { ...sel("rect"), croppingElementId: "cropped" } },
  { name: "line", elements: ALL, appState: sel("line") },
  { name: "arrow", elements: ALL, appState: sel("arrow") },
  { name: "freedraw", elements: ALL, appState: sel("draw") },
  { name: "sticky-note", elements: ALL, appState: sel("note") },
  { name: "deleted-selected", elements: ALL, appState: sel("deleted") },
  { name: "deleted-and-rect-selected", elements: ALL, appState: sel("deleted", "rect") },
  { name: "multi-same", elements: ALL, appState: sel("rect", "rect2") },
  { name: "multi-mixed", elements: ALL, appState: sel("rect", "rotated", "ellipse") },
  { name: "multi-text", elements: ALL, appState: sel("text", "box", "note") },
  { name: "multi-text-same", elements: ALL, appState: sel("text", "rect") },
  { name: "multi-frame", elements: ALL, appState: sel("frame", "rect") },
  { name: "frame-and-child", elements: ALL, appState: sel("frame", "child") },
  { name: "group", elements: ALL, appState: { ...sel("g1", "g2"), selectedGroupIds: { group: true } } },
  { name: "group-and-rect", elements: ALL, appState: { ...sel("g1", "g2", "rect"), selectedGroupIds: { group: true } } },
  { name: "group-unselected-group", elements: ALL, appState: sel("g1", "g2") },
  { name: "nested-groups", elements: ALL, appState: { ...sel("g3", "g4"), selectedGroupIds: { outer: true } } },
  { name: "nested-inner", elements: ALL, appState: { ...sel("g3", "g4"), selectedGroupIds: { inner: true, outer: true } } },
  { name: "scene-of-one", elements: ["ellipse"], appState: sel("ellipse") },
  { name: "scene-of-linear", elements: ["line", "arrow", "draw"] },
];

// -- DOM ----------------------------------------------------------------------

const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLDivElement: window.HTMLDivElement,
    HTMLInputElement: window.HTMLInputElement,
    SVGElement: window.SVGElement,
    MutationObserver: window.MutationObserver,
    DOMRect: window.DOMRect,
    devicePixelRatio: 1,
    Event: window.Event,
    MouseEvent: window.MouseEvent,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  return window;
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  return sorted(Array.from({ length: style.length }, (_, i) => style.item(i)).map((p) => [p, style.getPropertyValue(p)]));
};

const staticIcons = (icons) =>
  Object.entries(icons).filter(
    ([, value]) => value && value.$$typeof === Symbol.for("react.transitional.element") && value.type === "svg",
  );

const makeTree = (iconNames) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (!name) throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      return { icon: name };
    }
    const attrs = [...node.attributes].filter((a) => a.name !== "style").map((a) => [a.name, a.value]);
    const out = {
      tag: node.localName,
      attrs: sorted(attrs),
      style: styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
    // an input's value is a property React sets, not always an attribute
    if (node.localName === "input") out.value = node.value;
    return out;
  };
  return tree;
};

const wait = (ms) => new Promise((done) => setTimeout(done, ms));

/** The scene's elements as a scene file holds them (no undefined). */
const sceneElements = (up) => {
  up.reseed(7);
  return ELEMENTS(up).map((e) => JSON.parse(JSON.stringify({ ...e, updated: 1, created: 1 })));
};

const mount = async (up, window, all, c) => {
  const elements = c.elements === ALL ? all : all.filter((e) => c.elements.includes(e.id));
  const scene = new up.Scene(elements.map((e) => ({ ...e })));
  let state = { ...up.getDefaultAppState(), width: 1440, height: 900, ...(c.appState ?? {}) };
  const patches = [];
  let closed = 0;
  const app = {
    scene,
    props: c.gridModeEnabled === undefined ? {} : { gridModeEnabled: c.gridModeEnabled },
    state,
    ownerDocument: window.document,
    ownerWindow: window,
    focusContainer: () => {},
  };
  globalThis.__ui = {
    app,
    appState: state,
    setAppState: (u) => {
      const patch = typeof u === "function" ? u(state) : u;
      patches.push(patch);
      if (patch) state = { ...state, ...patch };
    },
  };
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const root = up.createRoot(container);
  await up.act(async () =>
    root.render(
      up.React.createElement(up.Stats, {
        app,
        onClose: () => closed++,
        renderCustomStats: undefined,
      }),
    ),
  );
  // the scene size is set by a throttled callback (STATS_TIMEOUT, 50ms)
  await up.act(async () => wait(80));
  return { container, root, patches, closed: () => closed, elements };
};

const render = async (up, window, iconNames, all, c) => {
  const m = await mount(up, window, all, c);
  const dom = [...m.container.childNodes].map(makeTree(iconNames));
  await up.act(async () => m.root.unmount());
  return {
    name: c.name,
    elements: m.elements.map((e) => e.id),
    appState: c.appState ?? {},
    gridModeEnabled: c.gridModeEnabled ?? null,
    dom,
  };
};

// -- clicks -------------------------------------------------------------------

const CONTROLS = [
  { control: "close", selector: ".exc-stats .title .close" },
  { control: "generalStats", selector: ".exc-stats .title + div" },
  { control: "elementProperties", selector: "#elementStats > div" },
];

const CLICK_CASES = [
  { name: "default", elements: ALL, appState: sel("rect") },
  { name: "general-collapsed", elements: ALL, appState: { stats: { open: true, panels: 2 }, ...sel("rect") } },
  { name: "element-collapsed", elements: ALL, appState: { stats: { open: true, panels: 1 }, ...sel("rect") } },
  { name: "both-collapsed", elements: ALL, appState: { stats: { open: true, panels: 0 }, ...sel("rect") } },
  { name: "closed-panels", elements: ALL, appState: { stats: { open: false, panels: 3 }, ...sel("rect") } },
];

const clicks = async (up, window, all) => {
  const out = [];
  for (const c of CLICK_CASES) {
    for (const { control, selector } of CONTROLS) {
      const m = await mount(up, window, all, c);
      const target = m.container.querySelector(selector);
      if (!target) throw new Error(`${c.name}: no ${selector}`);
      await up.act(async () => target.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
      await up.act(async () => m.root.unmount());
      out.push({
        case: c.name,
        control,
        appState: c.appState,
        closed: m.closed(),
        patches: m.patches,
      });
    }
  }
  return out;
};

// -- stylesheet -----------------------------------------------------------------

const STYLESHEETS = ["components/Stats/Stats.scss", "components/Stats/DragInput.scss"];

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const css = STYLESHEETS.map(
    (p) => sass.compile(join(root, p), { loadPaths: [root], style: "expanded" }).css,
  ).join("\n\n");
  return (
    "/* Generated by tools/goldens/stats.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded). */\n" +
    `${css}\n`
  );
};

// -- main -----------------------------------------------------------------------

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const iconNames = new Map();
  for (const [name, value] of staticIcons(up.icons)) {
    const host = window.document.createElement("div");
    const root = up.createRoot(host);
    await up.act(async () => root.render(value));
    const markup = host.innerHTML;
    await up.act(async () => root.unmount());
    if (iconNames.has(markup)) continue;
    iconNames.set(markup, name);
  }
  const all = sceneElements(up);
  const cases = [];
  for (const c of CASES) cases.push(await render(up, window, iconNames, all, c));
  const clickCases = await clicks(up, window, all);
  window.close();
  const en = JSON.parse(readFileSync(join(upstream.dir, "packages", "excalidraw", "locales", "en.json"), "utf8"));
  const keys = [
    ...Object.keys(en.stats).map((k) => `stats.${k}`),
    ...Object.keys(en.element).map((k) => `element.${k}`),
    "labels.unCroppedDimension",
    "labels.imageCropping",
  ];
  const fixture = {
    upstream: upstream.commit,
    locale: Object.fromEntries(keys.map((k) => [k, up.t(k)])),
    panels: up.STATS_PANELS,
    elements: all,
    cases,
    clicks: clickCases,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`stats: ${error.message}\n`);
    process.exit(1);
  }
  const files = await build(upstream);
  const dir = args.out ?? OUT_DIR;
  if (args.check) {
    const stale = Object.entries(files).filter(([f, text]) => {
      const path = join(dir, f);
      return !existsSync(path) || readFileSync(path, "utf8") !== text;
    });
    if (stale.length) {
      for (const [f] of stale) process.stderr.write(`stale: ${relative(process.cwd(), join(dir, f))}\n`);
      process.stderr.write("stats goldens are out of date: run node tools/goldens/stats.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`stats goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, text] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
    process.stdout.write(`wrote ${relative(process.cwd(), path)} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
  process.exit(0);
}
