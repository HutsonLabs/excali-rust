#!/usr/bin/env node
// Context menu goldens for excali-ui (ex-525): upstream's own ContextMenu
// (packages/excalidraw/components/ContextMenu.tsx) inside its Popover
// (components/Popover.tsx), fed the items App.getContextMenuItems returns
// (components/App.tsx:13835-13936, read from the pinned source and run
// against upstream's own actions), run from the pinned checkout under
// Node, and ContextMenu.scss compiled with sass 1.51.0.
//
//   node tools/goldens/context-menu.mjs            write the fixture and CSS
//   node tools/goldens/context-menu.mjs --check    exit 1 if either is stale
//   node tools/goldens/context-menu.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/context-menu.json:
//
// - `cases`: per case the menu type (`canvas` or `element`), the form
//   factor, the scene (`elements`, JSON as upstream holds them, built by
//   API.createElement), the app state keys over getDefaultAppState(),
//   `top` and `left`, the items getContextMenuItems returned (action names,
//   `|` for CONTEXT_MENU_SEPARATOR) and the tree ContextMenu renders:
//   `{ tag, class?, attrs?, children?, on? }` for a DOM element and a
//   string for text. `on` maps an event to what its handler does:
//   `{ close: true }` (onClose), `{ executeAction, source }` (the callback
//   onClose runs once the menu state is cleared) and `{ preventDefault:
//   true }`;
// - `locale`: the English strings (locales/en.json) of every label the
//   menus' actions can show: each static label, and each locale key
//   literal in a label function's source.
//
// App.getContextMenuItems is a private method of the whole editor; its
// source is taken from App.tsx at the pin (the `: ContextMenuItems`
// annotations dropped) and called with `this` holding the case's
// `state.viewModeEnabled` and `editorInterface.formFactor`, its
// identifiers resolved to upstream's actions (actions/index.ts) and
// CONTEXT_MENU_SEPARATOR.
//
// JSX is evaluated by a stand-in runtime (react/jsx-runtime shimmed, as in
// main-menu.mjs) that calls function components; effects do not run, so
// the Popover's focus, Tab trap, viewport fit and outside-press close
// (excali_ui::primitives::popover, tests/ui_primitives.rs) are not
// recorded here.
//
// The browser has the async clipboard (navigator.clipboard.writeText and
// write, ClipboardItem, canvas toBlob), so the copy-as-PNG/SVG and
// copy-text predicates can hold. Deterministic: upstream runs in its test
// mode (ids id0.., timestamps 1); `isDarwin` is false (jsdom's navigator),
// so shortcuts read Ctrl.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "context-menu.json");
export const STYLESHEET = join("src", "context_menu.css");

export const STYLESHEETS = ["components/ContextMenu.scss"];

const ENTRY = `
export { ContextMenu, CONTEXT_MENU_SEPARATOR } from "./packages/excalidraw/components/ContextMenu";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { t } from "./packages/excalidraw/i18n";
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
`;

// Packages the actions index reaches that the menu never calls.
const STUBS = ["fuzzy", "pica", "browser-fs-access", "image-blob-reduce"];

const FRAGMENT = Symbol.for("excali-rust.fragment");

const SHIMS = {
  "react/jsx-runtime": `const F = Symbol.for("excali-rust.fragment");
const jsx = (type, props, key) => ({ type, props });
module.exports = { jsx, jsxs: jsx, Fragment: F };`,
  react: `const F = Symbol.for("excali-rust.fragment");
const id = (f) => f;
const createElement = (type, props, ...children) => ({ type, props: { ...(props ?? {}), ...(children.length ? { children: children.length === 1 ? children[0] : children } : {}) } });
const createContext = (value) => ({ current: value, Provider: {} });
const R = {
  Fragment: F,
  createElement,
  createContext,
  forwardRef: id,
  memo: id,
  useCallback: id,
  useContext: (ctx) => ctx.current,
  useEffect: () => {},
  useLayoutEffect: () => {},
  useMemo: (f) => f(),
  useRef: (v) => ({ current: v }),
  useState: (v) => [typeof v === "function" ? v() : v, () => {}],
};
module.exports = { ...R, default: R };`,
  "react-dom": `module.exports = { unstable_batchedUpdates: (f) => f(), flushSync: (f) => f(), createPortal: (c) => c };`,
  clsx: `const c = (...args) => {
  const out = [];
  for (const x of args) {
    if (!x) continue;
    if (typeof x === "string" || typeof x === "number") out.push(String(x));
    else if (Array.isArray(x)) { const s = c(...x); if (s) out.push(s); }
    else if (typeof x === "object") for (const k of Object.keys(x)) if (x[k]) out.push(k);
  }
  return out.join(" ");
};
module.exports = c; module.exports.default = c; module.exports.clsx = c;`,
  // The editor's hooks, answered from globalThis.__menu.
  "packages/excalidraw/components/App": `module.exports = {
  useExcalidrawAppState: () => globalThis.__menu.appState,
  useExcalidrawElements: () => globalThis.__menu.elements,
  useApp: () => globalThis.__menu.app,
  useEditorInterface: () => globalThis.__menu.app.editorInterface,
  useExcalidrawContainer: () => ({ container: null, id: "excalidraw-id" }),
  useExcalidrawSetAppState: () => () => {},
  useExcalidrawActionManager: () => null,
  useAppProps: () => globalThis.__menu.app.props,
  useStylesPanelMode: () => "full",
};`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
  "packages/excalidraw/editor-jotai": `const atom = (init) => ({ init });
module.exports = {
  atom,
  useSetAtom: () => () => {},
  useAtomValue: (a) => a.init,
  useAtom: (a) => [a.init, () => {}],
  editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
};`,
};

