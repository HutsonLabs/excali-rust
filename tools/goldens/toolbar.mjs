#!/usr/bin/env node
// Shapes toolbar goldens for excali-ui (ex-518): upstream's own desktop
// Toolbar and its ExtraToolsDropdown (packages/excalidraw/components/
// Toolbar.tsx, tool buttons from Tools.tsx) rendered by React 19.0.0 (the
// version excalidraw-app pins) into jsdom 22.1.0 with radix-ui 1.4.3 (the
// version packages/excalidraw pins), and the toolbar's stylesheets compiled
// from upstream's SCSS with sass 1.51.0.
//
//   node tools/goldens/toolbar.mjs            write the fixture and CSS
//   node tools/goldens/toolbar.mjs --check    exit 1 if either is stale
//   node tools/goldens/toolbar.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/toolbar.json:
//
// - `locale`: the English strings the toolbar reads (locales/en.json);
// - `cases`: per case the app state, props and interface the toolbar is
//   rendered with, whether the extra-tools dropdown was opened (a click on
//   its trigger, as a user opens it), and the DOM React leaves in the
//   editor container: `{tag, attrs, style, children}` with attributes and
//   inline style sorted by name, text as a string, and three substitutions:
//   an icon's <svg> is `{icon: name}` (the icons.tsx export whose markup it
//   is; excali-ui's icon set holds that markup, tests/icons.rs), the ids
//   radix generates with React's useId are `radix-1`, `radix-2`... in the
//   order they first appear, and the style of radix's popper wrapper that
//   floating-ui computes from layout (jsdom has none) is left out;
//   the AI badge's inline style is the one React writes
//   (renderToStaticMarkup of DropdownMenu.Item.Badge), as jsdom 22's
//   CSSStyleDeclaration drops its `background` and `color` declarations,
//   whose values are var() references;
// - `order`: the tool entries of the default toolbar, in order, as the
//   research page lists them (site/content/research/ui-design-system.md
//   §3.1), from the rendered buttons' `data-testid`s.
//
// and crates/excali-ui/src/toolbar/toolbar.css: Toolbar.scss and
// dropdownMenu/DropdownMenu.scss compiled (expanded) as one entry.
//
// The host's slots are markers: the HintViewer (ex-528) renders
// `<template data-slot="HintViewer">`, the text-to-diagram trigger tunnel
// `<template data-slot="TTDDialogTrigger">`.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "toolbar.json");
export const STYLESHEET = join("src", "toolbar", "toolbar.css");

export const STYLESHEETS = ["components/Toolbar.scss", "components/dropdownMenu/DropdownMenu.scss"];

const ENTRY = `
export { Toolbar } from "./packages/excalidraw/components/Toolbar";
export * as icons from "./packages/excalidraw/components/icons";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { t } from "./packages/excalidraw/i18n";
export { default as DropdownMenu } from "./packages/excalidraw/components/dropdownMenu/DropdownMenu";
export { renderToStaticMarkup } from "react-dom/server.browser";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks the toolbar and the
// dropdown read; the shim answers them from globalThis.__ui. The tunnels'
// TTDDialogTrigger and the HintViewer are the host's slots (see the header).
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => globalThis.__ui.stylesPanelMode,
      useExcalidrawAppState: () => ({ theme: "light" }),
      useExcalidrawSetAppState: () => () => {},
    };`,
  "packages/excalidraw/context/tunnels": `
    const React = require("react");
    const slot = (name) => () => React.createElement("template", { "data-slot": name });
    module.exports = {
      useTunnels: () => ({ TTDDialogTriggerTunnel: { Out: slot("TTDDialogTrigger"), In: () => null } }),
    };`,
  "packages/excalidraw/components/HintViewer": `
    const React = require("react");
    module.exports = { HintViewer: () => React.createElement("template", { "data-slot": "HintViewer" }) };`,
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

