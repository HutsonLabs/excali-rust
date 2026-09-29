#!/usr/bin/env node
// Footer goldens for excali-ui (ex-521): upstream's own desktop Footer
// (packages/excalidraw/components/footer/Footer.tsx) with the ZoomActions,
// UndoRedoActions and ExitZenModeButton of components/Actions.tsx, the
// HelpButton, and the zoom and history actions' PanelComponents
// (actions/actionCanvas.tsx, actions/actionHistory.tsx) rendered through
// upstream's ActionManager.renderAction by React 19.0.0 into jsdom 22.1.0,
// and the footer's rules of upstream's stylesheets compiled with sass
// 1.51.0.
//
//   node tools/goldens/footer.mjs            write the fixture and CSS
//   node tools/goldens/footer.mjs --check    exit 1 if either is stale
//   node tools/goldens/footer.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/footer.json:
//
// - `locale`: the English strings the footer reads (locales/en.json);
// - `cases`: per case the footer's props and app state (`zoom`,
//   `zenModeEnabled`, `viewModeEnabled`), the history's stack emptiness,
//   whether navigation is enabled (`props.interaction`), and the DOM React
//   leaves in the editor container: `{tag, attrs, style, children}` with
//   attributes and inline style sorted by name, text as a string, and an
//   icon's <svg> as `{icon: name}` (the icons.tsx export whose markup it
//   is; excali-ui's icon set holds that markup, tests/icons.rs);
// - `tooltips`: the label each tooltip wrapper shows when hovered;
// - `viewport`: the size, offset and scroll of the click cases;
// - `clicks`: per control, the action the manager ran when it was clicked
//   (`updateData` → `perform`, `executeAction`), with the app state it
//   returned for the keys that action changes;
// - `order`: the footer's controls in order, as the research page lists
//   them (site/content/research/ui-design-system.md §3.4).
//
// and crates/excali-ui/src/footer/footer.css: css/styles.scss,
// components/LayerUI.scss, components/Actions.scss,
// components/footer/FooterCenter.scss and css/app.scss compiled (expanded)
// as one entry, of which only the rules whose selectors name the footer's
// classes (FOOTER_CLASSES) are kept: a comma-separated selector list keeps
// just its footer selectors, an at-rule the rules kept inside it.
//
// The host's slots are markers: the centre tunnel renders
// `<template data-slot="FooterCenter">`, the welcome screen's help hint
// `<template data-slot="WelcomeScreenHelpHint">`.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "footer.json");
export const STYLESHEET = join("src", "footer", "footer.css");

export const STYLESHEETS = [
  "css/styles.scss",
  "components/LayerUI.scss",
  "components/Actions.scss",
  "components/footer/FooterCenter.scss",
  "css/app.scss",
];

/** The class names of the footer's elements (Footer.tsx, Actions.tsx
 * ZoomActions / UndoRedoActions / ExitZenModeButton, HelpButton.tsx, the
 * zoom PanelComponents' wrappers and buttons, Section's heading). */
export const FOOTER_CLASSES = [
  "layer-ui__wrapper__footer",
  "layer-ui__wrapper__footer-left",
  "layer-ui__wrapper__footer-right",
  "layer-ui__wrapper__footer-left--transition-left",
  "layer-ui__wrapper__footer-left--transition-bottom",
  "App-menu_bottom",
  "App-menu_bottom--transition-left",
  "zen-mode-transition",
  "transition-right",
  "zoom-actions",
  "zoom-button",
  "zoom-in-button",
  "zoom-out-button",
  "zoom-in-button-wrapper",
  "zoom-out-button-wrapper",
  "reset-zoom-button",
  "reset-zoom-button-wrapper",
  "undo-redo-buttons",
  "undo-button-container",
  "redo-button-container",
  "footer-center",
  "help-icon",
  "disable-zen-mode",
  "disable-zen-mode--visible",
  "visually-hidden",
];

