#!/usr/bin/env node
// Command palette goldens for excali-ui (ex-527): upstream's own
// CommandPalette (packages/excalidraw/components/CommandPalette/
// CommandPalette.tsx) in its Dialog and Modal, fed upstream's own actions
// (actions/index.ts, undo and redo from createUndoAction/createRedoAction)
// and searched with fuzzy 0.1.3 (the version upstream's yarn.lock pins),
// rendered by React 19.0.0 into jsdom 22.1.0 once per platform (isDarwin is
// read from the navigator when the modules load), and CommandPalette.scss
// compiled with sass 1.51.0.
//
//   node tools/goldens/command-palette.mjs            write the fixture and CSS
//   node tools/goldens/command-palette.mjs --check    exit 1 if either is stale
//   node tools/goldens/command-palette.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/command-palette.json:
//
// - `locale`: the English strings (locales/en.json) of every label the
//   palette can show: the `t("...")` keys of CommandPalette.tsx, the
//   `toolBar.<tool>` of each TOOLS entry, and each palette action's label
//   (a static label, or every locale key literal in a label function);
// - `cases`: per case the platform (`darwin`), the form factor, the scene
//   (`elements`, JSON as upstream holds them, built by API.createElement),
//   the app state keys over getDefaultAppState(), the app props the palette
//   reads (`aiEnabled`, `UIOptions.tools`, `UIOptions.canvasActions
//   .toggleTheme`), the library item names, whether the hosted app's Links
//   items are passed (`customCommandPaletteItems`, excalidraw-app/App.tsx:
//   1176-1245), the command executed before the palette is opened again
//   (`lastUsed`), the search typed, the keys pressed; and what the palette
//   shows: `recents` (the last used command) and `categories` (`{title,
//   items: [{label, shortcut, icon, selected, disabled}]}`), `noMatch`, the
//   label selected after each key (`selections`), and (when `dom`) the tree
//   the palette portals to the body: `{tag, attrs, style, children}` with
//   attributes and inline style sorted by name, text as a string, and an
//   icon's <svg> as `{icon: name}` (the icons.tsx export whose markup it
//   is);
// - `perform`: per command of the `perform-*` cases, what a click on it
//   does, in order: `{setAppState: patch}` (the keys that change),
//   `{executeAction: name, source}`, `{confirmDialog: name}`
//   (activeConfirmDialogAtom), `{setActiveTool: type}`, `{toggleLock:
//   true}`, `{insertElements: count}`, `{openUrl: url}`;
// - `toggle`: the palette's window shortcut (Ctrl/Cmd+/, Ctrl/Cmd+Shift+P)
//   per key and modifiers, open or closed: the openDialog it sets.
//
// and crates/excali-ui/src/command_palette/command_palette.css:
// components/CommandPalette/CommandPalette.scss compiled (expanded).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "command-palette.json");
export const STYLESHEET = join("src", "command_palette", "command_palette.css");