const usage = () => {
  process.stderr.write("usage: toolbar.mjs [--check] [--out DIR]\n");
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

// Every tool type (types.ts ToolType), active in turn.
const TOOL_TYPES = [
  "selection",
  "lasso",
  "rectangle",
  "diamond",
  "ellipse",
  "arrow",
  "line",
  "freedraw",
  "text",
  "image",
  "eraser",
  "hand",
  "frame",
  "magicframe",
  "embeddable",
  "laser",
  "autoshape",
  "bucketfill",
  "stickynote",
];

/**
 * A case: `appState` keys over the defaults (activeTool as `{type,
 * locked}`), `props` (activeTool: a host-forced tool, isCollaborating,
 * aiEnabled, UIOptions.tools.image), `plugins.diagramToCode`, and `open`
 * to click the extra-tools trigger.
 */
const CASES = [
  { name: "default" },
  ...TOOL_TYPES.map((type) => ({ name: `active-${type}`, appState: { activeTool: { type } } })),
  { name: "locked", appState: { activeTool: { type: "rectangle", locked: true } } },
  { name: "pen-detected", appState: { penDetected: true } },
  { name: "pen-mode", appState: { penDetected: true, penMode: true } },
  { name: "pen-mode-undetected", appState: { penDetected: false, penMode: true } },
  { name: "zen-mode", appState: { zenModeEnabled: true } },
  { name: "preferred-lasso", appState: { preferredSelectionTool: { type: "lasso", initialized: true } } },
  {
    name: "preferred-lasso-active",
    appState: { activeTool: { type: "lasso" }, preferredSelectionTool: { type: "lasso", initialized: true } },
  },
  { name: "forced-rectangle", appState: { activeTool: { type: "rectangle" } }, props: { activeTool: { type: "rectangle" } } },
  { name: "forced-laser", appState: { activeTool: { type: "laser" } }, props: { activeTool: { type: "laser" } } },
  { name: "collaborating-laser", appState: { activeTool: { type: "laser" } }, props: { isCollaborating: true } },
  { name: "no-image-tool", props: { UIOptions: { tools: { image: false } } } },
  { name: "open", open: true },
  ...["image", "frame", "embeddable", "autoshape", "laser", "bucketfill", "lasso"].map((type) => ({
    name: `open-${type}`,
    appState: { activeTool: { type } },
    open: true,
  })),
  {
    name: "open-lasso-preferred",
    appState: { activeTool: { type: "lasso" }, preferredSelectionTool: { type: "lasso", initialized: true } },
    open: true,
  },
  { name: "open-forced", appState: { activeTool: { type: "frame" } }, props: { activeTool: { type: "frame" } }, open: true },
  { name: "open-collaborating-laser", appState: { activeTool: { type: "laser" } }, props: { isCollaborating: true }, open: true },
  { name: "open-no-image-tool", props: { UIOptions: { tools: { image: false } } }, open: true },
  { name: "open-ai-disabled", props: { aiEnabled: false }, open: true },
  { name: "open-magic-frame", plugins: { diagramToCode: true }, open: true },
  { name: "open-magic-frame-ai-disabled", props: { aiEnabled: false }, plugins: { diagramToCode: true }, open: true },
  { name: "open-zen-mode", appState: { zenModeEnabled: true }, open: true },
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
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  window.ResizeObserver = globals.ResizeObserver;
  return window;
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  return sorted(Array.from({ length: style.length }, (_, i) => style.item(i)).map((p) => [p, style.getPropertyValue(p)]));
};

/** The static <svg> icons of icons.tsx (the toolbar uses no themed one). */
const staticIcons = (icons) =>
  Object.entries(icons).filter(
    ([, value]) => value && value.$$typeof === Symbol.for("react.transitional.element") && value.type === "svg",
  );

/** An inline style attribute as React writes it → property → value. */
const parseStyle = (text) =>
  sorted(
    text
      .split(";")
      .filter(Boolean)
      .map((d) => {
        const at = d.indexOf(":");
        return [d.slice(0, at), d.slice(at + 1)];
      }),
  );

/** The AI badge's style, as React writes it (see the header). */
const badgeStyle = (up) => {
  const { React, DropdownMenu, renderToStaticMarkup } = up;
  const markup = renderToStaticMarkup(React.createElement(DropdownMenu.Item.Badge, null, "AI"));
  const m = markup.match(/^<div class="DropDownMenuItemBadge" style="([^"]*)">AI<\/div>$/);
  if (!m) throw new Error(`DropDownMenuItemBadge changed: ${markup}`);
  return parseStyle(m[1]);
};