const ENTRY = `
export { default as Footer } from "./packages/excalidraw/components/footer/Footer";
export { ActionManager } from "./packages/excalidraw/actions/manager";
export { actionZoomIn, actionZoomOut, actionResetZoom } from "./packages/excalidraw/actions/actionCanvas";
export { createUndoAction, createRedoAction } from "./packages/excalidraw/actions/actionHistory";
export { actionShortcuts } from "./packages/excalidraw/actions/actionMenu";
export { actionToggleZenMode } from "./packages/excalidraw/actions/actionToggleZenMode";
export { Emitter } from "./packages/common/src/emitter";
export * as icons from "./packages/excalidraw/components/icons";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks the footer and the action
// panels read; the shim answers them from globalThis.__ui. The tunnels are
// the host's slots (see the header).
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => "full",
      useExcalidrawAPI: () => globalThis.__ui.api,
      useExcalidrawAppState: () => globalThis.__ui.api.getAppState(),
      useExcalidrawSetAppState: () => () => {},
    };`,
  "packages/excalidraw/context/tunnels": `
    const React = require("react");
    const slot = (name) => () => React.createElement("template", { "data-slot": name });
    module.exports = {
      useTunnels: () => ({
        FooterCenterTunnel: { Out: slot("FooterCenter"), In: () => null },
        WelcomeScreenHelpHintTunnel: { Out: slot("WelcomeScreenHelpHint"), In: () => null },
      }),
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

// Packages the actions index reaches that the footer never runs (the
// command palette's search, image resizing, file access).
const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: footer.mjs [--check] [--out DIR]\n");
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

// -- cases --------------------------------------------------------------------

/**
 * A case: Footer's props (`showExitZenModeBtn`, `renderWelcomeScreen`,
 * `defaultUIEnabled`, `zoomUIEnabled`), app state keys over the defaults
 * (`zoom` as its value), `undo` / `redo`: the stack has entries, and
 * `navigation: false` for `props.interaction = false`.
 */
const CASES = [
  { name: "default" },
  { name: "zoom-min", appState: { zoom: 0.1 } },
  { name: "zoom-just-above-min", appState: { zoom: 0.11 } },
  { name: "zoom-half", appState: { zoom: 0.5 } },
  { name: "zoom-rounding-down", appState: { zoom: 1.234 } },
  { name: "zoom-rounding-half", appState: { zoom: 1.235 } },
  { name: "zoom-rounding-up", appState: { zoom: 0.996 } },
  // 12.5 exactly: toFixed takes the larger neighbour on a tie
  { name: "zoom-rounding-tie", appState: { zoom: 0.125 } },
  { name: "zoom-large", appState: { zoom: 12.5 } },
  { name: "zoom-just-below-max", appState: { zoom: 29.9 } },
  { name: "zoom-max", appState: { zoom: 30 } },
  { name: "undo", undo: true },
  { name: "redo", redo: true },
  { name: "undo-redo", undo: true, redo: true },
  { name: "zen-mode", appState: { zenModeEnabled: true } },
  { name: "zen-mode-exit-button", appState: { zenModeEnabled: true }, props: { showExitZenModeBtn: true } },
  { name: "exit-button-only", props: { showExitZenModeBtn: true } },
  { name: "view-mode", appState: { viewModeEnabled: true } },
  { name: "view-mode-zen", appState: { viewModeEnabled: true, zenModeEnabled: true }, props: { showExitZenModeBtn: true } },
  { name: "welcome-screen", props: { renderWelcomeScreen: true } },
  { name: "no-default-ui", props: { defaultUIEnabled: false } },
  { name: "no-default-ui-welcome", props: { defaultUIEnabled: false, renderWelcomeScreen: true } },
  { name: "no-zoom-ui", props: { zoomUIEnabled: false } },
  { name: "navigation-disabled", navigation: false },
  { name: "no-default-ui-navigation-disabled", props: { defaultUIEnabled: false }, navigation: false },
  { name: "no-default-ui-no-zoom-ui", props: { defaultUIEnabled: false, zoomUIEnabled: false } },
  {
    name: "no-ui-welcome",
    props: { defaultUIEnabled: false, zoomUIEnabled: false, renderWelcomeScreen: true },
  },
];

const DEFAULT_PROPS = {
  showExitZenModeBtn: false,
  renderWelcomeScreen: false,
  defaultUIEnabled: true,
  zoomUIEnabled: true,
};

// The viewport the click cases zoom around: its centre is (500 + 10, 400 + 20).
const VIEWPORT = { width: 1000, height: 800, offsetLeft: 10, offsetTop: 20, scrollX: 0, scrollY: 0 };

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

/** The static <svg> icons of icons.tsx (the footer uses no themed one). */
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
    return {
      tag: node.localName,
      attrs: sorted(attrs),
      style: styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

/** The editor for a case: app state, history, app and action manager. */
const editorFor = (up, c) => {
  const defaults = up.getDefaultAppState();
  const { zoom, ...rest } = c.appState ?? {};
  let state = { ...defaults, ...VIEWPORT, ...rest, zoom: { value: zoom ?? 1 } };
  const history = {
    isUndoStackEmpty: !c.undo,
    isRedoStackEmpty: !c.redo,
    onHistoryChangedEmitter: new up.Emitter(),
  };
  const performed = [];
  const props = {
    UIOptions: { canvasActions: {} },
    interaction: c.navigation === false ? false : undefined,
  };
  const app = {
    props,
    isNavigationEnabled: () => props.interaction !== false,
    isInteractionEnabled: () => props.interaction !== false,
    viewport: { isLockedTransitionPending: false },
    requestUnfollow: () => {},
    focusContainer: () => performed.push({ focusContainer: true }),
    editorInterface: { formFactor: "desktop" },
  };
  const api = {
    getAppState: () => state,
    onStateChange: () => () => {},
    isDestroyed: false,
  };
  const manager = new up.ActionManager(
    (result) => {
      if (result && result.appState) state = { ...state, ...result.appState };
      performed.push(result);
    },
    () => state,
    () => [],
    app,
  );
  // each action's perform records its name; the history's perform is the
  // history's (ex-513), so undo and redo return nothing here
  const ran = [];
  const recorded = (action) => ({
    ...action,
    perform(...args) {
      ran.push(action.name);
      return action.perform.apply(this, args);
    },
  });
  const history_ = (action) => ({ ...action, perform: () => ({ captureUpdate: "NEVER" }) });
  manager.registerAll(
    [
      up.actionZoomIn,
      up.actionZoomOut,
      up.actionResetZoom,
      up.actionShortcuts,
      up.actionToggleZenMode,
      history_(up.createUndoAction(history)),
      history_(up.createRedoAction(history)),
    ].map(recorded),
  );
  // the help button and the exit-zen button run the imported actions
  const execute = manager.executeAction.bind(manager);
  manager.executeAction = (action, ...rest) => {
    ran.push(action.name);
    return execute(action, ...rest);
  };
  return { app, api, manager, history, performed, ran, getState: () => state };
};

const mount = async (up, window, c, editor) => {
  const { React, act, createRoot, Footer } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  globalThis.__ui = {
    app: editor.app,
    api: editor.api,
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: "desktop", desktopUIMode: "full", userAgent: {}, isTouchScreen: false },
  };
  const root = createRoot(container);
  const render = () =>
    React.createElement(Footer, {
      appState: editor.getState(),
      actionManager: editor.manager,
      ...DEFAULT_PROPS,
      ...(c.props ?? {}),
    });
  await act(async () => root.render(render()));
  return { container, root };
};

const render = async (up, window, iconNames, c) => {
  const editor = editorFor(up, c);
  const { container, root } = await mount(up, window, c, editor);
  const dom = [...container.childNodes].map(makeTree(iconNames));
  await up.act(async () => root.unmount());
  return {
    name: c.name,
    props: { ...DEFAULT_PROPS, ...(c.props ?? {}) },
    appState: {
      zoom: c.appState?.zoom ?? 1,
      zenModeEnabled: c.appState?.zenModeEnabled ?? false,
      viewModeEnabled: c.appState?.viewModeEnabled ?? false,
    },
    undo: c.undo ?? false,
    redo: c.redo ?? false,
    navigation: c.navigation ?? true,
    dom,
  };
};

// -- clicks -------------------------------------------------------------------

/** The controls and the app state keys their actions change. */
const CONTROLS = [
  { control: "zoomOut", selector: ".zoom-out-button", keys: ["zoom", "scrollX", "scrollY"] },
  { control: "resetZoom", selector: ".reset-zoom-button", keys: ["zoom", "scrollX", "scrollY"] },
  { control: "zoomIn", selector: ".zoom-in-button", keys: ["zoom", "scrollX", "scrollY"] },
  { control: "undo", selector: "[data-testid=button-undo]", keys: [] },
  { control: "redo", selector: "[data-testid=button-redo]", keys: [] },
  { control: "help", selector: ".help-icon", keys: ["openDialog", "openMenu", "openPopup"] },
  { control: "exitZenMode", selector: ".disable-zen-mode", keys: ["zenModeEnabled"] },
];

const CLICK_CASES = [
  { name: "default", undo: true, redo: true },
  { name: "zoomed", appState: { zoom: 2.5, scrollX: 40, scrollY: -30 }, undo: true, redo: true },
  { name: "zen-mode", appState: { zenModeEnabled: true }, props: { showExitZenModeBtn: true }, undo: true, redo: true },
  {
    name: "help-open",
    appState: { openDialog: { name: "help" }, openMenu: "canvas", openPopup: "fontFamily" },
    undo: true,
    redo: true,
  },
  {
    name: "other-dialog-open",
    appState: { openDialog: { name: "imageExport" }, openMenu: "canvas", openPopup: "fontFamily" },
    undo: true,
    redo: true,
  },
];

const clicks = async (up, window) => {
  const out = [];
  for (const c of CLICK_CASES) {
    for (const { control, selector, keys } of CONTROLS) {
      const editor = editorFor(up, c);
      const { container, root } = await mount(up, window, c, editor);
      const button = container.querySelector(selector);
      if (!button) throw new Error(`${c.name}: no ${selector}`);
      await up.act(async () => button.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
      await up.act(async () => root.unmount());
      if (editor.ran.length !== 1) throw new Error(`${c.name} ${control}: ran ${editor.ran}`);
      const state = editor.getState();
      out.push({
        case: c.name,
        control,
        appState: {
          zoom: c.appState?.zoom ?? 1,
          scrollX: c.appState?.scrollX ?? 0,
          scrollY: c.appState?.scrollY ?? 0,
          zenModeEnabled: c.appState?.zenModeEnabled ?? false,
          openDialog: c.appState?.openDialog ?? null,
          openMenu: c.appState?.openMenu ?? null,
          openPopup: c.appState?.openPopup ?? null,
        },
        action: editor.ran[0],
        focusContainer: editor.performed.some((r) => r.focusContainer),
        result: Object.fromEntries(keys.map((k) => [k, k === "zoom" ? state.zoom.value : state[k]])),
      });
    }
  }
  return out;
};

// -- tooltips -----------------------------------------------------------------

/** The label each tooltip wrapper shows when hovered (after its delay). */
const tooltips = async (up, window) => {
  const c = { name: "tooltips", undo: true, redo: true };
  const editor = editorFor(up, c);
  const { container, root } = await mount(up, window, c, editor);
  const out = [];
  const wait = (ms) => new Promise((done) => setTimeout(done, ms));
  for (const wrapper of container.querySelectorAll(".excalidraw-tooltip-wrapper")) {
    const button = wrapper.querySelector("button");
    const over = new window.MouseEvent("pointerover", { bubbles: true, relatedTarget: window.document.body });
    await up.act(async () => {
      button.dispatchEvent(over);
      await wait(600);
    });
    const tip = window.document.querySelector(".excalidraw-tooltip");
    if (!tip || !tip.classList.contains("excalidraw-tooltip--visible")) {
      throw new Error(`no tooltip for ${button.outerHTML.slice(0, 80)}`);
    }
    out.push({
      wrapper: wrapper.className,
      button: button.getAttribute("aria-label"),
      label: tip.textContent,
    });
    const leave = new window.MouseEvent("pointerout", { bubbles: true, relatedTarget: window.document.body });
    await up.act(async () => {
      button.dispatchEvent(leave);
      await wait(400);
    });
  }
  await up.act(async () => root.unmount());
  return out;
};

// -- stylesheets --------------------------------------------------------------

const CLASS_RE = new RegExp(`\\.(${FOOTER_CLASSES.map((c) => c.replace(/[-_]/g, "\\$&")).join("|")})(?![\\w-])`);

/** Top-level `{...}` blocks of expanded CSS: `[prelude, body]`. */
const blocks = (css) => {
  const out = [];
  let i = 0;
  while (i < css.length) {
    const open = css.indexOf("{", i);
    if (open < 0) break;
    let depth = 1;
    let j = open + 1;
    while (depth > 0) {
      if (css[j] === "{") depth++;
      else if (css[j] === "}") depth--;
      j++;
    }
    const prelude = css
      .slice(i, open)
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .trim();
    out.push([prelude, css.slice(open + 1, j - 1)]);
    i = j;
  }
  return out;
};

/** A selector list's selectors (commas inside parentheses stay). */
const splitSelectors = (prelude) => {
  const out = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < prelude.length; i++) {
    if (prelude[i] === "(") depth++;
    else if (prelude[i] === ")") depth--;
    else if (prelude[i] === "," && depth === 0) {
      out.push(prelude.slice(start, i).trim());
      start = i + 1;
    }
  }
  out.push(prelude.slice(start).trim());
  return out;
};

/** The footer's rules of `css` (see the header), re-indented at `indent`. */
const footerRules = (css, indent = "") => {
  const out = [];
  for (const [prelude, body] of blocks(css)) {
    if (prelude.startsWith("@")) {
      if (prelude.startsWith("@keyframes") || prelude.startsWith("@font-face")) continue;
      const inner = footerRules(body, `${indent}  `);
      if (inner.length) out.push(`${indent}${prelude} {\n${inner.join("\n")}\n${indent}}`);
      continue;
    }
    const selectors = splitSelectors(prelude).filter((s) => CLASS_RE.test(s));
    if (!selectors.length) continue;
    const declarations = body
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => `${indent}  ${l}`);
    out.push(`${indent}${selectors.join(",\n" + indent)} {\n${declarations.join("\n")}\n${indent}}`);
  }
  return out;
};

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/footer.mjs; do not edit. The footer's\n" +
    " * rules (the selectors naming its classes) of upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${footerRules(compiled).join("\n\n")}\n`
  );
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = [
  "buttons.zoomIn",
  "buttons.zoomOut",
  "buttons.resetZoom",
  "buttons.undo",
  "buttons.redo",
  "buttons.exitZenMode",
  "helpDialog.title",
  "headings.canvasActions",
];