const ENTRY = `
// the actions first, as index.tsx loads them: actionLinearEditor reads
// DEFAULT_CATEGORIES while it loads
export * as actions from "./packages/excalidraw/actions";
export { actions as registered } from "./packages/excalidraw/actions/register";
// the palette's actions actions/index.ts does not re-export (they register
// when their module loads)
import "./packages/excalidraw/actions/actionToggleViewMode";
import "./packages/excalidraw/actions/actionFrame";
import "./packages/excalidraw/actions/actionElementLock";
import "./packages/excalidraw/actions/actionCanvas";
import "./packages/excalidraw/actions/actionLinearEditor";
import "./packages/excalidraw/actions/actionProperties";
import "./packages/excalidraw/actions/actionElementLink";
export { CommandPalette, DEFAULT_CATEGORIES } from "./packages/excalidraw/components/CommandPalette/CommandPalette";
export { createUndoAction, createRedoAction } from "./packages/excalidraw/actions/actionHistory";
export { activeConfirmDialogAtom } from "./packages/excalidraw/components/ActiveConfirmDialog";
export { libraryItemsAtom } from "./packages/excalidraw/data/library";
export { TOOLS } from "./packages/excalidraw/components/Tools";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { Scene } from "./packages/element/src/Scene";
export { reseed } from "./packages/common/src/random";
export {
  DEFAULT_VERTICAL_ALIGN,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
  ROUNDNESS,
} from "./packages/common/src/index";
export {
  newElement,
  newFrameElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newImageElement,
  newEmbeddableElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks the palette reads; the
// shim answers them from globalThis.__ui. editor-jotai is a store over
// React's useSyncExternalStore (globalThis.__React) that
// globalThis.__jotai resets per case and reports sets of.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useAppProps: () => globalThis.__ui.app.props,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.app.editorInterface,
      useExcalidrawSetAppState: () => globalThis.__ui.setAppState,
      useExcalidrawActionManager: () => globalThis.__ui.actionManager,
      useExcalidrawAppState: () => globalThis.__ui.app.state,
      useExcalidrawElements: () => globalThis.__ui.app.scene.getNonDeletedElements(),
    };`,
  "packages/excalidraw/editor-jotai": `
    const atom = (init) => ({ init });
    const J = (globalThis.__jotai = globalThis.__jotai || {
      values: new Map(),
      listeners: new Set(),
      onSet: null,
      reset() { this.values = new Map(); },
    });
    const get = (a) => (J.values.has(a) ? J.values.get(a) : a.init);
    const set = (a, v) => {
      J.values.set(a, v);
      if (J.onSet) J.onSet(a, v);
      for (const l of [...J.listeners]) l();
    };
    const subscribe = (l) => { J.listeners.add(l); return () => J.listeners.delete(l); };
    const useAtomValue = (a) => globalThis.__React.useSyncExternalStore(subscribe, () => get(a));
    // jotai's setters are stable: the palette's effects depend on them
    const setters = new WeakMap();
    const setter = (a) => {
      if (!setters.has(a)) setters.set(a, (v) => set(a, v));
      return setters.get(a);
    };
    module.exports = {
      atom,
      useAtomValue,
      useSetAtom: setter,
      useAtom: (a) => [useAtomValue(a), setter(a)],
      editorJotaiStore: { get, set, sub: (a, l) => subscribe(l) },
    };`,
  "packages/excalidraw/hooks/useLibraryItemSvg": `
    module.exports = {
      useLibraryCache: () => ({ svgCache: new Map() }),
      useLibraryItemSvg: () => undefined,
    };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
};

// Packages the actions index reaches that the palette never runs.
const STUBS = ["pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: command-palette.mjs [--check] [--out DIR]\n");
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

// -- scenes and cases ---------------------------------------------------------

const rect = (up, id, x, y, rest = {}) => apiCreateElement(up, { type: "rectangle", id, x, y, ...rest });
const text = (up, id, x, y, rest = {}) => apiCreateElement(up, { type: "text", id, x, y, text: "hello", ...rest });
const selecting = (...ids) => ({ selectedElementIds: Object.fromEntries(ids.map((id) => [id, true])) });

const CHROME = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
const PLATFORMS = [
  { name: "linux", platform: "Linux x86_64" },
  { name: "darwin", platform: "MacIntel" },
];

/** The library items (all named but the last, which the palette skips). */
const LIBRARY = ["Server rack", "Café table", "Router", null];

/**
 * { name, platform, scene(up), state, formFactor, aiEnabled, tools,
 * toggleTheme, library, links, lastUsed, search, keys, dom }: the lists of
 * research/ui-design-system.md 3.14 over the states their predicates read.
 */
const CASES = [
  { name: "default", dom: true },
  { name: "darwin", platform: "darwin", dom: true },
  { name: "phone", formFactor: "phone", dom: true },
  { name: "dark", state: { theme: "dark" }, toggleTheme: true },
  { name: "toggle-theme", toggleTheme: true },
  { name: "ai-disabled", aiEnabled: false },
  { name: "tools-hidden", tools: { image: false } },
  { name: "tool-locked", state: { activeTool: { type: "selection", customType: null, locked: true, fromSelection: false, lastActiveTool: null } } },
  { name: "view-mode", scene: (up) => [rect(up, "r1", 0, 0)], state: { viewModeEnabled: true } },
  { name: "scene", scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)] },
  { name: "selection", scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)], state: selecting("r1"), dom: true },
  {
    name: "selection-two",
    scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)],
    state: selecting("r1", "r2"),
  },
  { name: "selection-text", scene: (up) => [text(up, "t1", 0, 0)], state: selecting("t1") },
  {
    name: "selection-arrow",
    scene: (up) => [apiCreateElement(up, { type: "arrow", id: "a1", x: 0, y: 0, width: 100, height: 100 })],
    state: selecting("a1"),
  },
  { name: "selection-locked", scene: (up) => [rect(up, "r1", 0, 0, { locked: true })], state: selecting("r1") },
  { name: "links", links: true, dom: true },
  { name: "library", library: LIBRARY },
  { name: "search-zoom", search: "zoom", dom: true },
  { name: "search-one-letter", search: "r", library: LIBRARY },
  { name: "search-library", search: "ro", library: LIBRARY },
  { name: "search-deburr", search: "cafe", library: LIBRARY },
  { name: "search-keyword", search: "outline", scene: (up) => [rect(up, "r1", 0, 0)], state: selecting("r1") },
  { name: "search-spaces", search: "zoom in", links: true },
  { name: "search-exact", search: "undo" },
  { name: "search-links", search: "twitter", links: true },
  { name: "search-none", search: "qqqq", dom: true },
  { name: "last-used", lastUsed: "Zoom in", dom: true },
  { name: "last-used-search", lastUsed: "Zoom in", search: "zoom" },
  {
    name: "last-used-unavailable",
    lastUsed: "Group selection",
    scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)],
    state: selecting("r1", "r2"),
    after: { selectedElementIds: {} },
  },
  { name: "keys-down", keys: ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp"] },
  { name: "keys-up-wraps", keys: ["ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown"] },
  { name: "keys-search", search: "zoom", keys: ["ArrowDown", "ArrowDown", "ArrowUp", "ArrowUp", "ArrowUp"] },
  { name: "keys-last-used", lastUsed: "Zoom in", keys: ["ArrowDown", "ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown"] },
  { name: "keys-letter", keys: ["a", "Escape", "ArrowDown"] },
].map((c) => ({ platform: "linux", ...c }));

/** The cases whose every command is clicked (see `perform`). */
const PERFORM = [
  { name: "perform-default", links: true, toggleTheme: true, library: LIBRARY, search: "ro" },
  { name: "perform-canvas", links: true, toggleTheme: true },
  {
    name: "perform-selection",
    scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)],
    state: { ...selecting("r1", "r2"), openMenu: "canvas" },
  },
].map((c) => ({ platform: "linux", ...c }));

// -- DOM ----------------------------------------------------------------------

const installDom = (p) => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  Object.defineProperty(window.navigator, "platform", { value: p.platform, configurable: true });
  Object.defineProperty(window.navigator, "userAgent", { value: CHROME, configurable: true });
  // the async clipboard of a current browser: copy as PNG/SVG can hold
  Object.defineProperty(window.navigator, "clipboard", {
    value: { writeText: () => Promise.resolve(), write: () => Promise.resolve(), readText: () => Promise.resolve("") },
    configurable: true,
  });
  window.ClipboardItem = function ClipboardItem() {};
  if (!("toBlob" in window.HTMLCanvasElement.prototype)) window.HTMLCanvasElement.prototype.toBlob = () => {};
  window.open = (url) => globalThis.__ui?.effect({ openUrl: url });
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLDivElement: window.HTMLDivElement,
    HTMLInputElement: window.HTMLInputElement,
    HTMLTextAreaElement: window.HTMLTextAreaElement,
    HTMLCanvasElement: window.HTMLCanvasElement,
    SVGElement: window.SVGElement,
    MutationObserver: window.MutationObserver,
    DOMRect: window.DOMRect,
    devicePixelRatio: 1,
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
  return window;
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  return sorted(Array.from({ length: style.length }, (_, i) => style.item(i)).map((p) => [p, style.getPropertyValue(p)]));
};

/** The static <svg> icons of icons.tsx. */
const staticIcons = (icons) =>
  Object.entries(icons).filter(
    ([, value]) => value && value.$$typeof === Symbol.for("react.transitional.element") && value.type === "svg",
  );

/** The themed icons of icons.tsx: React.memo components. */
const themedIcons = (icons) =>
  Object.entries(icons).filter(([, value]) => value && value.$$typeof === Symbol.for("react.memo"));

const makeTree = (iconNames) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (!name) throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      return { icon: name };
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

const iconOf = (iconNames, item) => {
  const svg = item.querySelector(".name > .icon > svg");
  if (!svg) return null;
  const name = iconNames.get(svg.outerHTML);
  if (!name) throw new Error(`an svg that is no icons.tsx export: ${svg.outerHTML.slice(0, 120)}`);
  return name;
};

const itemOf = (iconNames, item) => ({
  label: item.querySelector(".name").textContent,
  shortcut: item.querySelector(":scope > .shortcut")
    ? [...item.querySelectorAll(":scope > .shortcut .shortcut-key")].map((k) => k.textContent)
    : null,
  icon: item.querySelector(".library-item-icon") ? "library-item" : iconOf(iconNames, item),
  selected: item.classList.contains("item-selected"),
  disabled: item.classList.contains("item-disabled"),
  large: item.classList.contains("command-item-large"),
});

/** What the palette shows (see the header). */
const shown = (iconNames, portal) => {
  const categories = [...portal.querySelectorAll(".commands > .command-category")].map((category) => ({
    title: category.querySelector(".command-category-title").textContent,
    items: [...category.querySelectorAll(".command-item")].map((i) => itemOf(iconNames, i)),
  }));
  // the recents category is the one whose title carries the history icon
  const first = portal.querySelector(".commands > .command-category");
  const recents = first?.querySelector(".command-category-title > .icon") ? categories.shift() : null;
  return {
    recents: recents && recents.items[0],
    categories,
    noMatch: !!portal.querySelector(".commands > .no-match"),
  };
};

const selectedLabel = (portal) => portal.querySelector(".command-item.item-selected .name")?.textContent ?? null;

// -- mounting -------------------------------------------------------------------

/** The hosted app's Links items (excalidraw-app/App.tsx:1176-1245). */
const linkItems = (up) => [
  {
    label: "GitHub",
    icon: up.icons.GithubIcon,
    category: up.DEFAULT_CATEGORIES.links,
    predicate: true,
    keywords: ["issues", "bugs", "requests", "report", "features", "social", "community"],
    perform: () => window.open("https://github.com/excalidraw/excalidraw", "_blank", "noopener noreferrer"),
  },
  {
    label: up.t("labels.followUs"),
    icon: up.icons.XBrandIcon,
    category: up.DEFAULT_CATEGORIES.links,
    predicate: true,
    keywords: ["twitter", "contact", "social", "community"],
    perform: () => window.open("https://x.com/excalidraw", "_blank", "noopener noreferrer"),
  },
  {
    label: up.t("labels.discordChat"),
    category: up.DEFAULT_CATEGORIES.links,
    predicate: true,
    icon: up.icons.DiscordIcon,
    keywords: [
      "chat",
      "talk",
      "contact",
      "bugs",
      "requests",
      "report",
      "feedback",
      "suggestions",
      "social",
      "community",
    ],
    perform: () => window.open("https://discord.gg/UexuTaE", "_blank", "noopener noreferrer"),
  },
  {
    label: "YouTube",
    icon: up.icons.youtubeIcon,
    category: up.DEFAULT_CATEGORIES.links,
    predicate: true,
    keywords: ["features", "tutorials", "howto", "help", "community"],
    perform: () => window.open("https://youtube.com/@excalidraw", "_blank", "noopener noreferrer"),
  },
];

/** Every registered action (register.ts) by name, undo and redo over an
 * empty history, as App's ActionManager holds them. */
const actionsByName = (up) => {
  const history = {
    isUndoStackEmpty: true,
    isRedoStackEmpty: true,
    onHistoryChangedEmitter: { on: () => () => {} },
  };
  const out = {};
  for (const action of up.registered) out[action.name] = action;
  out.undo = up.createUndoAction(history);
  out.redo = up.createRedoAction(history);
  return out;
};

const libraryItem = (up, name, i) => ({
  id: `lib${i}`,
  status: "unpublished",
  created: 1,
  elements: [rect(up, `lib${i}-r`, 0, 0)],
  ...(name === null ? {} : { name }),
});

/** The state of a case: scene, app state, app, props. */
const setup = (up, c) => {
  up.reseed(1);
  const elements = c.scene ? c.scene(up) : [];
  const scene = new up.Scene(elements, { skipValidation: true });
  const appState = {
    ...up.getDefaultAppState(),
    width: 1200,
    height: 800,
    ...(c.state ?? {}),
    openDialog: { name: "commandPalette" },
  };
  const props = {
    UIOptions: {
      canvasActions: { toggleTheme: c.toggleTheme ? true : null },
      tools: { image: true, ...(c.tools ?? {}) },
    },
    // index.tsx passes `aiEnabled !== false`
    aiEnabled: c.aiEnabled !== false,
  };
  return { elements, scene, appState, props };
};

const mount = async (up, window, c, s) => {
  const { React, act, createRoot, CommandPalette } = up;
  const { document } = window;
  document.body.innerHTML = "";
  document.body.className = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const effects = [];
  const effect = (e) => effects.push(e);
  const app = {
    props: s.props,
    scene: s.scene,
    state: s.appState,
    editorInterface: { formFactor: c.formFactor ?? "desktop" },
    isInteractionEnabled: () => true,
    isNavigationEnabled: () => true,
    setActiveTool: (tool) => effect({ setActiveTool: tool.type }),
    toggleLock: () => effect({ toggleLock: true }),
    onInsertElements: (elements) => effect({ insertElements: elements.length }),
  };
  const actionManager = {
    app,
    actions: s.actions,
    executeAction: (action, source) => effect({ executeAction: action.name, source: source ?? "api" }),
  };
  const setAppState = (patch, cb) => {
    const next = typeof patch === "function" ? patch(app.state) : patch;
    const changed = Object.fromEntries(
      Object.entries(next).filter(([k, v]) => JSON.stringify(v) !== JSON.stringify(app.state[k])),
    );
    effect({ setAppState: json(changed) });
    cb?.();
  };
  globalThis.__ui = { container: { container, id: "excalidraw-id" }, app, actionManager, setAppState, effect };
  globalThis.__jotai.onSet = (atom, value) => {
    if (atom === up.activeConfirmDialogAtom) effect({ confirmDialog: value });
  };
  const root = createRoot(container);
  const element = React.createElement(
    up.UIAppStateContext.Provider,
    { value: s.appState },
    React.createElement(CommandPalette, { customCommandPaletteItems: c.links ? linkItems(up) : undefined }),
  );
  await act(async () => root.render(element));
  return { root, effects, portal: document.body.lastElementChild };
};

const type = async (up, window, portal, value) => {
  const input = portal.querySelector("input");
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  await up.act(async () => {
    setter.call(input, value);
    input.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
};

const click = async (up, window, portal, label) => {
  const item = [...portal.querySelectorAll(".command-item")].find((i) => i.querySelector(".name").textContent === label);
  if (!item) throw new Error(`no command ${label}`);
  await up.act(async () => {
    item.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true }));
  });
};

/** Runs `lastUsed` (a click, then the palette closed) before the case. */
const primeLastUsed = async (up, window, c) => {
  if (!c.lastUsed) return;
  const s = { ...setup(up, c), actions: actionsByName(up) };
  const { root, portal } = await mount(up, window, c, s);
  await click(up, window, portal, c.lastUsed);
  await up.act(async () => root.unmount());
};

const renderCase = async (up, window, iconNames, c) => {
  globalThis.__jotai.reset();
  if (c.library) {
    globalThis.__jotai.values.set(up.libraryItemsAtom, {
      status: "loaded",
      isInitialized: true,
      libraryItems: c.library.map((name, i) => libraryItem(up, name, i)),
    });
  }
  await primeLastUsed(up, window, c);
  const s = { ...setup(up, c), actions: actionsByName(up) };
  if (c.after) Object.assign(s.appState, c.after);
  const { root, portal } = await mount(up, window, c, s);
  if (c.search) await type(up, window, portal, c.search);
  const selections = [];
  for (const key of c.keys ?? []) {
    await up.act(async () => {
      window.dispatchEvent(new window.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    });
    selections.push(selectedLabel(portal));
  }
  const out = {
    name: c.name,
    darwin: c.platform === "darwin",
    formFactor: c.formFactor ?? "desktop",
    elements: json(s.elements),
    appState: json(statePatch(up, s.appState)),
    aiEnabled: s.props.aiEnabled,
    tools: s.props.UIOptions.tools,
    toggleTheme: !!c.toggleTheme,
    library: c.library ?? [],
    links: !!c.links,
    lastUsed: c.lastUsed ?? null,
    search: c.search ?? "",
    keys: c.keys ?? [],
    ...shown(iconNames, portal),
    selections,
    ...(c.dom ? { dom: [makeTree(iconNames)(portal)] } : {}),
  };
  await up.act(async () => root.unmount());
  return out;
};

const statePatch = (up, appState) => {
  const defaults = up.getDefaultAppState();
  return Object.fromEntries(
    Object.entries(appState).filter(([k, v]) => JSON.stringify(v) !== JSON.stringify(defaults[k])),
  );
};

/** Clicks each command of `c` in a fresh palette; the effects in order. */
const performCase = async (up, window, c) => {
  const reset = () => {
    globalThis.__jotai.reset();
    if (c.library) {
      globalThis.__jotai.values.set(up.libraryItemsAtom, {
        status: "loaded",
        isInitialized: true,
        libraryItems: c.library.map((name, i) => libraryItem(up, name, i)),
      });
    }
  };
  const labels = async () => {
    reset();
    const s = { ...setup(up, c), actions: actionsByName(up) };
    const { root, portal } = await mount(up, window, c, s);
    if (c.search) await type(up, window, portal, c.search);
    const out = [...portal.querySelectorAll(".command-item:not(.item-disabled)")].map(
      (i) => i.querySelector(".name").textContent,
    );
    await up.act(async () => root.unmount());
    return out;
  };
  const out = [];
  for (const label of await labels()) {
    reset();
    const s = { ...setup(up, c), actions: actionsByName(up) };
    const { root, portal, effects } = await mount(up, window, c, s);
    if (c.search) await type(up, window, portal, c.search);
    await click(up, window, portal, label);
    await up.act(async () => root.unmount());
    out.push({ label, effects: json(effects) });
  }
  return {
    name: c.name,
    elements: json(setup(up, c).elements),
    appState: json(statePatch(up, setup(up, c).appState)),
    toggleTheme: !!c.toggleTheme,
    library: c.library ?? [],
    links: !!c.links,
    search: c.search ?? "",
    commands: out,
  };
};

/** The window shortcut per key, modifiers and open state. */
const toggles = async (up, window, darwin) => {
  const keys = [
    { key: "/", ctrlKey: true },
    { key: "/", metaKey: true },
    { key: "P", ctrlKey: true, shiftKey: true },
    { key: "p", ctrlKey: true, shiftKey: true },
    { key: "P", metaKey: true, shiftKey: true },
    { key: "/", ctrlKey: true, altKey: true },
    { key: "p", ctrlKey: true },
    { key: "/" },
    { key: "?", ctrlKey: true, shiftKey: true },
  ];
  const out = [];
  for (const open of [false, true]) {
    for (const k of keys) {
      globalThis.__jotai.reset();
      const s = { ...setup(up, {}), actions: actionsByName(up) };
      // the outer component listens whether or not the palette is open
      if (!open) s.appState.openDialog = null;
      const { root, effects } = await mount(up, window, {}, s);
      let prevented = false;
      await up.act(async () => {
        const event = new window.KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...k });
        window.dispatchEvent(event);
        prevented = event.defaultPrevented;
      });
      await up.act(async () => root.unmount());
      const patch = effects.find((e) => e.setAppState)?.setAppState;
      out.push({
        darwin,
        open,
        key: k.key,
        ctrlKey: !!k.ctrlKey,
        metaKey: !!k.metaKey,
        shiftKey: !!k.shiftKey,
        altKey: !!k.altKey,
        prevented,
        openDialog: patch === undefined ? "unchanged" : patch.openDialog ?? null,
      });
    }
  }
  return out;
};

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString('@use "components/CommandPalette/CommandPalette";\n', {
    loadPaths: [root],
    style: "expanded",
  }).css;
  return (
    "/* Generated by tools/goldens/command-palette.mjs; do not edit. Upstream's\n" +
    " * packages/excalidraw/components/CommandPalette/CommandPalette.scss at the\n" +
    " * pin, compiled with sass 1.51.0 (expanded). */\n" +
    `${compiled}\n`
  );
};

// -- locale ---------------------------------------------------------------------

const KEY = /"([A-Za-z]+(?:\.[A-Za-z_]+)+)"/g;

/** Every locale key the palette can show (see the header). */
const localeKeys = (up, upstream, actions) => {
  const source = readFileSync(
    join(upstream.dir, "packages", "excalidraw", "components", "CommandPalette", "CommandPalette.tsx"),
    "utf8",
  );
  const keys = new Set([...source.matchAll(/\bt\(\s*"([^"]+)"/g)].map((m) => m[1]));
  for (const tool of Object.keys(up.TOOLS)) keys.add(`toolBar.${tool}`);
  for (const k of ["labels.followUs", "labels.discordChat"]) keys.add(k);
  const names = [...source.matchAll(/actionManager\.actions\.(\w+)/g)].map((m) => m[1]);
  const palette = [
    ...names.map((n) => actions[n]),
    actions.hyperlink,
    actions.copyElementLink,
    actions.linkToElement,
    actions.clearCanvas,
    actions.toggleTheme,
  ];
  for (const action of palette) {
    if (!action) throw new Error("a palette action is missing");
    if (typeof action.label === "string") keys.add(action.label);
    else if (typeof action.label === "function") {
      for (const [, k] of action.label.toString().matchAll(KEY)) if (up.t(k) !== k) keys.add(k);
    }
  }
  // getContextMenuLabel (hyperlink/Hyperlink.tsx), actionLink's label
  for (const k of ["labels.link.editEmbed", "labels.link.edit", "labels.link.createEmbed", "labels.link.create"]) {
    keys.add(k);
  }
  return [...keys].filter((k) => up.t(k) !== k).sort();
};

// -- main -----------------------------------------------------------------------

export const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
  const css = await stylesheet(upstream);
  const cases = [];
  const perform = [];
  const toggle = [];
  let locale;
  for (const p of PLATFORMS) {
    const window = installDom(p);
    const up = await loadUpstream(upstream, {
      entry: ENTRY,
      stubs: STUBS,
      shims: SHIMS,
      jsx: "automatic",
      define: {
        "import.meta.env.MODE": '"test"',
        "import.meta.env.PKG_NAME": "undefined",
        "import.meta.env.PKG_VERSION": "undefined",
      },
    });
    globalThis.__React = up.React;
    up.setCustomTextMetricsProvider({ getLineWidth: (s) => s.length * 10 });
    // markup → icons.tsx export, from each static icon mounted by React
    const iconNames = new Map();
    // and the themed ones (React.memo components taking `theme`) in each
    // theme
    const rendered = [
      ...staticIcons(up.icons),
      ...themedIcons(up.icons).flatMap(([name, value]) =>
        ["light", "dark"].map((theme) => [name, up.React.createElement(value, { theme })]),
      ),
    ];
    for (const [name, value] of rendered) {
      const host = window.document.createElement("div");
      const root = up.createRoot(host);
      await up.act(async () => root.render(value));
      const markup = host.innerHTML;
      await up.act(async () => root.unmount());
      if (iconNames.has(markup)) continue; // an alias: the first export names it
      iconNames.set(markup, name);
    }
    for (const c of CASES.filter((c) => c.platform === p.name)) {
      cases.push(await renderCase(up, window, iconNames, c));
    }
    for (const c of PERFORM.filter((c) => c.platform === p.name)) {
      perform.push(await performCase(up, window, c));
    }
    toggle.push(...(await toggles(up, window, p.name === "darwin")));
    if (p === PLATFORMS[0]) {
      locale = Object.fromEntries(localeKeys(up, upstream, actionsByName(up)).map((k) => [k, up.t(k)]));
    }
    delete globalThis.__ui;
    window.close();
  }
  const fixture = { upstream: upstream.commit, locale, cases, perform, toggle };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`command-palette: ${error.message}\n`);
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
      process.stderr.write("command palette goldens are out of date: run node tools/goldens/command-palette.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`command palette goldens up to date: ${Object.keys(files).length} files\n`);
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
  // React's act() queues its tasks on MessageChannels when it cannot
  // require("timers") (as in this bundle), and their ports stay open.
  process.exit(0);
}
