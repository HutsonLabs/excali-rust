#!/usr/bin/env node
// Phone layout goldens for excali-ui (ex-702): upstream's own MobileToolbar
// (packages/excalidraw/components/MobileToolbar.tsx, with ToolPopover.tsx
// and the tool buttons and popovers of Tools.tsx) and MobileMenu
// (MobileMenu.tsx) rendered by React 19.0.0 into jsdom 22.1.0 with
// radix-ui 1.4.3, as tools/goldens/toolbar.mjs renders the desktop
// toolbar, and their stylesheets compiled from upstream's SCSS with sass
// 1.51.0.
//
//   node tools/goldens/mobile-menu.mjs            write the fixture and CSS
//   node tools/goldens/mobile-menu.mjs --check    exit 1 if either is stale
//   node tools/goldens/mobile-menu.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/mobile-menu.json:
//
// - `locale`: the English strings the phone layout reads;
// - `toolbar`: per case the app state, props, the toolbar's measured width
//   (MobileToolbar reads its own getBoundingClientRect().width, which the
//   generator answers, as jsdom has no layout), the clicks made after the
//   first render (`click`: the data-testid of a popover trigger, or
//   "extra-tools" for the "…" trigger), and the DOM React leaves, in the
//   tree format of toolbar.mjs (icons as `{icon}`, radix's ids as
//   `radix-N`, the popper wrapper's layout style left out);
// - `order`: the toolbar entries of the case the research page lists
//   (site/content/research/ui-design-system.md §3.7) at the phone width of
//   mockup 06 (a 390 px editor: see `toolbarWidth` below);
// - `menu`: per case the app state and props of MobileMenu and its DOM,
//   with the host's slots as `<template data-slot>`: the sidebars
//   (`renderSidebars`), the welcome screen, the main menu and the sidebar
//   trigger (tunnels), MobileShapeActions (the styles panel's mobile row,
//   ported in excali_ui::styles_panel) and the toolbar (MobileToolbar, held
//   above), and `renderTopLeftUI` / `renderTopRightUI` when the case
//   passes one;
// - `toolbarWidth`: the toolbar's width in the bottom bar for editor
//   widths, from the compiled CSS below: `.App-bottom-bar` is
//   `calc(100% - 28px)` at most 450px wide and its island has 4px padding.
//
// and crates/excali-ui/src/mobile_menu/mobile_menu.css: MobileToolbar.scss,
// ToolPopover.scss and FixedSideContainer.scss compiled (expanded) as one
// entry, then the rules of css/styles.scss (compiled) the phone layout's
// classes select, in source order.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { badgeStyle, installDom, makeTree, staticIcons } from "./toolbar.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "mobile-menu.json");
export const STYLESHEET = join("src", "mobile_menu", "mobile_menu.css");

export const STYLESHEETS = [
  "components/MobileToolbar.scss",
  "components/ToolPopover.scss",
  "components/FixedSideContainer.scss",
];

/** The classes of MobileMenu whose rules styles.scss holds. */
export const LAYOUT_CLASSES = [
  "App-top-bar",
  "App-bottom-bar",
  "App-toolbar",
  "App-toolbar-content",
  "App-welcome-screen",
  "excalidraw-ui-top-left",
  "excalidraw-ui-top-right",
  "floating-status-stack",
  "scroll-back-to-content",
  "disable-view-mode",
];

