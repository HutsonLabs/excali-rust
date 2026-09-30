#!/usr/bin/env node
// Search menu goldens for excali-ui (ex-708): upstream's own SearchMenu
// (packages/excalidraw/components/SearchMenu.tsx), the search tab of the
// default sidebar, rendered by React 19.0.0 into jsdom 22.1.0 inside a
// stand-in App that holds the app state and the scene, its module-private
// search helpers called directly, actionToggleSearchMenu's perform
// (actions/actionToggleSearchMenu.ts, Ctrl/Cmd+F) and SearchMenu.scss
// compiled with sass 1.51.0.
//
//   node tools/goldens/search-menu.mjs            write the fixture and CSS
//   node tools/goldens/search-menu.mjs --check    exit 1 if either is stale
//   node tools/goldens/search-menu.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/search-menu.json:
//
// - `locale`: the English strings the menu reads (locales/en.json);
// - `scene`: the elements every case searches (texts, wrapped, aligned and
//   bound texts, a deleted text, frames with and without a name, an AI
//   frame and a rectangle);
// - `units`: getMatchPreview (text, index, query), normalizeWrappedText
//   (text, originalText), getMatchedLines (element id, query, index) and
//   getMatchInFrame (element id, query, index, zoom), each with upstream's
//   result;
// - `toggle`: actionToggleSearchMenu.perform on app states (the sidebar
//   closed, on the library tab, on the search tab, a dialog open): the
//   result (false, or the app state keys it sets) and whether it focused
//   and selected the search input;
// - `interactions`: per interaction the case (`zoom`, `visible`: the ids
//   app.visibleElements holds, `query`: searchQueryAtom's value at mount,
//   `openDialog`) and steps: `mount`, `debounce` (the pending debounced
//   search runs), `input` (a React change with `value`), `click` (a CSS
//   selector and its index), `keydown` (`key`, `ctrlKey`, on the input or
//   the document), `blur` and `unmount`; per step what the handlers did, in order
//   (`effects`: `setAppState` with the keys it set, `setViewport` with the
//   target's bounds, fit, animation and offsets, `atom` (an editor atom
//   set, by name and value) and `scrollIntoView` (the result item's
//   text)), `setViewport`'s target being getCommonBounds' [x1, y1, x2, y2], whether the default was prevented, whether the search input
//   has the focus (and its selection), and the DOM React leaves: `{tag,
//   attrs, style, children}` with attributes and inline style sorted by
//   name, text as a string, an icon's <svg> as `{icon: name}`.
//
// and crates/excali-ui/src/search_menu/search_menu.css: SearchMenu.scss
// compiled (expanded).
//
// Text is measured through upstream's setCustomTextMetricsProvider with the
// `scaled` metric of text-element-sizing.mjs (restated in the Rust test),
// so the font size and the characters reach every offset. The debounce
// (lodash.debounce, 350 ms) is a slot the `debounce` step runs.
// app.viewport.getOffsets() answers the case's `offsets`; setViewport is
// recorded, not run.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { METRICS } from "./text-element-sizing.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "search-menu.json");
export const STYLESHEET = join("src", "search_menu", "search_menu.css");

const SEARCH_MENU = "packages/excalidraw/components/SearchMenu";

