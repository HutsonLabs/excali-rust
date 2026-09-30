#!/usr/bin/env node
// Convert element type popup goldens for excali-ui (ex-535): upstream's own
// ConvertElementTypePopup (packages/excalidraw/components/
// ConvertElementTypePopup.tsx, buttons from IconButton.tsx) rendered by
// React 19.0.0 into jsdom 22.1.0, and its stylesheet compiled from
// upstream's SCSS with sass 1.51.0.
//
//   node tools/goldens/convert-popup.mjs            write the fixture and CSS
//   node tools/goldens/convert-popup.mjs --check    exit 1 if either is stale
//   node tools/goldens/convert-popup.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/convert-popup.json:
//
// - `cases`: per case the scene (`elements`), the selection, the app state
//   keys over the defaults (scroll, zoom, offsets), and the DOM React
//   leaves in the editor container, as tools/goldens/toolbar.mjs records it
//   (`{tag, attrs, style, children}`, an icon's <svg> as `{icon: name}`);
//
// and crates/excali-ui/src/convert_popup/convert_popup.css:
// ConvertElementTypePopup.scss compiled (expanded).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { installDom, makeTree, staticIcons } from "./toolbar.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "convert-popup.json");
export const STYLESHEET = join("src", "convert_popup", "convert_popup.css");

const SCSS = "components/ConvertElementTypePopup.scss";

const ENTRY = `
export { default as ConvertElementTypePopup } from "./packages/excalidraw/components/ConvertElementTypePopup";
export * as icons from "./packages/excalidraw/components/icons";
export { Scene } from "./packages/element/src/Scene";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { reseed } from "./packages/common/src/random";
export {
  ROUNDNESS,
  DEFAULT_VERTICAL_ALIGN,
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
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

const SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
  "packages/excalidraw/analytics": "module.exports = { trackEvent: () => {} };",
};

const usage = () => {
  process.stderr.write("usage: convert-popup.mjs [--check] [--out DIR]\n");
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

const clone = (value) => (value === undefined ? null : JSON.parse(JSON.stringify(value)));

const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

/** { name, elements(up), selected, appState } */
const CASES = [
  { name: "rectangle", elements: (up) => [el(up, { type: "rectangle", id: "r", x: 10, y: 20, width: 100, height: 60 })], selected: ["r"] },
  { name: "diamond", elements: (up) => [el(up, { type: "diamond", id: "d", x: 10, y: 20, width: 100, height: 60 })], selected: ["d"] },
  {
    name: "ellipse-scrolled",
    elements: (up) => [el(up, { type: "ellipse", id: "e", x: -40, y: 5, width: 80, height: 80 })],
    selected: ["e"],
    appState: { scrollX: 30, scrollY: -12.5, zoom: { value: 1.5 }, offsetLeft: 20, offsetTop: 40 },
  },
  {
    name: "rotated",
    elements: (up) => [el(up, { type: "rectangle", id: "r", x: 100, y: 100, width: 120, height: 40, angle: 0.5 })],
    selected: ["r"],
  },
  {
    name: "mixed-generic",
    elements: (up) => [
      el(up, { type: "rectangle", id: "r", x: 0, y: 0 }),
      el(up, { type: "ellipse", id: "e", x: 150, y: 40, width: 60, height: 90 }),
    ],
    selected: ["r", "e"],
    appState: { zoom: { value: 0.5 } },
  },
  {
    name: "generic-with-line",
    elements: (up) => [
      el(up, { type: "diamond", id: "d", x: 0, y: 0 }),
      el(up, { type: "line", id: "l", x: 200, y: 0, points: [[0, 0], [50, 250]] }),
    ],
    selected: ["d", "l"],
  },
  { name: "line", elements: (up) => [el(up, { type: "line", id: "l", x: 10, y: 20, points: [[0, 0], [100, 50]] })], selected: ["l"] },
  {
    name: "sharp-arrow",
    elements: (up) => [el(up, { type: "arrow", id: "a", x: 10, y: 20, roundness: false, points: [[0, 0], [100, 50]] })],
    selected: ["a"],
  },
  {
    name: "curved-arrow",
    elements: (up) => [el(up, { type: "arrow", id: "a", x: 10, y: 20, roundness: true, points: [[0, 0], [100, 50]] })],
    selected: ["a"],
  },
  {
    name: "elbow-arrow",
    elements: (up) => [el(up, { type: "arrow", id: "a", x: 10, y: 20, elbowed: true, roundness: false, points: [[0, 0], [100, 0], [100, 50]] })],
    selected: ["a"],
  },
  {
    name: "two-lines",
    elements: (up) => [
      el(up, { type: "line", id: "l", x: 10, y: 20, points: [[0, 0], [100, 50]] }),
      el(up, { type: "line", id: "m", x: 200, y: 80, points: [[0, 0], [-30, 90]] }),
    ],
    selected: ["l", "m"],
  },
  {
    name: "line-and-arrow",
    elements: (up) => [
      el(up, { type: "line", id: "l", x: 10, y: 20, points: [[0, 0], [100, 50]] }),
      el(up, { type: "arrow", id: "a", x: 200, y: 80, points: [[0, 0], [-30, 90]] }),
    ],
    selected: ["l", "a"],
  },
];

const render = async (up, window, iconNames, c) => {
  const { React, act, createRoot, ConvertElementTypePopup } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  up.reseed(1);
  const elements = c.elements(up);
  const scene = new up.Scene(elements, { skipValidation: true });
  // App measures the container's offsets (getDefaultAppState leaves them out)
  const state = {
    ...up.getDefaultAppState(),
    offsetLeft: 0,
    offsetTop: 0,
    ...(c.appState ?? {}),
    selectedElementIds: Object.fromEntries(c.selected.map((id) => [id, true])),
  };
  const closed = [];
  const app = { scene, state, updateEditorAtom: (atom, value) => closed.push(value) };
  const root = createRoot(container);
  await act(async () => root.render(React.createElement(ConvertElementTypePopup, { app })));
  if (closed.length) throw new Error(`${c.name}: the popup closed itself`);
  const tree = makeTree(iconNames, new Map(), {});
  const dom = [...container.childNodes].map(tree);
  await act(async () => root.unmount());
  return {
    name: c.name,
    elements: scene.getElementsIncludingDeleted().map(clone),
    selected: c.selected,
    appState: c.appState ?? {},
    dom,
  };
};

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString(`@use "${SCSS.replace(/\.scss$/, "")}";\n`, {
    loadPaths: [root],
    style: "expanded",
  }).css;
  return (
    "/* Generated by tools/goldens/convert-popup.mjs; do not edit. Upstream's\n" +
    ` * packages/excalidraw/${SCSS} at the pin, compiled with sass 1.51.0\n` +
    " * (expanded). */\n" +
    `${compiled}\n`
  );
};

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    jsx: "automatic",
    // test mode: timestamps 1, as upstream's tests (the scene is recorded)
    define: { "import.meta.env.MODE": '"test"' },
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
  const cases = [];
  for (const c of CASES) cases.push(await render(up, window, iconNames, c));
  window.close();
  const fixture = { upstream: upstream.commit, cases };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`convert-popup: ${error.message}\n`);
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
      process.stderr.write("convert popup goldens are out of date: run node tools/goldens/convert-popup.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`convert popup goldens up to date: ${Object.keys(files).length} files\n`);
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
  // React's act() keeps MessageChannel ports open (see toolbar.mjs)
  process.exit(0);
}
