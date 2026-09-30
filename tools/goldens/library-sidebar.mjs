#!/usr/bin/env node
// Library sidebar goldens for excali-ui (ex-526): upstream's own
// DefaultSidebar (packages/excalidraw/components/DefaultSidebar.tsx) with
// the Sidebar parts it composes (components/Sidebar/*: the island, the
// header with its tab triggers, dock and close buttons, the tabs), the
// LibraryMenu in its library tab (LibraryMenu.tsx, LibraryMenuItems.tsx,
// LibraryMenuSection.tsx, LibraryUnit.tsx, LibraryMenuHeaderContent.tsx,
// LibraryMenuControlButtons.tsx, LibraryMenuBrowseButton.tsx) and the
// library trigger LayerUI renders (DefaultSidebar.Trigger,
// LayerUI.tsx:481-499), rendered by React 19.0.0 into jsdom 22.1.0 with
// radix-ui 1.4.3 inside a stand-in App that holds the app state, and the
// sidebar's stylesheets compiled with sass 1.51.0.
//
//   node tools/goldens/library-sidebar.mjs            write the fixture and CSS
//   node tools/goldens/library-sidebar.mjs --check    exit 1 if either is stale
//   node tools/goldens/library-sidebar.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/library-sidebar.json:
//
// - `locale`: the English strings the sidebar reads (locales/en.json);
// - `items`: the library items the cases hold (id, status, name, and the
//   elements, a rectangle each, whose preview the host draws);
// - `cases`: per case the app state (`openSidebar`,
//   `defaultSidebarDockedPreference`, `theme`, `selectedElementIds`), the
//   editor interface (form factor, `canFitSidebar`), the library's status,
//   the items (ids), which items have a preview, the pending elements'
//   preview, `libraryReturnUrl`, whether the header menu is open
//   (isLibraryMenuOpenAtom), and the DOM React leaves in the editor
//   container: `{tag, attrs, style, children}` with attributes and inline
//   style sorted by name, text as a string, an icon's <svg> as
//   `{icon: name}`, a preview's <svg> as `{preview: id}` (`null` for the
//   pending item), radix's useId ids as `radix-N`, and what floating-ui
//   computes from layout (jsdom has none) left out;
// - `interactions`: per interaction a case to start from and steps, each a
//   DOM event on an element (a CSS selector and its index among the
//   matches): `click` (with `shiftKey`), `input` (a React change with
//   `value`), `keydown` (`key`, on the element or the document),
//   `mouseenter`, `mouseleave`, `dragstart`; per step what the handlers
//   did, in order (`effects`: `setAppState` with the keys it set,
//   `trackEvent`, `focusContainer`, `insert` (the items inserted, by id,
//   and the elements' ids and positions after
//   distributeLibraryItemsOnSquareGrid), `setLibrary` (the items, by id,
//   status and element count), `updateLibrary`, `exportLibrary` (the items
//   by id), `dataTransfer` (type and data) and `atom` (an editor atom set,
//   by name and value)), whether the default was prevented, and the DOM
//   after it;
// - `order`: the sidebar's header controls and the library's sections, in
//   order, as the research page lists them (site/content/research/
//   ui-design-system.md §3.6).
//
// and crates/excali-ui/src/library_sidebar/library_sidebar.css:
// CheckboxItem.scss, LibraryUnit.scss, Spinner.scss, LibraryMenuItems.scss,
// LibraryMenu.scss, SidebarTrigger.scss and Sidebar.scss compiled
// (expanded) as one entry, in upstream's import order (STYLESHEETS).
//
// The search tab's content (SearchMenu, ex-708) is a marker:
// `<template data-slot="SearchMenu">`. A preview is what
// useLibraryItemSvg puts in the unit (exportToSvg of the item, ex-526's
// host renders it with excali-svg): here `<svg data-library-item="id">`.
// The confirm and publish dialogs the menu opens (ConfirmDialog.tsx,
// PublishLibrary.tsx, the publish success Dialog of
// LibraryMenuHeaderContent.tsx; ex-537) portal to the body: each step
// records them as `dialogs` (the body's portal containers, the same
// tree), and `storage`, the publish dialog's saved fields
// (EditorLocalStorage's "publish-library-data", parsed, or null). The
// dialog's item previews (exportToSvg) are markers as the units' are; its
// submit's preview image (generatePreviewImage, canvas and pica) is
// `"previewImage"`, and fetch to the library backend is recorded as
// `submit` (the url and the form's fields, the library parsed) and answers
// what the interaction says (`backend`: a url, or an error). window.alert
// is recorded as `alert`.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test": randomId is id0, id1...), Date.now is 1, window.name is empty.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "library-sidebar.json");
export const STYLESHEET = join("src", "library_sidebar", "library_sidebar.css");

// In the order upstream's modules evaluate them (a module's imports
// before its own stylesheet): LibraryMenuItems.tsx imports
// LibraryMenuHeaderContent (whose ConfirmDialog imports DialogActionButton,
// which imports Spinner, and then PublishLibrary) before
// LibraryMenuSection; LibraryUnit.tsx imports CheckboxItem before
// LibraryUnit.scss, whose `.library-unit__checkbox .Checkbox-box` must win
// over CheckboxItem.scss's; LibraryMenuItems.tsx imports LibraryUnit (via
// LibraryMenuSection) and Spinner before LibraryMenuItems.scss; LibraryMenu
// imports LibraryMenuItems before LibraryMenu.scss; Sidebar.tsx imports
// SidebarTrigger before Sidebar.scss.
export const STYLESHEETS = [
  "components/Spinner.scss",
  "components/DialogActionButton.scss",
  "components/ConfirmDialog.scss",
  "components/PublishLibrary.scss",
  "components/CheckboxItem.scss",
  "components/LibraryUnit.scss",
  "components/LibraryMenuItems.scss",
  "components/LibraryMenu.scss",
  "components/Sidebar/SidebarTrigger.scss",
  "components/Sidebar/Sidebar.scss",
];