const ENTRY = `
export {
  SearchMenu,
  searchItemInFocusAtom,
  searchQueryAtom,
  getMatchPreview,
  normalizeWrappedText,
  getMatchedLines,
  getMatchInFrame,
} from "./${SEARCH_MENU}";
export { actionToggleSearchMenu } from "./packages/excalidraw/actions/actionToggleSearchMenu";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// The editor's atoms: a store the stand-in App resets per interaction,
// whose sets are recorded (by the atom's name, globalThis.__ui.atomNames).
const JOTAI = `
    const React = require("react");
    const atom = (init) => ({ init });
    const store = () => globalThis.__jotai;
    const get = (a) => (store().values.has(a) ? store().values.get(a) : a.init);
    const set = (a, v) => {
      const next = typeof v === "function" ? v(get(a)) : v;
      store().values.set(a, next);
      const name = globalThis.__ui.atomNames.get(a);
      if (name) globalThis.__ui.effect({ atom: name, value: next });
      for (const l of [...store().listeners]) l();
    };
    const useSubscribe = () => {
      const [, force] = React.useReducer((x) => x + 1, 0);
      React.useEffect(() => {
        store().listeners.add(force);
        return () => store().listeners.delete(force);
      }, []);
    };
    const useAtomValue = (a) => { useSubscribe(); return get(a); };
    const useSetAtom = (a) => React.useCallback((v) => set(a, v), [a]);
    const useAtom = (a) => [useAtomValue(a), useSetAtom(a)];
    module.exports = {
      atom,
      useAtom,
      useAtomValue,
      useSetAtom,
      editorJotaiStore: { get, set, sub: () => () => {} },
    };`;

const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useExcalidrawSetAppState: () => globalThis.__ui.setAppState,
    };`,
  "packages/excalidraw/editor-jotai": JOTAI,
  // the debounced search waits in a slot the "debounce" step runs
  "lodash.debounce": `
    module.exports = (fn) => (...args) => {
      globalThis.__ui.pending = () => fn(...args);
    };`,
};

const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const LOCALE_KEYS = [
  "search.title",
  "search.noMatch",
  "search.singleResult",
  "search.multipleResults",
  "search.placeholder",
  "search.frames",
  "search.texts",
];