const ENTRY = `
export { MobileToolbar } from "./packages/excalidraw/components/MobileToolbar";
export { MobileMenu } from "./packages/excalidraw/components/MobileMenu";
export * as icons from "./packages/excalidraw/components/icons";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { t } from "./packages/excalidraw/i18n";
export { default as DropdownMenu } from "./packages/excalidraw/components/dropdownMenu/DropdownMenu";
export { SCROLLBAR_WIDTH, SCROLLBAR_MARGIN } from "./packages/excalidraw/scene/scrollbars";
export { renderToStaticMarkup } from "react-dom/server.browser";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx supplies the hooks; the tunnels are the host's slots; the
// styles panel's mobile row is a slot (its tree is styles-panel.mjs's);
// ExitViewModeButton (Actions.tsx:930-942) is restated, as Actions.tsx's
// module graph (the action registry) is not bundled here.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => "mobile",
      useExcalidrawAppState: () => ({ theme: "light" }),
      useExcalidrawSetAppState: () => () => {},
    };`,
  "packages/excalidraw/context/tunnels": `
    const React = require("react");
    const slot = (name) => () => React.createElement("template", { "data-slot": name });
    const tunnel = (name) => ({ Out: slot(name), In: () => null });
    module.exports = {
      useTunnels: () => ({
        TTDDialogTriggerTunnel: tunnel("TTDDialogTrigger"),
        MainMenuTunnel: tunnel("MainMenu"),
        DefaultSidebarTriggerTunnel: tunnel("DefaultSidebarTrigger"),
        WelcomeScreenCenterTunnel: tunnel("WelcomeScreenCenter"),
      }),
    };`,
  "packages/excalidraw/components/Actions": `
    const React = require("react");
    module.exports = {
      MobileShapeActions: () => React.createElement("template", { "data-slot": "MobileShapeActions" }),
      ExitViewModeButton: () =>
        React.createElement("button", { type: "button", className: "disable-view-mode", onClick: () => {} }, globalThis.__ui.pencilIcon),
    };`,
  "packages/excalidraw/components/ViewportStatusFrame/ViewportStatusFrame": `module.exports = { ViewportStatusBadge: () => null };`,
  "packages/excalidraw/scene": `module.exports = { getScrollToContentState: () => ({}) };`,
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
  process.stderr.write("usage: mobile-menu.mjs [--check] [--out DIR]\n");
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

// MobileToolbar's thresholds (MIN_WIDTH 276, then +40 per tool shown
// outside the "…" menu): each and either side of it, 0 (the first render)
// and the widest bar.
const WIDTHS = [0, 275, 276, 315, 316, 355, 356, 395, 396, 442];

const LASSO = { preferredSelectionTool: { type: "lasso", initialized: true } };

/** The toolbar width in the bottom bar of a 390 px editor (mockup 06). */
const PHONE = 354;

/**
 * A toolbar case: `appState` keys over the defaults (activeTool as
 * `{type}`), `props` (activeTool: a host-forced tool, aiEnabled),
 * `plugins.diagramToCode`, the measured `width`, and `click`.
 */