const usage = () => {
  process.stderr.write("usage: context-menu.mjs [--check] [--out DIR]\n");
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

// -- scenes ---------------------------------------------------------------------------

/** API.createElement, then the attributes it does not take (`link`,
 * `autoResize`) set on the result. */
const create = (up, { link, autoResize, ...opts }) => {
  const element = apiCreateElement(up, opts);
  return {
    ...element,
    ...(link === undefined ? {} : { link }),
    ...(autoResize === undefined ? {} : { autoResize }),
  };
};

const rect = (up, id, x, y, rest = {}) => create(up, { type: "rectangle", id, x, y, ...rest });

const text = (up, id, x, y, rest = {}) => create(up, { type: "text", id, x, y, text: "hello", ...rest });

/** A rectangle with a bound label. */
const labelled = (up, id) => [
  rect(up, id, 0, 0, { width: 200, height: 100, boundElements: [{ type: "text", id: `${id}-label` }] }),
  text(up, `${id}-label`, 50, 40, { containerId: id, text: "label" }),
];

const selecting = (...ids) => ({ selectedElementIds: Object.fromEntries(ids.map((id) => [id, true])) });

/**
 * [id, type, { scene(up), state, formFactor, top, left }]: the menus of
 * research/ui-design-system.md 3.10 over the states their predicates,
 * labels and checkmarks read.
 */
const CASES = [
  ["canvas-empty", "canvas", {}],
  ["canvas-scene", "canvas", { scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)] }],
  [
    "canvas-locked",
    "canvas",
    { scene: (up) => [rect(up, "r1", 0, 0, { locked: true }), rect(up, "r2", 200, 0)] },
  ],
  [
    "canvas-toggles-on",
    "canvas",
    {
      scene: (up) => [rect(up, "r1", 0, 0)],
      state: {
        gridModeEnabled: true,
        objectsSnapModeEnabled: true,
        zenModeEnabled: true,
        stats: { open: true, panels: 3 },
        bindingPreference: "disabled",
        isMidpointSnappingEnabled: false,
      },
    },
  ],
  ["canvas-view-mode", "canvas", { scene: (up) => [rect(up, "r1", 0, 0)], state: { viewModeEnabled: true } }],
  ["canvas-view-mode-empty", "canvas", { state: { viewModeEnabled: true, gridModeEnabled: true } }],
  ["canvas-phone", "canvas", { formFactor: "phone", scene: (up) => [rect(up, "r1", 0, 0)] }],
  ["element-rectangle", "element", { scene: (up) => [rect(up, "r1", 0, 0)], state: selecting("r1") }],
  [
    "element-rectangle-phone",
    "element",
    { formFactor: "phone", scene: (up) => [rect(up, "r1", 0, 0)], state: selecting("r1") },
  ],
  [
    "element-rectangle-tablet",
    "element",
    { formFactor: "tablet", scene: (up) => [rect(up, "r1", 0, 0)], state: selecting("r1") },
  ],
  [
    "element-two",
    "element",
    { scene: (up) => [rect(up, "r1", 0, 0), rect(up, "r2", 200, 0)], state: selecting("r1", "r2") },
  ],
  [
    "element-group",
    "element",
    {
      scene: (up) => [rect(up, "r1", 0, 0, { groupIds: ["g1"] }), rect(up, "r2", 200, 0, { groupIds: ["g1"] })],
      state: { ...selecting("r1", "r2"), selectedGroupIds: { g1: true } },
    },
  ],
  ["element-text", "element", { scene: (up) => [text(up, "t1", 0, 0)], state: selecting("t1") }],
  [
    "element-text-fixed-width",
    "element",
    { scene: (up) => [text(up, "t1", 0, 0, { autoResize: false })], state: selecting("t1") },
  ],
  ["element-labelled", "element", { scene: (up) => labelled(up, "c1"), state: selecting("c1") }],
  [
    "element-container-and-text",
    "element",
    { scene: (up) => [rect(up, "r1", 0, 0), text(up, "t1", 300, 0)], state: selecting("r1", "t1") },
  ],
  [
    "element-frame",
    "element",
    {
      scene: (up) => [
        rect(up, "r1", 20, 20, { frameId: "f1" }),
        create(up, { type: "frame", id: "f1", x: 0, y: 0, width: 300, height: 300 }),
      ],
      state: selecting("f1"),
    },
  ],
  [
    "element-image",
    "element",
    {
      scene: (up) => [create(up, { type: "image", id: "i1", x: 0, y: 0, fileId: "file1", status: "saved" })],
      state: selecting("i1"),
    },
  ],
  [
    "element-line",
    "element",
    {
      scene: (up) => [create(up, { type: "line", id: "l1", x: 0, y: 0, width: 100, height: 100 })],
      state: selecting("l1"),
    },
  ],
  [
    "element-arrow",
    "element",
    {
      scene: (up) => [create(up, { type: "arrow", id: "a1", x: 0, y: 0, width: 100, height: 100 })],
      state: selecting("a1"),
    },
  ],
  [
    "element-locked",
    "element",
    { scene: (up) => [rect(up, "r1", 0, 0, { locked: true })], state: selecting("r1") },
  ],
  [
    "element-link",
    "element",
    { scene: (up) => [rect(up, "r1", 0, 0, { link: "https://example.com" })], state: selecting("r1") },
  ],
  [
    "element-embeddable",
    "element",
    {
      scene: (up) => [create(up, { type: "embeddable", id: "e1", x: 0, y: 0, link: "https://example.com" })],
      state: selecting("e1"),
    },
  ],
  [
    "element-view-mode",
    "element",
    { scene: (up) => [text(up, "t1", 0, 0)], state: { ...selecting("t1"), viewModeEnabled: true } },
  ],
  [
    "element-at-edge",
    "element",
    { scene: (up) => [rect(up, "r1", 0, 0)], state: selecting("r1"), top: 700, left: 1000 },
  ],
];