const usage = () => {
  process.stderr.write("usage: search-menu.mjs [--check] [--out DIR]\n");
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

const json = (value) => JSON.parse(JSON.stringify(value));

// -- DOM ----------------------------------------------------------------------

const written = new WeakMap();

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
    HTMLInputElement: window.HTMLInputElement,
    SVGElement: window.SVGElement,
    MutationObserver: window.MutationObserver,
    Event: window.Event,
    KeyboardEvent: window.KeyboardEvent,
    MouseEvent: window.MouseEvent,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  // the highlighted result scrolls itself into view (jsdom has no layout)
  window.Element.prototype.scrollIntoView = function (opts) {
    globalThis.__ui?.effect({ scrollIntoView: { text: this.textContent, ...opts } });
  };
  const proto = window.CSSStyleDeclaration.prototype;
  const hyphenate = (p) => p.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`);
  const note = (style, property, value) => {
    if (!written.has(style)) written.set(style, new Map());
    const map = written.get(style);
    if (value === null || value === undefined || value === "") map.delete(property);
    else map.set(property, String(value));
  };
  for (const name of Object.getOwnPropertyNames(proto)) {
    const desc = Object.getOwnPropertyDescriptor(proto, name);
    if (!desc?.set || ["cssText", "cssFloat", "length", "parentRule"].includes(name)) continue;
    Object.defineProperty(proto, name, {
      ...desc,
      set(value) {
        desc.set.call(this, value);
        note(this, hyphenate(name), value);
      },
    });
  }
  const setProperty = proto.setProperty;
  proto.setProperty = function (property, value, priority) {
    setProperty.call(this, property, value, priority);
    note(this, property, value);
  };
  return window;
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  const map = written.get(style);
  if (map) return sorted(map.entries());
  if (style.length) throw new Error(`a style nobody wrote through the recorder: ${node.outerHTML.slice(0, 120)}`);
  return {};
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
      if (name) return { icon: name };
      throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
    }
    const attrs = [...node.attributes].filter((a) => a.name !== "style").map((a) => [a.name, a.value]);
    return {
      tag: node.localName,
      attrs: sorted(attrs),
      style: styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

// -- scene --------------------------------------------------------------------

const base = (id, type, x, y, width, height, rest = {}) => ({
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
  index: null,
  roundness: null,
  seed: 1,
  version: 1,
  versionNonce: 0,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
  ...rest,
});

const text = (id, x, y, width, height, t, rest = {}) =>
  base(id, "text", x, y, width, height, {
    text: t,
    originalText: t,
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    autoResize: true,
    lineHeight: 1.25,
    ...rest,
  });

const frame = (id, type, x, y, width, height, name) => base(id, type, x, y, width, height, { name });

// y order: f2 (-100), t2 (0), t4 (50), t1 (100), t6 (150), t3 (300), f1
// (400), f3 (500); the array order differs so the sort is exercised.
const SCENE = [
  base("r1", "rectangle", 0, 0, 100, 50, { boundElements: [{ type: "text", id: "t4" }] }),
  text("t1", 20, 100, 230, 25, "Hello world, hello again"),
  text("t3", 400, 300, 120, 50, "alpha\nbeta hello", { textAlign: "right", fontSize: 16, fontFamily: 1 }),
  text("t2", 0, 0, 200, 75, "The quick brown\nfox jumps over\nthe lazy dog", {
    originalText: "The quick brown fox jumps over the lazy dog",
    textAlign: "center",
    autoResize: false,
  }),
  text("t4", 5, 50, 90, 25, "hello box", { containerId: "r1", textAlign: "center", verticalAlign: "middle" }),
  text("t5", 0, 200, 60, 25, "hello deleted", { isDeleted: true }),
  text("t6", 600, 150, 300, 25, "one two three four five six seven eight nine ten hello eleven twelve thirteen fourteen", {
    fontSize: 36,
    fontFamily: 6,
  }),
  frame("f1", "frame", 0, 400, 60, 100, "Hello frame"),
  frame("f2", "frame", 300, -100, 400, 200, null),
  frame("f3", "magicframe", 800, 500, 200, 200, null),
  text("t7", 50, 700, 200, 25, "a.b*c (x) [y] a-b A.B", { fontFamily: 8 }),
];

const byId = (id) => SCENE.find((e) => e.id === id);

// -- units ------------------------------------------------------------------------

const PREVIEWS = [
  ["small", 1, "mall"],
  ["small", 0, "smal"],
  ["small", 0, "small"],
  ["one two three four five six seven eight nine ten", 20, "six"],
  ["one two three four five six seven eight nine ten", 19, " six"],
  ["one two three four five six seven eight", 0, "one"],
  ["one two three four five six seven eight", 34, "eight"],
  ["averyveryverylongwordthatgoesonandon hello", 36, " hello"],
  ["averyveryverylongwordthatgoesonandon hello", 37, "hello"],
  ["x averyveryverylongwordthatgoesonandonandon hello", 47, "hello"],
  ["tabs\tand\nnewlines  here and there", 17, "here"],
  ["Hello world, hello again", 13, "hello"],
  ["héllo wörld ünïcode ✓ done", 12, "ünï"],
  ["emoji 😀 smile and more words after this", 9, "smile"],
];

const NORMALIZE = [
  ["The quick brown\nfox jumps over\nthe lazy dog", "The quick brown fox jumps over the lazy dog"],
  ["one\ntwo", "one   two"],
  ["alpha\nbeta", "alpha\nbeta"],
  ["a\n\nb", "a\n\nb"],
  ["longword\nsplit", "longwordsplit"],
  ["same", "same"],
];

const MATCHED_LINES = [
  ["t1", "hello", 0],
  ["t1", "hello", 13],
  ["t1", "o w", 4],
  ["t2", "quick", 4],
  ["t2", "brown fox", 10],
  ["t2", "over the lazy", 26],
  ["t2", "dog", 40],
  ["t3", "hello", 11],
  ["t3", "alpha\nbeta", 0],
  ["t4", "box", 6],
  ["t6", "hello", 49],
];

const FRAME_MATCHES = [
  ["f1", "hello", 0, 1],
  ["f1", "frame", 6, 1],
  ["f1", "frame", 6, 2],
  ["f2", "fra", 0, 1],
  ["f3", "frame", 3, 0.5],
];

// -- toggle ---------------------------------------------------------------------

const TOGGLE = [
  { name: "closed", openSidebar: null, openDialog: null },
  { name: "library-tab", openSidebar: { name: "default", tab: "library" }, openDialog: null },
  { name: "search-tab", openSidebar: { name: "default", tab: "search" }, openDialog: null },
  { name: "other-sidebar", openSidebar: { name: "custom", tab: "search" }, openDialog: null },
  { name: "dialog", openSidebar: null, openDialog: { name: "help" } },
  { name: "dialog-search-tab", openSidebar: { name: "default", tab: "search" }, openDialog: { name: "help" } },
];

const toggle = (up, c) => {
  const effects = [];
  const input = {
    focus: () => effects.push("focus"),
    select: () => effects.push("select"),
  };
  const app = {
    excalidrawContainerValue: {
      container: {
        querySelector: (selector) => {
          effects.push(`querySelector ${selector}`);
          return input;
        },
      },
    },
  };
  const appState = { openSidebar: c.openSidebar, openDialog: c.openDialog, theme: "light" };
  const result = up.actionToggleSearchMenu.perform([], appState, null, app);
  return {
    ...c,
    result:
      result === false
        ? false
        : { appState: { openSidebar: result.appState.openSidebar, openDialog: result.appState.openDialog }, captureUpdate: result.captureUpdate },
    effects,
  };
};

// -- interactions ---------------------------------------------------------------

const INPUT = ".layer-ui__search-header input";
const ITEM = ".layer-ui__result-item";
const NAV = ".result-nav-btn";

const DEFAULT_CASE = {
  zoom: 1,
  scrollX: 0,
  scrollY: 0,
  offsetLeft: 0,
  offsetTop: 0,
  canvas: [1000, 800],
  offsets: { top: 24, right: 326, bottom: 24, left: 24 },
  visible: ["t1", "t4", "r1", "f1"],
  query: "",
  openDialog: null,
  openPopup: null,
};

/**
 * [name, case, steps]; a step is [event, extra]: `debounce`, `input`
 * (value), `click` ([selector, index]), `keydown` ({ key, ctrlKey, on:
 * "input" | "document" }), `blur` (the input loses the focus), `unmount`. Every interaction starts with the
 * `mount` step, then `debounce`.
 */
const INTERACTIONS = [
  ["empty", {}, []],
  [
    "type-and-search",
    {},
    [
      ["input", "hello"],
      ["debounce"],
      ["input", "  HELLO  "],
      ["debounce"],
      ["input", "zzz"],
      ["debounce"],
      ["input", ""],
      ["debounce"],
    ],
  ],
  [
    "navigate",
    {},
    [
      ["input", "hello"],
      ["debounce"],
      ["keydown", { key: "Enter", on: "input" }],
      ["keydown", { key: "ArrowDown", on: "input" }],
      ["keydown", { key: "ArrowUp", on: "input" }],
      ["keydown", { key: "ArrowUp", on: "input" }],
      ["keydown", { key: "ArrowUp", on: "input" }],
      ["click", [NAV, 0]],
      ["click", [NAV, 1]],
      ["click", [ITEM, 3]],
      ["click", [ITEM, 0]],
      ["keydown", { key: "Enter", on: "document" }],
    ],
  ],
  [
    "none-visible",
    { visible: [] },
    [
      ["input", "hello"],
      ["debounce"],
      ["keydown", { key: "ArrowUp", on: "input" }],
      ["keydown", { key: "ArrowDown", on: "input" }],
    ],
  ],
  [
    "none-visible-next",
    { visible: [] },
    [
      ["input", "hello"],
      ["debounce"],
      ["keydown", { key: "Enter", on: "input" }],
    ],
  ],
  ["frames", { visible: ["f3"], zoom: 0.5 }, [["input", "frame"], ["debounce"], ["click", [ITEM, 0]]]],
  ["frames-and-texts", {}, [["input", "o"], ["debounce"], ["click", [ITEM, 4]]]],
  ["special-characters", {}, [["input", "a.b"], ["debounce"], ["input", "(x) [y]"], ["debounce"], ["input", "a-b"], ["debounce"]]],
  ["single", { visible: ["t2"] }, [["input", "lazy"], ["debounce"], ["keydown", { key: "Enter", on: "input" }]]],
  [
    "tiny-text",
    { zoom: 0.25, visible: ["t3"] },
    [["input", "beta"], ["debounce"], ["keydown", { key: "Enter", on: "input" }]],
  ],
  [
    "offscreen",
    { scrollX: -2000, visible: [] },
    [["input", "quick"], ["debounce"], ["keydown", { key: "Enter", on: "input" }]],
  ],
  ["reopen", { query: "hello" }, [["keydown", { key: "ArrowDown", on: "input" }]]],
  ["pending-before-debounce", {}, [["input", "hel"], ["input", "hello"], ["debounce"]]],
  [
    "escape",
    {},
    [
      ["input", "hello"],
      ["debounce"],
      ["keydown", { key: "Escape", on: "document" }],
    ],
  ],
  ["escape-dialog", { openDialog: { name: "help" } }, [["keydown", { key: "Escape", on: "input" }]]],
  ["escape-popup", { openPopup: "fontFamily" }, [["keydown", { key: "Escape", on: "input" }]]],
  [
    "ctrl-f",
    {},
    [
      ["blur"],
      ["keydown", { key: "f", ctrlKey: true, on: "document" }],
      ["input", "hello"],
      ["blur"],
      ["keydown", { key: "f", ctrlKey: true, on: "document" }],
      ["keydown", { key: "f", ctrlKey: true, on: "input" }],
    ],
  ],
  ["ctrl-f-dialog", { openDialog: { name: "help" } }, [["keydown", { key: "f", ctrlKey: true, on: "document" }]]],
  ["unmount", {}, [["input", "hello"], ["debounce"], ["click", [ITEM, 1]], ["unmount"]]],
];

const interact = async (up, window, iconNames, [name, raw, steps]) => {
  const c = { ...DEFAULT_CASE, ...raw };
  const { React, act, createRoot, SearchMenu } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);

  const effects = [];
  const effect = (e) => effects.push(json(e));
  globalThis.__jotai = { values: new Map(), listeners: new Set() };
  if (c.query) globalThis.__jotai.values.set(up.searchQueryAtom, c.query);

  let state = {
    zoom: { value: c.zoom },
    scrollX: c.scrollX,
    scrollY: c.scrollY,
    offsetLeft: c.offsetLeft,
    offsetTop: c.offsetTop,
    openDialog: c.openDialog,
    openPopup: c.openPopup,
    openSidebar: { name: "default", tab: "search" },
    searchMatches: null,
  };
  const setAppState = (patch) => {
    const next = typeof patch === "function" ? patch(state) : patch;
    if (next === null) return;
    const keys = Object.keys(next).filter((k) => typeof patch !== "function" || next[k] !== state[k]);
    effect({ setAppState: Object.fromEntries(keys.map((k) => [k, next[k]])) });
    state = { ...state, ...next };
  };
  const live = SCENE.filter((e) => !e.isDeleted);
  const elementsMap = new Map(live.map((e) => [e.id, e]));
  const app = {
    get state() {
      return state;
    },
    scene: {
      getNonDeletedElementsMap: () => elementsMap,
      getNonDeletedElements: () => live,
      getSceneNonce: () => 1,
    },
    visibleElements: live.filter((e) => c.visible.includes(e.id)),
    canvas: { width: c.canvas[0], height: c.canvas[1] },
    ownerWindow: window,
    viewport: {
      getOffsets: () => c.offsets,
      setViewport: (opts) =>
        effect({
          setViewport: {
            target: opts.target,
            fit: opts.fit,
            animation: opts.animation,
            offsets: opts.offsets,
          },
        }),
    },
  };
  globalThis.__ui = {
    app,
    setAppState,
    effect,
    pending: null,
    atomNames: new Map([
      [up.searchItemInFocusAtom, "searchItemInFocus"],
      [up.searchQueryAtom, "searchQuery"],
    ]),
  };

  const root = createRoot(container);
  const tree = makeTree(iconNames);
  const snapshot = () => [...container.childNodes].map(tree);
  const focus = () => {
    const input = container.querySelector(INPUT);
    const active = document.activeElement === input && input !== null;
    return active ? { focused: true, selection: [input.selectionStart, input.selectionEnd] } : { focused: false };
  };
  const settle = async () => {
    for (let i = 0; i < 3; i++) await act(async () => {});
  };

  const out = [];
  const record = (event, extra, prevented) =>
    out.push({ event, ...extra, effects: [...effects], defaultPrevented: prevented, ...focus(), dom: snapshot() });

  await act(async () => root.render(React.createElement(SearchMenu)));
  await settle();
  record("mount", {}, false);
  effects.length = 0;
  const debounce = async () => {
    const pending = globalThis.__ui.pending;
    globalThis.__ui.pending = null;
    if (pending) await act(async () => pending());
    await settle();
  };
  await debounce();
  record("debounce", { ran: true }, false);

  for (const [event, extra] of steps) {
    effects.length = 0;
    let prevented = false;
    if (event === "debounce") {
      const ran = globalThis.__ui.pending !== null;
      await debounce();
      record("debounce", { ran }, false);
      continue;
    }
    if (event === "blur") {
      await act(async () => container.querySelector(INPUT).blur());
      record("blur", {}, false);
      continue;
    }
    if (event === "unmount") {
      await act(async () => root.unmount());
      out.push({ event, effects: [...effects], defaultPrevented: false, focused: false, dom: [] });
      continue;
    }
    await act(async () => {
      let e;
      let target;
      if (event === "input") {
        target = container.querySelector(INPUT);
        const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
        setter.call(target, extra);
        e = new window.Event("input", { bubbles: true, cancelable: true });
      } else if (event === "click") {
        target = container.querySelectorAll(extra[0])[extra[1]];
        e = new window.MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
      } else if (event === "keydown") {
        target = extra.on === "input" ? container.querySelector(INPUT) : document.body;
        e = new window.KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: extra.key, ctrlKey: !!extra.ctrlKey });
      } else {
        throw new Error(`event ${event}`);
      }
      if (!target) throw new Error(`${name}: no target for ${event} ${JSON.stringify(extra)}`);
      target.dispatchEvent(e);
      prevented = e.defaultPrevented;
    });
    await settle();
    const detail =
      event === "input"
        ? { value: extra }
        : event === "click"
          ? { selector: extra[0], index: extra[1] }
          : { key: extra.key, ctrlKey: !!extra.ctrlKey, on: extra.on };
    record(event, detail, prevented);
  }
  if (steps.at(-1)?.[0] !== "unmount") await act(async () => root.unmount());
  return {
    name,
    zoom: c.zoom,
    scrollX: c.scrollX,
    scrollY: c.scrollY,
    offsetLeft: c.offsetLeft,
    offsetTop: c.offsetTop,
    canvas: c.canvas,
    offsets: c.offsets,
    visible: c.visible,
    query: c.query,
    openDialog: c.openDialog,
    openPopup: c.openPopup,
    steps: out,
  };
};

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString('@use "components/SearchMenu";\n', { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/search-menu.mjs; do not edit. Upstream's\n" +
    " * packages/excalidraw/components/SearchMenu.scss at the pin, compiled\n" +
    " * with sass 1.51.0 (expanded). */\n" +
    `${compiled}\n`
  );
};

// -- build ------------------------------------------------------------------------

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    expose: {
      [SEARCH_MENU]: ["searchQueryAtom", "getMatchPreview", "normalizeWrappedText", "getMatchedLines", "getMatchInFrame"],
    },
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.DEV": "false",
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: METRICS.scaled });
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
  const units = {
    preview: PREVIEWS.map(([t, index, query]) => ({ text: t, index, query, result: up.getMatchPreview(t, index, query) })),
    normalize: NORMALIZE.map(([wrapped, original]) => ({ text: wrapped, originalText: original, result: up.normalizeWrappedText(wrapped, original) })),
    matchedLines: MATCHED_LINES.map(([id, query, index]) => ({ id, query, index, result: up.getMatchedLines(byId(id), query, index) })),
    frame: FRAME_MATCHES.map(([id, query, index, zoom]) => ({ id, query, index, zoom, result: up.getMatchInFrame(byId(id), query, index, zoom) })),
  };
  const interactions = [];
  for (const i of INTERACTIONS) interactions.push(await interact(up, window, iconNames, i));
  const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
  window.close();
  const fixture = {
    upstream: upstream.commit,
    locale,
    scene: SCENE,
    units: json(units),
    toggle: TOGGLE.map((c) => toggle(up, c)),
    interactions,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`search-menu: ${error.message}\n`);
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
      process.stderr.write("search menu goldens are out of date: run node tools/goldens/search-menu.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`search menu goldens up to date: ${Object.keys(files).length} files\n`);
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