const LIBRARY_URL = "https://libraries.excalidraw.com";
const LIBRARY_BACKEND = "https://library-backend.test";
const PUBLISH_KEY = "publish-library-data";

const ENTRY = `
export { DefaultSidebar } from "./packages/excalidraw/components/DefaultSidebar";
export { isSidebarDockedAtom } from "./packages/excalidraw/components/Sidebar/Sidebar";
export { isLibraryMenuOpenAtom } from "./packages/excalidraw/components/LibraryMenu";
export { libraryItemsAtom } from "./packages/excalidraw/data/library";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { deburr } from "./packages/excalidraw/deburr";
export { DEFAULT_SIDEBAR } from "./packages/common/src/constants";
export { capitalizeString } from "./packages/common/src/utils";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// The editor's atoms: a store the stand-in App resets per case, whose sets
// are recorded (by the atom's name, globalThis.__ui.atomNames).
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

// App.tsx (the whole editor) supplies the hooks; the shim answers them from
// globalThis.__ui. The tunnels render in place; the search menu is a slot.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useAppProps: () => globalThis.__ui.appProps,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => "full",
      useExcalidrawElements: () => globalThis.__ui.elements,
      useExcalidrawAppState: () => globalThis.__ui.appState(),
      useExcalidrawSetAppState: () => globalThis.__ui.setAppState,
    };`,
  "packages/excalidraw/context/tunnels": `
    const React = require("react");
    const passThrough = { In: ({ children }) => children, Out: () => null };
    module.exports = {
      useTunnels: () => ({
        DefaultSidebarTriggerTunnel: passThrough,
        DefaultSidebarTabTriggersTunnel: passThrough,
        // withInternalFallback's render counter, per component
        tunnelsJotai: { useAtom: (a) => React.useState(a.init) },
      }),
    };`,
  "packages/excalidraw/editor-jotai": JOTAI,
  "packages/excalidraw/components/SearchMenu": `
    const React = require("react");
    module.exports = { SearchMenu: () => React.createElement("template", { "data-slot": "SearchMenu" }) };`,
  "packages/excalidraw/hooks/useLibraryItemSvg": `
    const React = require("react");
    const libraryItemSvgsCache = { init: null };
    // the preview upstream exports with exportToSvg, as a marker: an item
    // has one when the case lists it (the pending item as "pending")
    const useLibraryItemSvg = (id, elements, svgCache, ref) => {
      const [svg, setSvg] = React.useState();
      React.useEffect(() => {
        if (elements && globalThis.__ui.previews.has(id ?? "pending")) {
          Promise.resolve().then(() => {
            const el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            el.setAttribute("data-library-item", id ?? "pending");
            setSvg(el);
          });
        }
      }, [id, elements]);
      React.useEffect(() => {
        const node = ref.current;
        if (!node) return;
        if (svg) node.innerHTML = svg.outerHTML;
        return () => { node.innerHTML = ""; };
      }, [svg, ref]);
      return svg;
    };
    const useLibraryCache = () => ({
      svgCache: globalThis.__ui.svgCache,
      clearLibraryCache: () => globalThis.__ui.effect({ clearLibraryCache: true }),
      deleteItemsFromLibraryCache: (ids) => globalThis.__ui.effect({ deleteItemsFromLibraryCache: ids }),
    });
    module.exports = { libraryItemSvgsCache, useLibraryItemSvg, useLibraryCache };`,
  "packages/excalidraw/data/json": `
    module.exports = {
      saveLibraryAsJSON: (items) => { globalThis.__ui.effect({ exportLibrary: items.map((i) => i.id) }); return Promise.resolve(); },
    };`,
  "packages/excalidraw/data/filesystem": `
    module.exports = { fileOpen: () => "fileOpen" };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: (category, action, label) => globalThis.__ui.effect({ trackEvent: [category, action, label] }) };`,
};

const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: library-sidebar.mjs [--check] [--out DIR]\n");
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
    ResizeObserver: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
    DOMRect: window.DOMRect,
    devicePixelRatio: 1,
    Event: window.Event,
    CustomEvent: window.CustomEvent,
    MouseEvent: window.MouseEvent,
    KeyboardEvent: window.KeyboardEvent,
    FocusEvent: window.FocusEvent,
    NodeFilter: window.NodeFilter,
    PointerEvent: window.MouseEvent,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  window.ResizeObserver = globals.ResizeObserver;
  recordStyles(window);
  return window;
};

// The inline style React (and radix) write, per declaration object, as set
// (jsdom 22 normalises or drops some values; see color-picker.mjs).
const written = new WeakMap();