const TOOLBAR_CASES = [
  ...WIDTHS.map((width) => ({ name: `width-${width}`, width })),
  { name: "phone", width: PHONE, appState: LASSO },
  ...TOOL_TYPES.map((type) => ({ name: `active-${type}`, width: PHONE, appState: { activeTool: { type } } })),
  ...["text", "image", "frame"].map((type) => ({ name: `narrow-active-${type}`, width: 0, appState: { activeTool: { type } } })),
  ...["text", "image", "frame"].map((type) => ({ name: `wide-active-${type}`, width: 442, appState: { activeTool: { type } } })),
  { name: "lasso-preferred", width: PHONE, appState: { ...LASSO, activeTool: { type: "lasso" } } },
  { name: "lasso-preferred-selection-active", width: PHONE, appState: { ...LASSO, activeTool: { type: "selection" } } },
  { name: "forced-rectangle", width: PHONE, appState: { activeTool: { type: "rectangle" } }, props: { activeTool: { type: "rectangle" } } },
  { name: "forced-frame-narrow", width: 0, appState: { activeTool: { type: "frame" } }, props: { activeTool: { type: "frame" } } },
  { name: "open-selection", width: PHONE, click: "toolbar-selection" },
  { name: "open-selection-lasso", width: PHONE, appState: { ...LASSO, activeTool: { type: "lasso" } }, click: "toolbar-selection" },
  { name: "open-freedraw", width: PHONE, appState: { activeTool: { type: "freedraw" } }, click: "toolbar-freedraw" },
  { name: "open-autoshape", width: PHONE, appState: { activeTool: { type: "autoshape" } }, click: "toolbar-freedraw" },
  { name: "open-rectangle", width: PHONE, appState: { activeTool: { type: "rectangle" } }, click: "toolbar-rectangle" },
  { name: "open-ellipse", width: PHONE, appState: { activeTool: { type: "ellipse" } }, click: "toolbar-rectangle" },
  { name: "open-arrow", width: PHONE, appState: { activeTool: { type: "arrow" } }, click: "toolbar-arrow" },
  { name: "open-line", width: PHONE, appState: { activeTool: { type: "line" } }, click: "toolbar-arrow" },
  { name: "open-forced-line", width: PHONE, appState: { activeTool: { type: "line" } }, props: { activeTool: { type: "line" } }, click: "toolbar-arrow" },
  ...[0, PHONE, 442].map((width) => ({ name: `extra-open-${width}`, width, click: "extra-tools" })),
  ...["text", "image", "frame", "stickynote", "embeddable", "laser", "autoshape", "bucketfill"].map((type) => ({
    name: `extra-open-active-${type}`,
    width: 0,
    appState: { activeTool: { type } },
    click: "extra-tools",
  })),
  { name: "extra-open-ai-disabled", width: PHONE, props: { aiEnabled: false }, click: "extra-tools" },
  { name: "extra-open-magic-frame", width: PHONE, plugins: { diagramToCode: true }, click: "extra-tools" },
  { name: "extra-open-magic-frame-ai-disabled", width: PHONE, props: { aiEnabled: false }, plugins: { diagramToCode: true }, click: "extra-tools" },
  { name: "extra-open-forced", width: 0, appState: { activeTool: { type: "laser" } }, props: { activeTool: { type: "laser" } }, click: "extra-tools" },
];

/**
 * A MobileMenu case: `appState` keys, MobileMenu's props (`renderWelcomeScreen`,
 * `defaultUIEnabled`, `scrollBackToContentUIEnabled`, `topLeftUI` /
 * `topRightUI` for a host render function returning a slot, `interaction`
 * false when app.isInteractionEnabled() is).
 */
const MENU_CASES = [
  { name: "default" },
  { name: "welcome-screen", props: { renderWelcomeScreen: true } },
  { name: "pen-detected", appState: { penDetected: true } },
  { name: "pen-mode", appState: { penDetected: true, penMode: true } },
  { name: "view-mode", appState: { viewModeEnabled: true } },
  { name: "view-mode-pen", appState: { viewModeEnabled: true, penDetected: true } },
  { name: "view-mode-no-interaction", appState: { viewModeEnabled: true }, props: { interaction: false } },
  { name: "no-default-ui", props: { defaultUIEnabled: false } },
  { name: "no-default-ui-view-mode", appState: { viewModeEnabled: true }, props: { defaultUIEnabled: false } },
  { name: "scrolled-outside", appState: { scrolledOutside: true } },
  { name: "scrolled-outside-menu-open", appState: { scrolledOutside: true, openMenu: "canvas" } },
  { name: "scrolled-outside-sidebar-open", appState: { scrolledOutside: true, openSidebar: { name: "default" } } },
  { name: "scrolled-outside-no-scroll-ui", appState: { scrolledOutside: true }, props: { scrollBackToContentUIEnabled: false } },
  { name: "scrolled-outside-view-mode", appState: { scrolledOutside: true, viewModeEnabled: true } },
  { name: "element-link-selector", appState: { openDialog: { name: "elementLinkSelector", sourceElementId: "x" } } },
  { name: "top-left-ui", props: { topLeftUI: true } },
  { name: "top-right-ui", props: { topRightUI: true } },
  { name: "top-right-ui-view-mode", appState: { viewModeEnabled: true }, props: { topRightUI: true } },
];

// -- rendering --------------------------------------------------------------

const stateFor = (up, c) => {
  const defaults = up.getDefaultAppState();
  const state = { ...defaults, ...(c.appState ?? {}) };
  state.activeTool = { ...defaults.activeTool, ...(c.appState?.activeTool ?? {}), locked: false };
  return state;
};

