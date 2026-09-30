#!/usr/bin/env node
// Help dialog goldens for excali-ui (ex-522): upstream's own HelpDialog
// (packages/excalidraw/components/HelpDialog.tsx) in its Dialog and Modal,
// rendered by React 19.0.0 into jsdom 22.1.0 once per platform (isDarwin,
// isWindows, isFirefox and probablySupportsClipboardBlob are read from the
// navigator when the modules load, so each platform is a fresh load of the
// bundle), and HelpDialog.scss compiled with sass 1.51.0.
//
//   node tools/goldens/help-dialog.mjs            write the fixture and CSS
//   node tools/goldens/help-dialog.mjs --check    exit 1 if either is stale
//   node tools/goldens/help-dialog.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/help-dialog.json:
//
// - `locale`: the English strings the dialog reads (every `t("...")` key
//   of HelpDialog.tsx, from locales/en.json);
// - `cases`: per case the platform (`darwin`, `windows`, `firefox`,
//   `clipboardBlob`: the navigator's), whether the theme action is
//   enabled (`UIOptions.canvasActions.toggleTheme`, actionToggleTheme's
//   predicate), the form factor and theme, and the DOM React portals to
//   the body: `{tag, attrs, style, children}` with attributes and inline
//   style sorted by name, text as a string, and an icon's <svg> as
//   `{icon: name}` (the icons.tsx export whose markup it is);
// - `islands`: per case the three islands as the dialog shows them,
//   `{caption, rows: [{label, keys: [[key, ...], ...], or}]}`, read from
//   the DOM (each `kbd` a key, the text between key groups the separator);
// - `close`: the app state patches of closing the dialog with Escape and
//   with a click on the backdrop: Dialog's `setAppState({openMenu: null})`,
//   then LayerUI's `onClose` (`setAppState({openDialog: null})`,
//   LayerUI.tsx:577-583).
//
// and crates/excali-ui/src/help_dialog/help_dialog.css:
// components/HelpDialog.scss compiled (expanded).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "help-dialog.json");
export const STYLESHEET = join("src", "help_dialog", "help_dialog.css");

const ENTRY = `
export { HelpDialog } from "./packages/excalidraw/components/HelpDialog";
export { actionToggleTheme } from "./packages/excalidraw/actions/actionCanvas";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks HelpDialog and Dialog
// read; the shim answers them from globalThis.__ui. The action manager's
// isActionEnabled is upstream's (manager.tsx:237-245) over the app's props.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useExcalidrawSetAppState: () => (patch) => globalThis.__ui.setAppState(patch),
      useExcalidrawActionManager: () => ({
        isActionEnabled: (action) =>
          !action.predicate || action.predicate([], {}, globalThis.__ui.app.props, globalThis.__ui.app),
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
  "packages/excalidraw/components/LibraryMenu": `module.exports = { isLibraryMenuOpenAtom: { init: false } };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
};

// Packages the actions index reaches that the dialog never runs.
const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: help-dialog.mjs [--check] [--out DIR]\n");
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

// -- platforms and cases ------------------------------------------------------

const CHROME = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
const FIREFOX = "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0";

/** The navigators: `platform`, `userAgent`, Firefox's `window.netscape`,
 * and the async clipboard (`navigator.clipboard.write`, `ClipboardItem`). */
const PLATFORMS = [
  { name: "linux", platform: "Linux x86_64", userAgent: CHROME, clipboard: true },
  { name: "linux-no-clipboard", platform: "Linux x86_64", userAgent: CHROME, clipboard: false },
  { name: "darwin", platform: "MacIntel", userAgent: CHROME, clipboard: true },
  { name: "windows", platform: "Win32", userAgent: CHROME, clipboard: true },
  { name: "firefox", platform: "Linux x86_64", userAgent: FIREFOX, firefox: true, clipboard: false },
];

/** Per platform: the theme action enabled or not; on Linux also a phone
 * and the dark theme. */
const CASES = PLATFORMS.flatMap((p) => [
  { platform: p, toggleTheme: true },
  { platform: p, toggleTheme: false },
  ...(p.name === "linux"
    ? [
        { platform: p, toggleTheme: true, formFactor: "phone" },
        { platform: p, toggleTheme: true, theme: "dark" },
      ]
    : []),
]).map((c) => ({
  ...c,
  name: [
    c.platform.name,
    c.toggleTheme ? null : "no-theme-toggle",
    c.formFactor === "phone" ? "phone" : null,
    c.theme === "dark" ? "dark" : null,
  ]
    .filter(Boolean)
    .join("-"),
}));

// -- DOM ----------------------------------------------------------------------