/** The tree, with the substitutions of the header. */
const makeTree = (iconNames, ids, badge) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (!name) throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      return { icon: name };
    }
    const attrs = [...node.attributes]
      .filter((a) => a.name !== "style")
      .map((a) => [a.name, a.name === "id" || a.name.startsWith("aria-") ? renameIds(a.value, ids) : a.value]);
    const popper = node.hasAttribute("data-radix-popper-content-wrapper");
    return {
      tag: node.localName,
      attrs: sorted(attrs),
      style: popper ? {} : node.classList.contains("DropDownMenuItemBadge") ? checkedBadge(styleOf(node), badge) : styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

/** The badge style React writes, which must hold what jsdom kept. */
const checkedBadge = (kept, badge) => {
  for (const [k, v] of Object.entries(kept)) {
    if (badge[k] !== v) throw new Error(`the AI badge's ${k} is ${v} in jsdom, ${badge[k]} in React's markup`);
  }
  return badge;
};

/** React's useId ids (`«r0»` in React 19.0, `:r0:` before) → radix-N. */
const renameIds = (value, ids) =>
  value.replace(/radix-(«r[0-9a-z]+»|:r[0-9a-z]+:)/g, (id) => {
    if (!ids.has(id)) ids.set(id, `radix-${ids.size + 1}`);
    return ids.get(id);
  });

const appFor = (up, c) => {
  const defaults = up.getDefaultAppState();
  const state = { ...defaults, ...(c.appState ?? {}) };
  state.activeTool = {
    ...defaults.activeTool,
    ...(c.appState?.activeTool ?? {}),
    locked: c.appState?.activeTool?.locked ?? false,
  };
  const props = { UIOptions: { tools: { image: true } }, isCollaborating: false, ...(c.props ?? {}) };
  const app = {
    state,
    props,
    plugins: { diagramToCode: c.plugins?.diagramToCode ? {} : undefined },
    setActiveTool: () => {},
    togglePenMode: () => {},
    setOpenDialog: () => {},
    onMagicframeToolSelect: () => {},
    toolDrag: { handleButtonPointerDown: () => {} },
  };
  return { app, state, props };
};

const render = async (up, window, iconNames, badge, c) => {
  const { React, act, createRoot, Toolbar, UIAppStateContext } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  globalThis.__ui = {
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: "desktop", desktopUIMode: "full", userAgent: {}, isTouchScreen: false, canFitSidebar: true },
    stylesPanelMode: "full",
  };
  const { app, state, props } = appFor(up, c);
  const root = createRoot(container);
  const element = React.createElement(
    UIAppStateContext.Provider,
    { value: state },
    React.createElement(Toolbar, {
      app,
      appState: state,
      setAppState: () => {},
      UIOptions: props.UIOptions,
      onPenModeToggle: () => {},
      onLockToggle: () => {},
      heading: null,
    }),
  );
  await act(async () => root.render(element));
  if (c.open) {
    const trigger = container.querySelector(".App-toolbar__extra-tools-trigger");
    await act(async () => trigger.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
  }
  const ids = new Map();
  const tree = makeTree(iconNames, ids, badge);
  const dom = [...container.childNodes].map(tree);
  await act(async () => root.unmount());
  return {
    name: c.name,
    appState: c.appState ?? {},
    props: c.props ?? {},
    plugins: c.plugins ?? {},
    open: c.open ?? false,
    dom,
  };
};

// -- stylesheets --------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/toolbar.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = [
  "toolBar.hand",
  "toolBar.selection",
  "toolBar.lasso",
  "toolBar.rectangle",
  "toolBar.diamond",
  "toolBar.ellipse",
  "toolBar.arrow",
  "toolBar.line",
  "toolBar.freedraw",
  "toolBar.text",
  "toolBar.stickynote",
  "toolBar.image",
  "toolBar.eraser",
  "toolBar.frame",
  "toolBar.embeddable",
  "toolBar.autoshape",
  "toolBar.laser",
  "toolBar.bucketfill",
  "toolBar.magicframe",
  "toolBar.lock",
  "toolBar.penMode",
  "toolBar.extraTools",
  "toolBar.mermaidToExcalidraw",
  "helpDialog.or",
  "keys.shift",
];

export const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
  const css = await stylesheet(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    jsx: "automatic",
    define: { "import.meta.env.MODE": '"production"' },
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
  const badge = badgeStyle(up);
  for (const c of CASES) cases.push(await render(up, window, iconNames, badge, c));
  window.close();
  const order = [];
  const walk = (n) => {
    if (typeof n !== "object" || n.icon) return;
    const id = n.attrs["data-testid"];
    if (id?.startsWith("toolbar-")) order.push(id.slice("toolbar-".length));
    else if (n.attrs.class?.includes("App-toolbar__divider")) order.push("|");
    else if (n.attrs.class?.includes("App-toolbar__extra-tools-trigger")) order.push("extra-tools");
    n.children.forEach(walk);
  };
  cases[0].dom.forEach(walk);
  const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
  const fixture = { upstream: upstream.commit, locale, order, cases };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`toolbar: ${error.message}\n`);
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
      process.stderr.write("toolbar goldens are out of date: run node tools/goldens/toolbar.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`toolbar goldens up to date: ${Object.keys(files).length} files\n`);
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