// -- App.getContextMenuItems -------------------------------------------------------------

/** The method's body from App.tsx, the TypeScript annotations dropped. */
const contextMenuItemsSource = (upstream) => {
  const app = readFileSync(join(upstream.dir, "packages", "excalidraw", "components", "App.tsx"), "utf8");
  const start = app.indexOf("  private getContextMenuItems = (");
  if (start < 0) throw new Error("App.tsx: getContextMenuItems not found");
  const open = app.indexOf("=> {\n", start) + 3;
  const end = app.indexOf("\n  };\n", open);
  const body = app.slice(open, end + 4);
  return body.replace(/: ContextMenuItems/g, "");
};

/**
 * App.tsx's value imports from its actions modules: [name, module index]
 * with the modules, so the method's identifiers resolve as in App.tsx.
 */
const appActionImports = (upstream) => {
  const app = readFileSync(join(upstream.dir, "packages", "excalidraw", "components", "App.tsx"), "utf8");
  const modules = [];
  const names = [];
  for (const [, list, from] of app.matchAll(/^import \{([^}]*)\} from "\.\.\/(actions[^"]*)";$/gm)) {
    const i = modules.push(from) - 1;
    for (const name of list.split(",").map((n) => n.trim()).filter(Boolean)) names.push([name, i]);
  }
  return { modules, names };
};

const itemsFunction = (up, source, actionImports) => {
  const scope = { CONTEXT_MENU_SEPARATOR: up.CONTEXT_MENU_SEPARATOR };
  for (const [name, i] of actionImports) scope[name] = up[`actions${i}`][name];
  // eslint-disable-next-line no-new-func
  const fn = new Function("scope", "type", `with (scope) { return (() => ${source})(); }`);
  return (self, type) => fn.call(self, scope, type);
};