const installDom = (p) => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  Object.defineProperty(window.navigator, "platform", { value: p.platform, configurable: true });
  Object.defineProperty(window.navigator, "userAgent", { value: p.userAgent, configurable: true });
  if (p.firefox) window.netscape = {};
  if (p.clipboard) {
    Object.defineProperty(window.navigator, "clipboard", { value: { write: () => {} }, configurable: true });
    window.ClipboardItem = class ClipboardItem {};
  }
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLDivElement: window.HTMLDivElement,
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

/** The islands as the dialog shows them (see the header). */
const islandsOf = (root) =>
  [...root.querySelectorAll(".HelpDialog__island")].map((island) => ({
    className: island.className,
    caption: island.querySelector(".HelpDialog__island-title").textContent,
    rows: [...island.querySelectorAll(".HelpDialog__shortcut")].map((row) => {
      const [label, keys] = row.children;
      const groups = [[]];
      const separators = [];
      for (const n of keys.childNodes) {
        if (n.nodeType === 1 && n.localName === "kbd") groups[groups.length - 1].push(n.textContent);
        else if (n.nodeType === 3) {
          separators.push(n.data);
          groups.push([]);
        }
      }
      // isOr={false} leaves "" between groups, which React writes as no node
      return { label: label.textContent, keys: groups, separators };
    }),
  }));

const mount = async (up, window, c) => {
  const { React, act, createRoot, HelpDialog } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const patches = [];
  globalThis.__ui = {
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: c.formFactor ?? "desktop" },
    app: { props: { UIOptions: { canvasActions: { toggleTheme: c.toggleTheme ? true : null } } } },
    setAppState: (patch) => patches.push(patch),
  };
  const root = createRoot(container);
  // LayerUI.tsx:577-583
  const onClose = () => patches.push({ openDialog: null });
  const element = React.createElement(
    up.UIAppStateContext.Provider,
    { value: { theme: c.theme ?? "light", openMenu: null } },
    React.createElement(HelpDialog, { onClose }),
  );
  await act(async () => root.render(element));
  return { root, patches, portal: document.body.lastElementChild };
};

const renderCase = async (up, window, iconNames, c) => {
  const { root, portal } = await mount(up, window, c);
  const dom = [makeTree(iconNames)(portal)];
  const islands = islandsOf(portal);
  await up.act(async () => root.unmount());
  return {
    case: {
      name: c.name,
      darwin: /Mac|iPod|iPhone|iPad/.test(c.platform.platform),
      windows: /^Win/.test(c.platform.platform),
      firefox: !!c.platform.firefox,
      clipboardBlob: !!c.platform.clipboard,
      toggleTheme: c.toggleTheme,
      formFactor: c.formFactor ?? "desktop",
      theme: c.theme ?? "light",
      dom,
    },
    islands: { name: c.name, islands },
  };
};

const closes = async (up, window) => {
  const c = { platform: PLATFORMS[0], toggleTheme: true };
  const out = [];
  for (const how of ["escape", "backdrop"]) {
    const { root, patches, portal } = await mount(up, window, c);
    await up.act(async () => {
      if (how === "escape") {
        portal
          .querySelector(".Modal")
          .dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      } else {
        portal
          .querySelector(".Modal__background")
          .dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true }));
      }
    });
    await up.act(async () => root.unmount());
    out.push({ how, patches });
  }
  return out;
};

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString('@use "components/HelpDialog";\n', {
    loadPaths: [root],
    style: "expanded",
  }).css;
  return (
    "/* Generated by tools/goldens/help-dialog.mjs; do not edit. Upstream's\n" +
    " * packages/excalidraw/components/HelpDialog.scss at the pin, compiled\n" +
    " * with sass 1.51.0 (expanded). */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

/** Every `t("...")` key HelpDialog.tsx reads, in source order. */
const localeKeys = (upstream) => {
  const source = readFileSync(join(upstream.dir, "packages", "excalidraw", "components", "HelpDialog.tsx"), "utf8");
  return [...new Set([...source.matchAll(/\bt\("([^"]+)"\)/g)].map((m) => m[1]))];
};

export const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
  const css = await stylesheet(upstream);
  const cases = [];
  const islands = [];
  let close;
  let locale;
  for (const p of PLATFORMS) {
    const window = installDom(p);
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
    for (const c of CASES.filter((c) => c.platform === p)) {
      const out = await renderCase(up, window, iconNames, c);
      cases.push(out.case);
      islands.push(out.islands);
    }
    if (p === PLATFORMS[0]) {
      close = await closes(up, window);
      locale = Object.fromEntries(localeKeys(upstream).map((k) => [k, up.t(k)]));
    }
    window.close();
  }
  const fixture = { upstream: upstream.commit, locale, cases, islands, close };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`help-dialog: ${error.message}\n`);
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
      process.stderr.write("help dialog goldens are out of date: run node tools/goldens/help-dialog.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`help dialog goldens up to date: ${Object.keys(files).length} files\n`);
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