const recordStyles = (window) => {
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
  const removeProperty = proto.removeProperty;
  proto.removeProperty = function (property) {
    note(this, property, null);
    return removeProperty.call(this, property);
  };
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

/** React's useId ids (`«r0»` in React 19.0, `:r0:` before) → radix-N. */
const renameIds = (value, ids) =>
  value.replace(/radix-(«r[0-9a-z]+»|:r[0-9a-z]+:)/g, (id) => {
    if (!ids.has(id)) ids.set(id, `radix-${ids.size + 1}`);
    return ids.get(id);
  });

const makeTree = (iconNames, ids) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (name) return { icon: name };
      const preview = node.getAttribute("data-library-item");
      if (preview) return { preview: preview === "pending" ? null : preview };
      // the Spinner's own <svg> is the one other
      if (!node.parentElement?.classList.contains("Spinner")) {
        throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      }
    }
    const attrs = [...node.attributes]
      .filter((a) => a.name !== "style")
      .map((a) => [
        a.name,
        a.name === "id" || a.name === "for" || a.name.startsWith("aria-") ? renameIds(a.value, ids) : a.value,
      ]);
    const popper = node.hasAttribute("data-radix-popper-content-wrapper");
    const placed = node.parentElement?.hasAttribute("data-radix-popper-content-wrapper");
    return {
      tag: node.localName,
      attrs: sorted(placed ? attrs.filter(([n]) => n !== "data-side" && n !== "data-align") : attrs),
      style: popper ? {} : styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

// -- library --------------------------------------------------------------------

/** A rectangle for a library item or the canvas selection. */
const rect = (id, x, y, width, height) => ({
  id,
  type: "rectangle",
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
});

// [id, status, name, width, height]
const ITEM_SPECS = [
  ["u1", "unpublished", "Box", 40, 20],
  ["u2", "unpublished", "", 20, 60],
  ["u3", "unpublished", "Café Diamond", 30, 30],
  ["u4", "unpublished", "  ", 10, 10],
  ["p1", "published", "Arrow", 80, 10],
  ["p2", "published", "Frame Cafe", 50, 50],
];

const ITEMS = ITEM_SPECS.map(([id, status, name, width, height], i) => ({
  id,
  status,
  ...(name !== "" ? { name } : {}),
  created: i + 1,
  elements: [rect(`${id}-rect`, i * 100, 0, width, height)],
}));

const item = (id) => ITEMS.find((i) => i.id === id);

const UNPUBLISHED = ["u1", "u2", "u3", "u4"];
const PUBLISHED = ["p1", "p2"];
const ALL = [...UNPUBLISHED, ...PUBLISHED];

// The canvas: two rectangles, the first selected in the "pending" cases.
const CANVAS = [rect("c1", 0, 0, 100, 50), rect("c2", 200, 0, 60, 60)];

// -- cases ----------------------------------------------------------------------

/**
 * A case: `tab` (openSidebar.tab; null closes it), `docked`
 * (defaultSidebarDockedPreference), `theme`, `formFactor`,
 * `canFitSidebar`, `status` (the library's "loaded" / "loading"),
 * `initialized` (isInitialized), `items` (ids), `previews` (the ids with
 * a preview; "pending" for the pending item; default every item),
 * `selected` (selectedElementIds of CANVAS), `libraryReturnUrl`, `menuOpen`
 * (isLibraryMenuOpenAtom) and `trigger` (render LayerUI's library trigger
 * too).
 */
const CASES = [
  { name: "empty" },
  { name: "unpublished", items: UNPUBLISHED },
  { name: "unpublished-no-previews", items: UNPUBLISHED, previews: [] },
  { name: "published", items: PUBLISHED },
  { name: "both", items: ALL },
  { name: "pending", selected: ["c1"], previews: ["pending"] },
  { name: "pending-no-preview", selected: ["c1"], previews: [] },
  { name: "pending-with-items", items: ALL, selected: ["c1", "c2"] },
  { name: "docked", items: ALL, docked: true },
  { name: "docked-cannot-fit", items: ALL, docked: true, canFitSidebar: false },
  { name: "cannot-fit", items: UNPUBLISHED, canFitSidebar: false },
  { name: "search-tab", items: ALL, tab: "search" },
  { name: "search-tab-docked", items: ALL, tab: "search", docked: true },
  { name: "dark", items: UNPUBLISHED, theme: "dark" },
  { name: "phone", items: ALL, formFactor: "phone" },
  { name: "phone-empty", formFactor: "phone" },
  { name: "loading-initial", status: "loading", initialized: false },
  { name: "loading", items: UNPUBLISHED, status: "loading" },
  { name: "loading-empty", status: "loading" },
  { name: "return-url", libraryReturnUrl: "https://host.example/return" },
  { name: "menu-open", items: ALL, menuOpen: true },
  { name: "menu-open-empty", menuOpen: true },
  { name: "closed", tab: null, items: ALL },
  { name: "trigger", tab: null, items: ALL, trigger: true },
  { name: "trigger-open", items: ALL, trigger: true },
  { name: "trigger-docked", items: ALL, docked: true, trigger: true },
  { name: "many", items: [...Array.from({ length: 20 }, (_, i) => `m${i}`)] },
];

// the "many" case's items: 20 unpublished squares
for (let i = 0; i < 20; i++) {
  ITEMS.push({ id: `m${i}`, status: "unpublished", created: 100 + i, elements: [rect(`m${i}-rect`, 0, 0, 10 + i, 10)] });
}

const DEFAULT_CASE = {
  tab: "library",
  docked: false,
  theme: "light",
  formFactor: "desktop",
  canFitSidebar: true,
  status: "loaded",
  initialized: true,
  items: [],
  previews: null,
  selected: [],
  libraryReturnUrl: null,
  menuOpen: false,
  trigger: false,
  publishData: null,
  backend: null,
};

const settle = (c) => {
  const s = { ...DEFAULT_CASE, ...c };
  if (s.previews === null) s.previews = [...s.items, ...(s.selected.length ? ["pending"] : [])];
  return s;
};

// -- the stand-in App -----------------------------------------------------------

/**
 * Mounts a case: an App component holding the app state (setAppState
 * re-renders it), the default sidebar in UIAppStateContext and, for
 * `trigger`, LayerUI's library trigger. Returns the container, the
 * recorded effects and a snapshot function.
 */
const mount = async (up, window, iconNames, c) => {
  const { React, act, createRoot, DefaultSidebar, UIAppStateContext } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  // ConfirmDialog's container?.focus() (useExcalidrawContainer's), recorded as
  // app.focusContainer is
  container.focus = () => globalThis.__ui.effect({ focusContainer: true });

  const effects = [];
  const effect = (e) => effects.push(json(e));
  // the case's own copies: the publish dialog renames and marks published
  // the library's items in place
  const caseItems = c.items.map((id) => structuredClone(item(id)));
  globalThis.__jotai = { values: new Map(), listeners: new Set() };
  globalThis.__jotai.values.set(up.libraryItemsAtom, {
    status: c.status,
    isInitialized: c.initialized,
    libraryItems: caseItems,
  });
  if (c.menuOpen) globalThis.__jotai.values.set(up.isLibraryMenuOpenAtom, true);
  window.localStorage.clear();
  if (c.publishData) window.localStorage.setItem(PUBLISH_KEY, JSON.stringify(c.publishData));
  window.alert = (message) => effect({ alert: String(message) });
  // the library backend (PublishLibrary's fetch): records the form, answers
  // the case's `backend`
  globalThis.fetch = async (url, init) => {
    const fields = [];
    for (const [name, value] of init.body.entries()) {
      const text = typeof value === "string" ? value : await value.text();
      fields.push([name, name === "excalidrawLib" ? JSON.parse(text) : text]);
    }
    effect({ submit: { url, method: init.method, fields } });
    if (c.backend?.error) throw new Error(c.backend.error);
    return {
      ok: true,
      statusText: "OK",
      json: () => Promise.resolve({ url: c.backend?.url ?? "" }),
    };
  };

  let state = {
    openSidebar: c.tab ? { name: "default", tab: c.tab } : null,
    defaultSidebarDockedPreference: c.docked,
    theme: c.theme,
    selectedElementIds: Object.fromEntries(c.selected.map((id) => [id, true])),
    selectedGroupIds: {},
    openMenu: null,
    openPopup: null,
    activeEmbeddable: null,
    errorMessage: null,
  };
  let rerender = () => {};
  const setAppState = (patch) => {
    const next = typeof patch === "function" ? patch(state) : patch;
    const keys = Object.keys(next).filter((k) => typeof patch !== "function" || next[k] !== state[k]);
    effect({ setAppState: Object.fromEntries(keys.map((k) => [k, next[k]])) });
    state = { ...state, ...next };
    rerender();
  };
  const library = {
    setLibrary: (items) => {
      const list = typeof items === "function" ? items(caseItems) : items;
      effect({ setLibrary: list.map((i) => ({ id: i.id, status: i.status, ...(i.name !== undefined ? { name: i.name } : {}), created: i.created, elements: i.elements.map((e) => e.id) })) });
      return Promise.resolve(list);
    },
    resetLibrary: () => effect({ resetLibrary: true }),
    updateLibrary: (opts) => {
      effect({ updateLibrary: { libraryItems: String(opts.libraryItems), merge: opts.merge, openLibraryMenu: opts.openLibraryMenu } });
      return Promise.resolve([]);
    },
    getLatestLibrary: () => Promise.resolve(caseItems),
  };
  const app = {
    id: "app-id",
    library,
    ownerDocument: document,
    ownerWindow: window,
    viewport: { lastPosition: { x: -1, y: -1 } },
    state: { cursorButton: "up", activeTool: { type: "selection" } },
    scene: { getNonDeletedElementsMap: () => new Map(CANVAS.map((e) => [e.id, e])) },
    // the elements are the items' duplicates, distributed (recorded as
    // `distribute`, the items by id); their layout is excali-editor's
    // (tests/fixtures/editing.json "library" cases)
    onInsertElements: (elements) => effect({ insert: elements.length }),
    focusContainer: () => effect({ focusContainer: true }),
  };
  globalThis.__ui = {
    app,
    appProps: { libraryReturnUrl: c.libraryReturnUrl ?? undefined },
    container: { container, id: "excalidraw-id" },
    editorInterface: {
      formFactor: c.formFactor,
      desktopUIMode: "full",
      userAgent: {},
      isTouchScreen: false,
      canFitSidebar: c.canFitSidebar,
      isLandscape: true,
    },
    elements: CANVAS,
    appState: () => state,
    setAppState,
    effect,
    // isSidebarDockedAtom is recorded as its value after each render
    // (`sidebarDocked`), not as the sets of the sidebar's layout effect
    atomNames: new Map([[up.isLibraryMenuOpenAtom, "isLibraryMenuOpen"]]),
    previews: new Set(c.previews),
    svgCache: new Map(),
  };

  const App = () => {
    const [, force] = React.useReducer((x) => x + 1, 0);
    rerender = force;
    return React.createElement(
      UIAppStateContext.Provider,
      { value: state },
      c.trigger
        ? React.createElement(DefaultSidebar.Trigger, {
            __fallback: true,
            icon: up.icons.sidebarRightIcon,
            title: up.capitalizeString(up.t("toolBar.library")),
            onToggle: (open) => {
              if (open) up_track("sidebar", `${up.DEFAULT_SIDEBAR.name} (open)`, `button (${c.formFactor === "phone" ? "mobile" : "desktop"})`);
            },
            tab: up.DEFAULT_SIDEBAR.defaultTab,
          })
        : null,
      React.createElement(DefaultSidebar, {
        __fallback: true,
        onDock: (docked) =>
          up_track("sidebar", `toggleDock (${docked ? "dock" : "undock"})`, `(${c.formFactor === "phone" ? "mobile" : "desktop"})`),
      }),
    );
  };
  // LayerUI's handlers call trackEvent (analytics.ts)
  const up_track = (category, action, label) => effect({ trackEvent: [category, action, label] });

  const root = createRoot(container);
  await act(async () => root.render(React.createElement(App)));
  // the previews resolve after a microtask; the batches render in
  // transitions
  for (let i = 0; i < 5; i++) await act(async () => {});
  // a frame passes before the user acts: radix's tab panel stops
  // preventing its mount animation (the panel active at mount keeps
  // `animation-duration: 0s` until it renders again)
  await act(async () => new Promise((resolve) => window.requestAnimationFrame(() => resolve())));
  effects.length = 0;
  const ids = new Map();
  const snapshot = () => [...container.childNodes].map(makeTree(iconNames, ids));
  // the portals Modal appends to the body (useCreatePortalContainer)
  const dialogs = () =>
    [...document.body.children].filter((el) => el !== container && el.childNodes.length).map(makeTree(iconNames, ids));
  const storage = () => JSON.parse(window.localStorage.getItem(PUBLISH_KEY) ?? "null");
  const sidebarDocked = () => {
    const v = globalThis.__jotai.values.get(up.isSidebarDockedAtom);
    return v === undefined ? up.isSidebarDockedAtom.init : v;
  };
  return { container, root, effects, snapshot, dialogs, storage, sidebarDocked };
};

const render = async (up, window, iconNames, raw) => {
  const c = settle(raw);
  const { root, snapshot, sidebarDocked } = await mount(up, window, iconNames, c);
  const dom = snapshot();
  const docked = sidebarDocked();
  await up.act(async () => root.unmount());
  return { name: raw.name, ...caseProps(c), sidebarDocked: docked, dom };
};

const caseProps = (c) => ({
  tab: c.tab,
  docked: c.docked,
  theme: c.theme,
  formFactor: c.formFactor,
  canFitSidebar: c.canFitSidebar,
  status: c.status,
  initialized: c.initialized,
  items: c.items,
  previews: c.previews,
  selected: c.selected,
  libraryReturnUrl: c.libraryReturnUrl,
  menuOpen: c.menuOpen,
  trigger: c.trigger,
  ...(c.publishData ? { publishData: c.publishData } : {}),
  ...(c.backend ? { backend: c.backend } : {}),
});

// -- interactions ---------------------------------------------------------------

const UNIT = ".library-unit__dragger";
const SEARCH = ".library-menu-items-container__search input";
const MENU_ITEM = "[role=menuitem]";
const CONFIRM_BUTTON = ".confirm-dialog-buttons .Dialog__action-button";
// the publish entry's test id is upstream's
const PUBLISH = "[data-testid=lib-dropdown--remove]";

/**
 * [name, case, steps]; a step is [event, selector, index, extra]: `click`
 * (extra: { shiftKey }), `input` (extra: value), `keydown` (extra: key;
 * selector null for the document), `mouseenter`, `mouseleave`,
 * `dragstart`.
 */
const INTERACTIONS = [
  // radix's tab trigger selects on a primary mousedown
  ["tab-search", { items: ALL }, [["mousedown", ".sidebar-tab-trigger", 0]]],
  ["tab-library", { items: ALL, tab: "search" }, [["mousedown", ".sidebar-tab-trigger", 1]]],
  ["tab-same", { items: ALL }, [["mousedown", ".sidebar-tab-trigger", 1]]],
  ["close", { items: ALL }, [["click", ".sidebar__close", 0]]],
  ["dock", { items: ALL }, [["click", ".sidebar__dock", 0]]],
  ["undock", { items: ALL, docked: true }, [["click", ".sidebar__dock", 0]]],
  ["trigger-open", { tab: null, items: ALL, trigger: true }, [["click", ".sidebar-trigger__label-element", 0]]],
  ["trigger-close", { items: ALL, trigger: true }, [["click", ".sidebar-trigger__label-element", 0]]],
  ["insert", { items: ALL }, [["click", UNIT, 0]]],
  ["insert-published", { items: ALL }, [["click", UNIT, 4]]],
  [
    "select-and-insert",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", UNIT, 2, { shiftKey: true }],
      ["click", UNIT, 2],
    ],
  ],
  [
    "select-range",
    { items: ALL },
    [
      ["click", UNIT, 1, { shiftKey: true }],
      ["click", UNIT, 4, { shiftKey: true }],
      ["click", UNIT, 2, { shiftKey: true }],
      ["click", UNIT, 0, { shiftKey: true }],
    ],
  ],
  [
    "select-range-upwards",
    { items: ALL },
    [
      ["click", UNIT, 5, { shiftKey: true }],
      ["click", UNIT, 2, { shiftKey: true }],
    ],
  ],
  [
    "hover-and-checkbox",
    { items: ALL },
    [
      ["mouseenter", ".library-unit", 1],
      ["click", ".library-unit__checkbox", 0],
      ["mouseleave", ".library-unit", 1],
      ["mouseenter", ".library-unit", 3],
      ["click", ".library-unit__checkbox", 1],
    ],
  ],
  [
    "selection-escape",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["keydown", SEARCH, 0, "Escape"],
      ["keydown", SEARCH, 0, "Escape"],
    ],
  ],
  ["escape-undocked", { items: ALL }, [["keydown", null, 0, "Escape"]]],
  ["escape-docked", { items: ALL, docked: true }, [["keydown", null, 0, "Escape"]]],
  ["escape-docked-cannot-fit", { items: ALL, docked: true, canFitSidebar: false }, [["keydown", null, 0, "Escape"]]],
  [
    "search",
    { items: ALL },
    [
      ["input", SEARCH, 0, "caf"],
      ["input", SEARCH, 0, "  CAFÉ "],
      ["input", SEARCH, 0, "zzz"],
      ["click", ".library-menu-items__no-items button", 0],
      ["input", SEARCH, 0, "box"],
      ["click", UNIT, 0],
      ["keydown", SEARCH, 0, "Escape"],
    ],
  ],
  [
    "search-clear-hint",
    { items: ALL },
    [
      ["input", SEARCH, 0, "a"],
      ["click", ".library-menu-items-container__header__hint", 0],
    ],
  ],
  ["drag", { items: ALL }, [["dragstart", UNIT, 1]]],
  [
    "drag-selection",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", UNIT, 4, { shiftKey: true }],
      ["dragstart", UNIT, 1],
      ["dragstart", UNIT, 4],
    ],
  ],
  ["drag-pending", { items: ALL, selected: ["c1"] }, [["dragstart", UNIT, 0]]],
  ["add-to-library", { items: ALL, selected: ["c1", "c2"] }, [["click", UNIT, 0]]],
  ["add-to-empty-library", { selected: ["c2"] }, [["click", UNIT, 0]]],
  [
    "menu",
    { items: ALL },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", "[data-testid=lib-dropdown--export]", 0],
    ],
  ],
  [
    "menu-load",
    { items: ALL },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", "[data-testid=lib-dropdown--load]", 0],
    ],
  ],
  [
    "menu-selection",
    { items: ALL },
    [
      ["click", UNIT, 1, { shiftKey: true }],
      ["click", UNIT, 5, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", "[data-testid=lib-dropdown--export]", 0],
    ],
  ],
  // ex-537: the confirm dialogs (ConfirmDialog.tsx)
  [
    "reset-cancel",
    { items: ALL },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", MENU_ITEM, 2],
      ["click", CONFIRM_BUTTON, 0],
    ],
  ],
  [
    "reset-confirm",
    { items: ALL },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", MENU_ITEM, 2],
      ["click", CONFIRM_BUTTON, 1],
    ],
  ],
  [
    "reset-backdrop",
    { items: UNPUBLISHED },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", MENU_ITEM, 2],
      ["click", ".Modal__background", 0],
    ],
  ],
  [
    "remove-confirm",
    { items: ALL },
    [
      ["click", UNIT, 1, { shiftKey: true }],
      ["click", UNIT, 4, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", MENU_ITEM, 2],
      ["click", CONFIRM_BUTTON, 1],
    ],
  ],
  [
    "remove-cancel",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", MENU_ITEM, 2],
      ["click", CONFIRM_BUTTON, 0],
    ],
  ],
  // ex-537: the publish dialog (PublishLibrary.tsx) and its success dialog
  [
    "publish-validate",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", UNIT, 1, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", PUBLISH, 0],
      ["submit", ".publish-library form", 0],
      ["input", ".single-library-item input", 1, "Tall"],
      ["input", ".publish-library__fields input[name=name]", 0, "Shapes"],
      ["input", ".publish-library__fields textarea", 0, "Some shapes"],
      ["click", ".publish-library__buttons .Dialog__action-button", 0],
    ],
  ],
  [
    "publish-submit",
    { items: ALL, backend: { url: "https://libraries.test/pr/1" } },
    [
      ["click", UNIT, 2, { shiftKey: true }],
      ["click", UNIT, 5, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", PUBLISH, 0],
      ["input", ".single-library-item input", 0, "Wide box"],
      ["input", ".publish-library__fields input[name=name]", 0, "Shapes"],
      ["input", ".publish-library__fields textarea", 0, "Some shapes"],
      ["input", ".publish-library__fields input[name=authorName]", 0, "Ada"],
      ["input", ".publish-library__fields input[name=githubHandle]", 0, "ada"],
      ["input", ".publish-library__fields input[name=twitterHandle]", 0, "@ada"],
      ["input", ".publish-library__fields input[name=website]", 0, "https://ada.test"],
      ["submit", ".publish-library form", 0],
      ["click", ".publish-library-success-close", 0],
    ],
  ],
  [
    "publish-error",
    { items: ALL, backend: { error: "backend down" } },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", PUBLISH, 0],
      ["submit", ".publish-library form", 0],
    ],
  ],
  [
    "publish-saved",
    {
      items: ALL,
      publishData: {
        authorName: "Ada",
        githubHandle: "ada",
        name: "Saved",
        description: "Saved shapes",
        twitterHandle: "",
        website: "",
      },
    },
    [
      ["click", UNIT, 4, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", PUBLISH, 0],
      ["click", ".Modal__background", 0],
    ],
  ],
  [
    "publish-remove",
    { items: ALL },
    [
      ["click", UNIT, 0, { shiftKey: true }],
      ["click", UNIT, 2, { shiftKey: true }],
      ["click", ".dropdown-menu-button", 0],
      ["click", PUBLISH, 0],
      ["click", ".single-library-item--remove", 1],
      ["click", ".single-library-item--remove", 0],
    ],
  ],
  [
    "menu-toggle",
    { items: ALL },
    [
      ["click", ".dropdown-menu-button", 0],
      ["click", ".dropdown-menu-button", 0],
    ],
  ],
];