// The editor's size (App sets appState.width and height from its
// container; getDefaultAppState leaves them out).
const VIEWPORT = { width: 1024, height: 768 };

// -- running upstream -----------------------------------------------------------------

const flatten = (node, ctx) => {
  if (node === null || node === undefined || node === false || node === true) return [];
  if (Array.isArray(node)) return node.flatMap((n) => flatten(n, ctx));
  if (typeof node === "string" || typeof node === "number") return [String(node)];
  const { type, props } = node;
  if (type === FRAGMENT) return flatten(props.children, ctx);
  if (typeof type === "function") return flatten(type(props), ctx);
  if (typeof type !== "string") throw new Error(`unexpected element type ${String(type)}`);
  const out = { tag: type };
  if (props.className) out.class = props.className;
  const attrs = {};
  const on = {};
  for (const [k, v] of Object.entries(props)) {
    if (["children", "className", "style", "ref", "key"].includes(k) || v === undefined || v === null) continue;
    if (typeof v === "function" && /^on[A-Z]/.test(k)) {
      const event = k.slice(2).toLowerCase();
      on[event] = null;
      ctx.handlers.push({ on, event, handler: v });
      continue;
    }
    if (k === "tabIndex") {
      attrs.tabindex = String(v);
      continue;
    }
    if (typeof v !== "string") throw new Error(`<${type}> ${k} is not a string`);
    attrs[k] = v;
  }
  if (Object.keys(attrs).length) out.attrs = attrs;
  if (props.style) throw new Error(`<${type}> has an inline style`);
  const children = flatten(props.children, ctx);
  if (children.length) out.children = children;
  if (Object.keys(on).length) out.on = on;
  return [out];
};

const render = (up, element) => {
  const ctx = { handlers: [] };
  const tree = flatten(element, ctx);
  for (const h of ctx.handlers) {
    const effects = [];
    globalThis.__menu.effect = (e) => effects.push(e);
    const event = { preventDefault: () => effects.push({ preventDefault: true }) };
    h.handler(event);
    h.on[h.event] = effects;
  }
  return tree;
};

const runCase = (up, getItems, [id, type, c]) => {
  up.reseed(1);
  const elements = c.scene ? c.scene(up) : [];
  const appState = { ...up.getDefaultAppState(), ...VIEWPORT, ...(c.state ?? {}) };
  const formFactor = c.formFactor ?? "desktop";
  const scene = new up.Scene(elements, { skipValidation: true });
  const app = {
    props: { UIOptions: { canvasActions: {} } },
    scene,
    editorInterface: { formFactor },
    isInteractionEnabled: () => true,
    isNavigationEnabled: () => true,
    state: appState,
  };
  const items = getItems({ state: appState, editorInterface: app.editorInterface }, type);
  const actionManager = {
    app,
    executeAction: (action, source) =>
      globalThis.__menu.effect({ executeAction: action.name, source }),
  };
  globalThis.__menu = { appState, elements: scene.getNonDeletedElements(), app };
  const top = c.top ?? 100;
  const left = c.left ?? 200;
  const tree = render(up, {
    type: up.ContextMenu,
    props: {
      actionManager,
      items,
      top,
      left,
      onClose: (callback) => {
        globalThis.__menu.effect({ close: true });
        callback?.();
      },
    },
  });
  const defaults = up.getDefaultAppState();
  const patch = Object.fromEntries(
    Object.entries(appState).filter(([k, v]) => JSON.stringify(v) !== JSON.stringify(defaults[k])),
  );
  return {
    id,
    type,
    formFactor,
    elements: json(elements),
    appState: json(patch),
    top,
    left,
    viewport: { width: appState.width, height: appState.height },
    items: items.map((i) => (i === up.CONTEXT_MENU_SEPARATOR ? "|" : i.name)),
    tree,
  };
};

// Label functions that call a helper for the key: [helper, file].
const LABEL_HELPERS = [["getContextMenuLabel", "packages/excalidraw/components/hyperlink/Hyperlink.tsx"]];

/** The locale key literals of `name`'s source in `file`. */
const helperKeys = (upstream, [name, file]) => {
  const source = readFileSync(join(upstream.dir, file), "utf8");
  const start = source.indexOf(`export const ${name} = (`);
  if (start < 0) throw new Error(`${file}: ${name} not found`);
  const body = source.slice(start, source.indexOf("\n};\n", start));
  return [...body.matchAll(/"([A-Za-z]+(?:\.[A-Za-z_]+)+)"/g)].map(([, k]) => k);
};