const appFor = (up, c, state) => ({
  state,
  props: { UIOptions: { tools: { image: true } }, isCollaborating: false, ...(c.props ?? {}) },
  plugins: { diagramToCode: c.plugins?.diagramToCode ? {} : undefined },
  scene: { getNonDeletedElementsMap: () => new Map() },
  onPointerDownEmitter: { on: () => () => {} },
  isInteractionEnabled: () => c.props?.interaction !== false,
  setActiveTool: () => {},
  togglePenMode: () => {},
  setOpenDialog: () => {},
  onMagicframeToolSelect: () => {},
  toolDrag: { handleButtonPointerDown: () => {} },
});

const mount = (window) => {
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw excalidraw--mobile";
  document.body.appendChild(container);
  return container;
};

const click = async (up, window, el) => {
  const opts = { bubbles: true, cancelable: true, button: 0, pointerType: "touch" };
  await up.act(async () => {
    el.dispatchEvent(new window.MouseEvent("pointerdown", opts));
    el.dispatchEvent(new window.MouseEvent("pointerup", opts));
    el.dispatchEvent(new window.MouseEvent("click", opts));
  });
};

const renderToolbar = async (up, window, trees, c) => {
  const { React, act, createRoot, MobileToolbar, UIAppStateContext } = up;
  const container = mount(window);
  globalThis.__toolbarWidth = c.width;
  const state = stateFor(up, c);
  const app = appFor(up, c, state);
  const root = createRoot(container);
  const element = React.createElement(
    UIAppStateContext.Provider,
    { value: state },
    React.createElement(MobileToolbar, { app, setAppState: () => {} }),
  );
  await act(async () => root.render(element));
  if (c.click) {
    const selector =
      c.click === "extra-tools" ? ".App-toolbar__extra-tools-trigger" : `[data-testid="${c.click}"]`;
    const trigger = container.querySelector(selector);
    if (!trigger) throw new Error(`${c.name}: no ${selector}`);
    await click(up, window, trigger);
  }
  const dom = [...container.childNodes].map(trees());
  await act(async () => root.unmount());
  return {
    name: c.name,
    appState: c.appState ?? {},
    props: c.props ?? {},
    plugins: c.plugins ?? {},
    width: c.width,
    click: c.click ?? null,
    dom,
  };
};

const renderMenu = async (up, window, trees, c) => {
  const { React, act, createRoot, MobileMenu, UIAppStateContext } = up;
  const container = mount(window);
  globalThis.__toolbarWidth = PHONE;
  const state = stateFor(up, c);
  const app = appFor(up, c, state);
  const props = c.props ?? {};
  const slot = (name) => () => React.createElement("template", { "data-slot": name });
  const root = createRoot(container);
  const element = React.createElement(
    UIAppStateContext.Provider,
    { value: state },
    React.createElement(MobileMenu, {
      app,
      appState: state,
      elements: [],
      actionManager: {},
      renderJSONExportDialog: () => null,
      renderImageExportDialog: () => null,
      setAppState: () => {},
      onPenModeToggle: () => {},
      renderTopLeftUI: props.topLeftUI ? slot("TopLeftUI") : undefined,
      renderTopRightUI: props.topRightUI ? slot("TopRightUI") : undefined,
      renderSidebars: slot("Sidebars"),
      renderWelcomeScreen: props.renderWelcomeScreen ?? false,
      defaultUIEnabled: props.defaultUIEnabled ?? true,
      scrollBackToContentUIEnabled: props.scrollBackToContentUIEnabled ?? true,
    }),
  );
  await act(async () => root.render(element));
  // the toolbar is held by `toolbar`: here it is a slot
  for (const t of container.querySelectorAll(".mobile-toolbar")) {
    const s = window.document.createElement("template");
    s.setAttribute("data-slot", "MobileToolbar");
    t.replaceWith(s);
  }
  const dom = [...container.childNodes].map(trees());
  await act(async () => root.unmount());
  return { name: c.name, appState: c.appState ?? {}, props, dom };
};

// -- stylesheets --------------------------------------------------------------