const fire = async (up, window, container, step) => {
  const [event, selector, index, extra] = step;
  const { document } = window;
  // the sidebar's controls, then the dialogs' in the body
  const target =
    selector === null
      ? document.body
      : [...container.querySelectorAll(selector), ...[...document.body.children].filter((el) => el !== container).flatMap((el) => [...el.querySelectorAll(selector)])][index];
  if (!target) throw new Error(`no ${selector}[${index}]`);
  let prevented = false;
  await up.act(async () => {
    let e;
    if (event === "click" || event === "mousedown") {
      e = new window.MouseEvent(event, { bubbles: true, cancelable: true, button: 0, shiftKey: !!extra?.shiftKey });
    } else if (event === "submit") {
      e = new window.Event("submit", { bubbles: true, cancelable: true });
    } else if (event === "input") {
      const proto = target.localName === "textarea" ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, "value").set;
      setter.call(target, extra);
      e = new window.Event("input", { bubbles: true, cancelable: true });
    } else if (event === "keydown") {
      e = new window.KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: extra });
    } else if (event === "mouseenter" || event === "mouseleave") {
      // React listens for mouseover / mouseout to derive enter and leave
      const over = event === "mouseenter" ? "mouseover" : "mouseout";
      e = new window.MouseEvent(over, { bubbles: true, cancelable: true, relatedTarget: event === "mouseenter" ? document.body : document.body });
    } else if (event === "dragstart") {
      const data = [];
      e = new window.Event("dragstart", { bubbles: true, cancelable: true });
      e.dataTransfer = {
        setData: (type, value) => {
          data.push([type, value]);
          globalThis.__ui.effect({ dataTransfer: [type, value] });
        },
      };
    } else {
      throw new Error(`event ${event}`);
    }
    target.dispatchEvent(e);
    prevented = e.defaultPrevented;
  });
  for (let i = 0; i < 5; i++) await up.act(async () => {});
  // the submit's Blob reads and fetch settle in later tasks
  if (event === "submit") for (let i = 0; i < 5; i++) await up.act(() => new Promise((r) => setTimeout(r, 0)));
  return prevented;
};