/** Every label key the menus' actions can show. */
const labelKeys = (up, upstream, getItems) => {
  const keys = new Set();
  const actions = new Set();
  for (const type of ["canvas", "element"]) {
    for (const viewModeEnabled of [false, true]) {
      for (const item of getItems({ state: { viewModeEnabled }, editorInterface: { formFactor: "desktop" } }, type)) {
        if (item && item !== up.CONTEXT_MENU_SEPARATOR) actions.add(item);
      }
    }
  }
  for (const action of actions) {
    if (typeof action.label === "string") keys.add(action.label);
    else if (typeof action.label === "function") {
      for (const [, k] of action.label.toString().matchAll(/"([A-Za-z]+(?:\.[A-Za-z_]+)+)"/g)) {
        if (up.t(k) !== k) keys.add(k);
      }
      for (const helper of LABEL_HELPERS) {
        if (action.label.toString().includes(helper[0])) {
          for (const k of helperKeys(upstream, helper)) keys.add(k);
        }
      }
    }
  }
  return [...keys].sort();
};

// -- stylesheets --------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  // sass leads with @charset for the checkmark's non-ASCII content; the
  // rules go into a <style> element (and after the token rule), where it
  // has no effect
  const compiled = sass
    .compileString(entry, { loadPaths: [root], style: "expanded" })
    .css.replace(/^@charset "UTF-8";\n/, "");
  // the popover's layer, from the :root tokens of css/styles.scss, scoped
  // to the container as excali_ui::layers scopes the canvases' tokens
  const styles = readFileSync(join(root, "css", "styles.scss"), "utf8");
  const token = styles.match(/^\s*(--zIndex-ui-context-menu): (\d+);$/m);
  if (!token) throw new Error("css/styles.scss: --zIndex-ui-context-menu not found");
  return (
    "/* Generated by tools/goldens/context-menu.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded), after the\n" +
    " * context menu's z-index token of css/styles.scss's :root. */\n" +
    `.excalidraw {\n  ${token[1]}: ${token[2]};\n}\n\n` +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", { url: "http://localhost/" });
  const { window } = dom;
  // the async clipboard of a current browser (see the header)
  Object.defineProperty(window.navigator, "clipboard", {
    value: { writeText: () => Promise.resolve(), write: () => Promise.resolve(), readText: () => Promise.resolve("") },
    configurable: true,
  });
  window.ClipboardItem = function ClipboardItem() {};
  if (!("toBlob" in window.HTMLCanvasElement.prototype)) window.HTMLCanvasElement.prototype.toBlob = () => {};
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLCanvasElement: window.HTMLCanvasElement,
    SVGElement: window.SVGElement,
    devicePixelRatio: 1,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  return window;
};

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating context menu goldens");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  const imports = appActionImports(upstream);
  const up = await loadUpstream(upstream, {
    entry:
      ENTRY +
      imports.modules.map((m, i) => `export * as actions${i} from "./packages/excalidraw/${m}";\n`).join(""),
    stubs: STUBS,
    shims: SHIMS,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (s) => s.length * 10 });
  const getItems = itemsFunction(up, contextMenuItemsSource(upstream), imports.names);
  const cases = deterministic(() => CASES.map((c) => runCase(up, getItems, c)));
  const locale = Object.fromEntries(labelKeys(up, upstream, getItems).map((k) => [k, up.t(k)]));
  delete globalThis.__menu;
  window.close();
  const fixture = {
    description:
      "Upstream ContextMenu (components/ContextMenu.tsx) in its Popover over the items of App.getContextMenuItems (App.tsx:13835-13936) at the pinned commit (tools/goldens/context-menu.mjs): per case the scene, app state, form factor, the items, and the rendered tree ({tag, class, attrs, children, on}); `locale` holds the English label of every action the menus hold.",
    upstream: upstream.commit,
    locale,
    cases,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`context-menu: ${error.message}\n`);
    process.exit(1);
  }
  const files = await build(upstream);
  const dir = args.out ?? OUT_DIR;
  if (args.check) {
    const stale = Object.entries(files).filter(([f, t]) => {
      const path = join(dir, f);
      return !existsSync(path) || readFileSync(path, "utf8") !== t;
    });
    if (stale.length) {
      for (const [f] of stale) process.stderr.write(`stale: ${relative(process.cwd(), join(dir, f))}\n`);
      process.stderr.write("context-menu goldens are out of date: run node tools/goldens/context-menu.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`context-menu goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, t] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, t);
    process.stdout.write(`wrote ${relative(process.cwd(), path)} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