/** The rules of `css` (top level, in order) whose selector names a class
 * of `classes`, as sass wrote them. */
const rulesFor = (css, classes) => {
  const out = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < css.length; i++) {
    const ch = css[i];
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) {
        const rule = css.slice(start, i + 1).trim();
        start = i + 1;
        const selector = rule.slice(0, rule.indexOf("{"));
        if (selector.startsWith("@")) continue;
        const names = [...selector.matchAll(/\.([A-Za-z0-9_-]+)/g)].map((m) => m[1]);
        if (names.some((n) => classes.includes(n))) out.push(rule);
      }
    }
  }
  return out;
};

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  const styles = sass.compile(join(root, "css", "styles.scss"), { loadPaths: [root], style: "expanded" }).css;
  const layout = rulesFor(styles, LAYOUT_CLASSES);
  return (
    "/* Generated by tools/goldens/mobile-menu.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order, then the top-level rules of\n" +
    " * packages/excalidraw/css/styles.scss (compiled alike) that select\n" +
    ` * ${LAYOUT_CLASSES.map((c) => `.${c}`).join(", ")}. */\n` +
    `${compiled}\n\n${layout.join("\n\n")}\n`
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
  "toolBar.penMode",
  "toolBar.extraTools",
  "toolBar.mermaidToExcalidraw",
  "buttons.scrollBackToContent",
];

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  // radix's Popover focus scope walks the content's tabbables
  globalThis.NodeFilter = window.NodeFilter;
  globalThis.HTMLInputElement = window.HTMLInputElement;
  globalThis.HTMLButtonElement = window.HTMLButtonElement;
  // MobileToolbar measures itself (MobileToolbar.tsx:163-168)
  const rect = window.HTMLElement.prototype.getBoundingClientRect;
  window.HTMLElement.prototype.getBoundingClientRect = function () {
    if (this.classList.contains("mobile-toolbar")) {
      const w = globalThis.__toolbarWidth;
      return { x: 0, y: 0, left: 0, top: 0, width: w, height: 36, right: w, bottom: 36 };
    }
    return rect.call(this);
  };
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    jsx: "automatic",
    define: { "import.meta.env.MODE": '"production"' },
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
  globalThis.__ui = {
    container: { container: null, id: "excalidraw-id" },
    editorInterface: { formFactor: "phone", desktopUIMode: "full", userAgent: {}, isTouchScreen: true, canFitSidebar: false },
    pencilIcon: up.icons.pencilIcon,
  };
  const badge = badgeStyle(up);
  const trees = () => makeTree(iconNames, new Map(), badge);
  const toolbar = [];
  for (const c of TOOLBAR_CASES) toolbar.push(await renderToolbar(up, window, trees, c));
  const menu = [];
  for (const c of MENU_CASES) menu.push(await renderMenu(up, window, trees, c));
  window.close();
  const order = [];
  const walk = (n) => {
    if (typeof n !== "object" || n.icon) return;
    const id = n.attrs["data-testid"];
    if (id?.startsWith("toolbar-")) order.push(id.slice("toolbar-".length));
    else if (n.attrs.class?.includes("App-toolbar__extra-tools-trigger")) order.push("extra-tools");
    n.children.forEach(walk);
  };
  toolbar.find((c) => c.name === "phone").dom.forEach(walk);
  const toolbarWidth = [0, 320, 375, 390, 414, 478, 500, 599, 999].map((editor) => ({
    editor,
    toolbar: Math.max(0, Math.min(editor - 28, 450) - 8),
  }));
  const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
  const fixture = {
    upstream: upstream.commit,
    locale,
    scrollbar: { width: up.SCROLLBAR_WIDTH, margin: up.SCROLLBAR_MARGIN },
    order,
    toolbarWidth,
    toolbar,
    menu,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`mobile-menu: ${error.message}\n`);
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
      process.stderr.write("mobile-menu goldens are out of date: run node tools/goldens/mobile-menu.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`mobile-menu goldens up to date: ${Object.keys(files).length} files\n`);
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