const interact = async (up, window, iconNames, [name, raw, steps]) => {
  const c = settle({ name, ...raw });
  const mounted = await mount(up, window, iconNames, c);
  const out = [];
  for (const step of steps) {
    mounted.effects.length = 0;
    const defaultPrevented = await fire(up, window, mounted.container, step);
    const [event, selector, index, extra] = step;
    out.push({
      event,
      selector,
      index,
      ...(event === "click" ? { shiftKey: !!extra?.shiftKey } : {}),
      ...(event === "input" ? { value: extra } : {}),
      ...(event === "keydown" ? { key: extra } : {}),
      effects: [...mounted.effects],
      defaultPrevented,
      sidebarDocked: mounted.sidebarDocked(),
      dom: mounted.snapshot(),
      dialogs: mounted.dialogs(),
      storage: mounted.storage(),
    });
  }
  await up.act(async () => mounted.root.unmount());
  return { name, ...caseProps(c), steps: out };
};

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/library-sidebar.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`
  );
};

// -- deburr ---------------------------------------------------------------------

// Every code point of deburr's Latin-1 and Latin Extended-A ranges, the
// combining marks it strips (each range's ends and middle), and names.
const DEBURR_INPUTS = [
  ...Array.from({ length: 0x180 - 0xc0 }, (_, i) => String.fromCodePoint(0xc0 + i)),
  ...[0x2ff, 0x300, 0x301, 0x36f, 0x370, 0xfe1f, 0xfe20, 0xfe2f, 0xfe30, 0x20cf, 0x20d0, 0x20e0, 0x20ff, 0x2100].map(
    (c) => `e${String.fromCodePoint(c)}x`,
  ),
  "Café Diamond",
  "cafe\u0301",
  "Œuvre ĳssel ŉ ſ ×÷",
  "plain ascii",
  "",
];

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = [
  "toolBar.library",
  "labels.sidebarLock",
  "buttons.close",
  "labels.personalLib",
  "labels.excalidrawLib",
  "labels.libraries",
  "labels.libraryLoadingMessage",
  "library.noItems",
  "library.hint_emptyLibrary",
  "library.hint_emptyPrivateLibrary",
  "library.search.inputPlaceholder",
  "library.search.heading",
  "library.search.noResults",
  "library.search.clearSearch",
  "buttons.load",
  "buttons.export",
  "buttons.publishLibrary",
  "buttons.remove",
  "buttons.resetLibrary",
  "errors.libraryElementTypeError.embeddable",
  "errors.libraryElementTypeError.iframe",
  "errors.libraryElementTypeError.image",
  "errors.importLibraryError",
  "alerts.errorAddingToLibrary",
  "alerts.errorRemovingFromLibrary",
  "alerts.resetLibrary",
  "confirmDialog.resetLibrary",
  "confirmDialog.removeItemsFromLib",
  "alerts.removeItemsFromsLibrary",
  "buttons.confirm",
  "buttons.cancel",
  "buttons.saveLibNames",
  "buttons.submit",
  "labels.statusPublished",
  "publishDialog.title",
  "publishDialog.itemName",
  "publishDialog.authorName",
  "publishDialog.githubUsername",
  "publishDialog.twitterUsername",
  "publishDialog.libraryName",
  "publishDialog.libraryDesc",
  "publishDialog.website",
  "publishDialog.placeholder.authorName",
  "publishDialog.placeholder.libraryName",
  "publishDialog.placeholder.libraryDesc",
  "publishDialog.placeholder.githubHandle",
  "publishDialog.placeholder.twitterHandle",
  "publishDialog.placeholder.website",
  "publishDialog.errors.required",
  "publishDialog.errors.website",
  "publishDialog.noteDescription",
  "publishDialog.noteGuidelines",
  "publishDialog.noteLicense",
  "publishDialog.noteItems",
  "publishDialog.atleastOneLibItem",
  "publishDialog.republishWarning",
  "publishSuccessDialog.title",
  "publishSuccessDialog.content",
];

/** The header's controls and the sections, as §3.6 lists them. */
const order = (cases) => {
  const text = (n) => (typeof n === "string" ? n : n.children ? n.children.map(text).join("") : "");
  const walk = function* (nodes) {
    for (const n of nodes) {
      if (typeof n !== "object" || !n.tag) continue;
      yield n;
      yield* walk(n.children);
    }
  };
  const both = cases.find((c) => c.name === "both").dom;
  const menu = cases.find((c) => c.name === "menu-open").dom;
  return {
    header: [...walk(both)]
      .filter((n) => n.tag === "button" && /sidebar-tab-trigger|sidebar__dock|sidebar__close/.test(n.attrs.class))
      .map((n) => n.attrs["data-testid"] ?? n.attrs.class),
    sections: [...walk(both)].filter((n) => n.attrs.class === "library-menu-items-container__header").map(text),
    menu: [...walk(menu)].filter((n) => n.attrs.role === "menuitem").map(text),
  };
};

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  window.name = "";
  const now = Date.now;
  Date.now = () => 1;
  try {
    const up = await loadUpstream(upstream, {
      entry: ENTRY,
      stubs: STUBS,
      shims: SHIMS,
      jsx: "automatic",
      // distributeLibraryItemsOnSquareGrid records the items it lays out
      patch: {
        // the dialog's previews as markers (the units' too), and the submit's
        // preview image as a name: jsdom has no canvas
        "packages/excalidraw/components/PublishLibrary": (source) =>
          source
            .replace(
              'import { exportToCanvas, exportToSvg } from "@excalidraw/utils/export";',
              "const exportToCanvas = null;\n" +
                "const exportToSvg = async ({ elements }) => {\n" +
                '  const el = document.createElementNS("http://www.w3.org/2000/svg", "svg");\n' +
                '  el.setAttribute("data-library-item", elements[0].id.replace(/-rect$/, ""));\n' +
                "  return el;\n};",
            )
            .replace(
              "const previewImage = await generatePreviewImage(clonedLibItems);",
              'const previewImage = new File(["previewImage"], "preview", { type: "image/jpeg" });',
            ),
        "packages/excalidraw/data/library": (source) =>
          source.replace("export const distributeLibraryItemsOnSquareGrid = (", "const distributeLibraryItemsOnSquareGrid_ = (") +
          "\nexport const distributeLibraryItemsOnSquareGrid = (items) => {\n" +
          "  globalThis.__ui.effect({ distribute: items.map((i) => i.id) });\n" +
          "  return distributeLibraryItemsOnSquareGrid_(items);\n};\n",
      },
      define: {
        "import.meta.env.MODE": '"test"',
        "import.meta.env.DEV": "false",
        "import.meta.env.PKG_NAME": "undefined",
        "import.meta.env.PKG_VERSION": "undefined",
        "import.meta.env.VITE_APP_LIBRARY_URL": JSON.stringify(LIBRARY_URL),
        "import.meta.env.VITE_APP_LIBRARY_BACKEND": JSON.stringify(LIBRARY_BACKEND),
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
    const cases = [];
    for (const c of CASES) cases.push(await render(up, window, iconNames, c));
    const interactions = [];
    for (const i of INTERACTIONS) interactions.push(await interact(up, window, iconNames, i));
    const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
    window.close();
    const fixture = {
      upstream: upstream.commit,
      libraryUrl: LIBRARY_URL,
      libraryBackend: LIBRARY_BACKEND,
      locale,
      items: ITEMS.map((i) => ({ id: i.id, status: i.status, name: i.name ?? null, created: i.created, elements: i.elements })),
      canvas: CANVAS,
      order: order(cases),
      deburr: DEBURR_INPUTS.map((input) => [input, up.deburr(input)]),
      cases,
      interactions,
    };
    return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
  } finally {
    Date.now = now;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`library-sidebar: ${error.message}\n`);
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
      process.stderr.write("library sidebar goldens are out of date: run node tools/goldens/library-sidebar.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`library sidebar goldens up to date: ${Object.keys(files).length} files\n`);
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