export const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
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
  // markup → icons.tsx export, from each static icon mounted by React
  const iconNames = new Map();
  for (const [name, value] of staticIcons(up.icons)) {
    const host = window.document.createElement("div");
    const root = up.createRoot(host);
    await up.act(async () => root.render(value));
    const markup = host.innerHTML;
    await up.act(async () => root.unmount());
    if (iconNames.has(markup)) continue; // an alias: the first export names it
    iconNames.set(markup, name);
  }
  const cases = [];
  for (const c of CASES) cases.push(await render(up, window, iconNames, c));
  const clickCases = await clicks(up, window);
  const tooltipLabels = await tooltips(up, window);
  window.close();
  const order = [];
  const walk = (n) => {
    if (typeof n !== "object" || n.icon) return;
    const cls = n.attrs.class ?? "";
    const id = n.attrs["data-testid"];
    if (cls.includes("zoom-out-button ")) order.push("zoomOut");
    else if (cls.includes("reset-zoom-button ")) order.push("resetZoom");
    else if (cls.includes("zoom-in-button ")) order.push("zoomIn");
    else if (id === "button-undo") order.push("undo");
    else if (id === "button-redo") order.push("redo");
    else if (n.tag === "template") order.push(n.attrs["data-slot"]);
    else if (cls === "help-icon") order.push("help");
    else if (cls.startsWith("disable-zen-mode")) order.push("exitZenMode");
    n.children.forEach(walk);
  };
  cases.find((c) => c.name === "welcome-screen").dom.forEach(walk);
  const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
  const fixture = {
    upstream: upstream.commit,
    locale,
    order,
    tooltips: tooltipLabels,
    cases,
    viewport: VIEWPORT,
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
    process.stderr.write(`footer: ${error.message}\n`);
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
      process.stderr.write("footer goldens are out of date: run node tools/goldens/footer.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`footer goldens up to date: ${Object.keys(files).length} files\n`);
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
