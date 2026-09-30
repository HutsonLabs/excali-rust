#!/usr/bin/env node
// Main menu goldens for excali-ui (ex-520): upstream's own MainMenu
// (packages/excalidraw/components/main-menu/MainMenu.tsx), the default
// menu LayerUI renders (components/LayerUI.tsx:111-136), the items of
// main-menu/DefaultItems.tsx (the Light/Dark/System theme radio and the
// Preferences submenu among them) and the DropdownMenu components under
// them (components/dropdownMenu/*), run from the pinned checkout under
// Node, and their stylesheets compiled with sass 1.51.0.
//
//   node tools/goldens/main-menu.mjs            write the fixture and CSS
//   node tools/goldens/main-menu.mjs --check    exit 1 if either is stale
//   node tools/goldens/main-menu.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/main-menu.json:
//
// - `menus`: DefaultMainMenu for an app state, UIOptions.canvasActions,
//   a scene size and a form factor, with upstream's own actions (their
//   predicates decide what shows);
// - `items`: single DefaultItems (ToggleTheme with the system theme for
//   each theme, Preferences for several app states, CommandPalette,
//   LiveCollaborationTrigger) inside a DropdownMenu content;
// - `locale`: the English strings the menu shows (locales/en.json).
//
// A tree is `{ tag, class?, attrs?, style?, children?, on? }` for a DOM
// element, a string for text and `{ icon }` for an icons.tsx export. `on`
// maps an event to what its handler does, as a list of effects:
// `{ executeAction }`, `{ setAppState }`, `{ toggleLock }`,
// `{ confirmDialog }` (setActiveConfirmDialog), `{ openConfirmModal }`
// (its title; it resolves to confirmed), `{ onThemeChange }`,
// `{ trackEvent }`, `{ onSelect }` (a caller's handler) and `{ warn }`
// (console.warn). Radix's
// `onSelect` is recorded as `select`; the handler gets a fresh event and
// the effects are collected once its promises settle.
//
// JSX is evaluated by a stand-in runtime (react/jsx-runtime shimmed below)
// that calls function components, flattens fragments and applies context
// providers. radix-ui's DropdownMenu is shimmed to the DOM its parts leave
// for the markup upstream passes it: Trigger a <button aria-haspopup="menu"
// aria-expanded>, Content and SubContent a <div role="menu"> (with
// upstream's class and style; placement is Radix's), Item and SubTrigger
// the child (`asChild`, class names joined as Radix's Slot joins them) or a
// <div>, with role="menuitem"; Root and Sub render their children, the
// content only when open (a sub is rendered open). Radix's generated ids,
// data-state and roving focus are not recorded.
//
// Deterministic: no clocks or randomness are read; `isDarwin` is false
// (jsdom's navigator), so shortcuts read Ctrl.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "main-menu.json");
export const STYLESHEET = join("src", "main_menu.css");

// Upstream's SCSS for the menu, in this order in main_menu.css.
export const STYLESHEETS = ["components/dropdownMenu/DropdownMenu.scss", "components/main-menu/DefaultItems.scss"];

const ENTRY = `
export { default as MainMenu } from "./packages/excalidraw/components/main-menu/MainMenu";
export { default as DropdownMenu } from "./packages/excalidraw/components/dropdownMenu/DropdownMenu";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { t } from "./packages/excalidraw/i18n";
export * as icons from "./packages/excalidraw/components/icons";
`;

// Packages the module graph of DefaultItems (through actions/index)
// reaches but the menu never calls.
const STUBS = ["fuzzy", "pica", "react-dom", "browser-fs-access", "image-blob-reduce"];

const FRAGMENT = Symbol.for("excali-rust.fragment");
const PROVIDER = Symbol.for("excali-rust.provider");

const SHIMS = {
  "react/jsx-runtime": `const F = Symbol.for("excali-rust.fragment");
const jsx = (type, props, key) => ({ type, props });
module.exports = { jsx, jsxs: jsx, Fragment: F };`,
  react: `const F = Symbol.for("excali-rust.fragment");
const P = Symbol.for("excali-rust.provider");
const id = (f) => f;
const createElement = (type, props, ...children) => ({ type, props: { ...(props ?? {}), ...(children.length ? { children: children.length === 1 ? children[0] : children } : {}) } });
const Children = { toArray: (c) => (c === undefined || c === null ? [] : Array.isArray(c) ? c.flat(Infinity) : [c]).filter((x) => x !== null && x !== undefined && x !== false && x !== true) };
const isValidElement = (x) => !!x && typeof x === "object" && "type" in x && "props" in x;
const cloneElement = (el, props) => ({ type: el.type, props: { ...el.props, ...props } });
const createContext = (value) => { const ctx = { current: value }; ctx.Provider = { [P]: ctx }; return ctx; };
const R = {
  Fragment: F,
  Children,
  createElement,
  cloneElement,
  isValidElement,
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
  // radix-ui's DropdownMenu as the DOM it leaves (see the header).
  "radix-ui": `const F = Symbol.for("excali-rust.fragment");
let open = false;
const join = (...c) => c.filter(Boolean).join(" ");
const asChild = (props, extra) => {
  const { asChild, children, className, onSelect, ...rest } = props;
  const child = Array.isArray(children) ? children[0] : children;
  return { type: child.type, props: { ...extra, ...rest, ...child.props, className: join(className, child.props.className), onSelect } };
};
const DropdownMenu = {
  Root: ({ open: o, children }) => { open = o; return { type: F, props: { children } }; },
  Trigger: ({ children, ...rest }) => ({ type: "button", props: { "aria-haspopup": "menu", "aria-expanded": String(open), ...rest, children } }),
  Content: ({ className, style, children, "data-testid": testid }) => open ? ({ type: "div", props: { role: "menu", className, style, "data-testid": testid, children } }) : null,
  Item: (props) => props.asChild ? asChild(props, { role: "menuitem" }) : ({ type: "div", props: { role: "menuitem", ...props } }),
  Sub: ({ children }) => ({ type: F, props: { children } }),
  SubTrigger: ({ className, children }) => ({ type: "div", props: { role: "menuitem", "aria-haspopup": "menu", "aria-expanded": "true", className, children } }),
  SubContent: ({ className, children }) => ({ type: "div", props: { role: "menu", className, children } }),
};
module.exports = { DropdownMenu };`,
  // The editor's hooks, answered from globalThis.__menu.
  "packages/excalidraw/components/App": `module.exports = {
  useEditorInterface: () => globalThis.__menu.editorInterface,
  useExcalidrawSetAppState: () => (patch) => globalThis.__menu.effect({ setAppState: patch }),
  useExcalidrawActionManager: () => globalThis.__menu.actionManager,
  useExcalidrawElements: () => globalThis.__menu.elements,
  useExcalidrawAppState: () => globalThis.__menu.appState,
  useAppProps: () => globalThis.__menu.appProps,
  useApp: () => ({ toggleLock: () => globalThis.__menu.effect({ toggleLock: null }) }),
};`,
  "packages/excalidraw/context/ui-appState": `module.exports = { useUIAppState: () => globalThis.__menu.appState };`,
  "packages/excalidraw/context/tunnels": `module.exports = { useTunnels: () => ({
  MainMenuTunnel: { In: ({ children }) => children },
  tunnelsJotai: { useAtom: () => [0, () => {}] },
}) };`,
  "packages/excalidraw/editor-jotai": `const atom = (init) => ({ init });
module.exports = {
  atom,
  useSetAtom: () => (value) => globalThis.__menu.effect({ confirmDialog: value }),
  useAtomValue: (a) => a.init,
  useAtom: (a) => [a.init, () => {}],
  editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
};`,
  "packages/excalidraw/components/OverwriteConfirm/OverwriteConfirmState": `module.exports = {
  openConfirmModal: ({ title }) => { globalThis.__menu.effect({ openConfirmModal: title }); return Promise.resolve(true); },
};`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: (category, action, label) => globalThis.__menu.effect({ trackEvent: [category, action, label] }) };`,
  "packages/excalidraw/components/UserList": `module.exports = { UserList: () => { throw new Error("UserList: no collaborators in these cases"); } };`,
  "jotai-scope": `module.exports = { createIsolation: () => ({ Provider() {}, useAtom() { return []; }, useAtomValue() {}, useSetAtom() {}, useStore() {} }) };`,
  jotai: `module.exports = { atom: (init) => ({ init }), createStore: () => ({ get() {}, set() {}, sub() {} }) };`,
};

const usage = () => {
  process.stderr.write("usage: main-menu.mjs [--check] [--out DIR]\n");
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

// -- cases ----------------------------------------------------------------------------

// `UIOptions.canvasActions` as App normalizes them (App.tsx's
// DEFAULT_UI_OPTIONS, toggleTheme settled from `props.theme`).
const CANVAS_ACTIONS = {
  changeViewBackgroundColor: true,
  clearCanvas: true,
  export: { saveFileToDisk: true },
  loadScene: true,
  saveToActiveFile: true,
  toggleTheme: true,
  saveAsImage: true,
};

// [id, { state, canvasActions, elements (scene size), phone, viewModeProp }]
const MENU_CASES = [
  ["closed", { state: { openMenu: null } }],
  ["default", {}],
  ["dark", { state: { theme: "dark" }, elements: 2 }],
  ["no-theme-toggle", { canvasActions: { toggleTheme: false } }],
  ["file-handle", { state: { fileHandle: {} } }],
  ["view-mode", { state: { viewModeEnabled: true } }],
  [
    "trimmed",
    {
      canvasActions: {
        export: false,
        saveAsImage: false,
        clearCanvas: false,
        loadScene: false,
        changeViewBackgroundColor: false,
      },
    },
  ],
  ["link-selector", { state: { openDialog: { name: "elementLinkSelector" } } }],
  ["phone", { phone: true, elements: 1 }],
];

// [id, component, props, { state, phone, viewModeProp }]
const ITEM_CASES = [
  ["theme-system-light", "ToggleTheme", { allowSystemTheme: true, theme: "light" }, {}],
  ["theme-system-dark", "ToggleTheme", { allowSystemTheme: true, theme: "dark" }, { state: { theme: "dark" } }],
  ["theme-system-system", "ToggleTheme", { allowSystemTheme: true, theme: "system" }, {}],
  ["theme-system-phone", "ToggleTheme", { allowSystemTheme: true, theme: "light" }, { phone: true }],
  ["theme-system-no-handler", "ToggleTheme", { allowSystemTheme: true, theme: "dark" }, { noThemeHandler: true }],
  ["preferences-default", "Preferences", {}, {}],
  [
    "preferences-on",
    "Preferences",
    {},
    {
      state: {
        activeTool: { type: "selection", customType: null, locked: true, fromSelection: false, lastActiveTool: null },
        boxSelectionMode: "overlap",
        inputDevice: "mouse",
        objectsSnapModeEnabled: true,
        gridModeEnabled: true,
        zenModeEnabled: true,
        viewModeEnabled: true,
        stats: { open: true, panels: 3 },
        bindingPreference: "disabled",
        isMidpointSnappingEnabled: false,
        showHints: false,
      },
    },
  ],
  ["preferences-trackpad", "Preferences", {}, { state: { inputDevice: "trackpad" } }],
  ["preferences-view-mode-prop", "Preferences", {}, { viewModeProp: true }],
  ["preferences-phone", "Preferences", {}, { phone: true }],
  ["command-palette", "CommandPalette", {}, {}],
  ["live-collaboration", "LiveCollaborationTrigger", { isCollaborating: false }, {}],
  ["live-collaboration-active", "LiveCollaborationTrigger", { isCollaborating: true }, {}],
];

// Keys whose English text the menu shows.
const LOCALE_KEYS = [
  "buttons.load",
  "buttons.save",
  "buttons.exportImage",
  "buttons.export",
  "buttons.clearReset",
  "buttons.lightMode",
  "buttons.darkMode",
  "buttons.systemMode",
  "buttons.objectsSnapMode",
  "buttons.zenMode",
  "buttons.menu",
  "commandPalette.title",
  "search.title",
  "helpDialog.title",
  "labels.theme",
  "labels.canvasBackground",
  "labels.followUs",
  "labels.discordChat",
  "labels.liveCollaboration",
  "labels.preferences",
  "labels.preferences_toolLock",
  "labels.boxSelectionMode",
  "labels.boxSelectionContain",
  "labels.boxSelectionOverlap",
  "labels.inputDevice",
  "labels.inputDeviceTrackpad",
  "labels.inputDeviceMouse",
  "labels.arrowBinding",
  "labels.midpointSnapping",
  "labels.showHints",
  "labels.toggleGrid",
  "labels.viewMode",
  "labels.collaborators",
  "stats.fullTitle",
  "overwriteConfirm.modal.loadFromFile.title",
];

// -- running upstream -----------------------------------------------------------------

const kebab = (name) => (name.startsWith("--") ? name : name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`));

/** The rendered JSX as a plain tree (see the header); handlers queued. */
const flatten = (node, ctx) => {
  if (node === null || node === undefined || node === false || node === true) return [];
  if (Array.isArray(node)) return node.flatMap((n) => flatten(n, ctx));
  if (typeof node === "string" || typeof node === "number") return [String(node)];
  const icon = ctx.iconNames.get(node);
  if (icon) return [{ icon }];
  const { type, props } = node;
  if (type === FRAGMENT) return flatten(props.children, ctx);
  if (type && type[PROVIDER]) {
    const c = type[PROVIDER];
    const saved = c.current;
    c.current = props.value;
    try {
      return flatten(props.children, ctx);
    } finally {
      c.current = saved;
    }
  }
  if (typeof type === "function") return flatten(type(props), ctx);
  if (typeof type !== "string") throw new Error(`unexpected element type ${String(type)}`);
  const out = { tag: type };
  if (props.className) out.class = props.className;
  const attrs = {};
  const on = {};
  for (const [k, v] of Object.entries(props)) {
    if (["children", "className", "style", "ref"].includes(k) || v === undefined || v === null) continue;
    if (typeof v === "function" && /^on[A-Z]/.test(k)) {
      const event = k.slice(2).toLowerCase();
      on[event] = null;
      ctx.handlers.push({ on, event, handler: v });
      continue;
    }
    if (typeof v === "boolean") {
      // as React 19 writes a boolean (react-dom setProp and
      // setValueForAttribute): an input's `checked` and the boolean
      // attributes present and empty when true, `data-`/`aria-` as text,
      // any other (DropdownMenuItemCheckbox's `checked` on its button) left
      // out
      if (/^(data|aria)-/.test(k)) attrs[k] = String(v);
      else if ((type === "input" && k === "checked") || ["disabled", "hidden", "required"].includes(k)) {
        if (v) attrs[k] = "";
      }
      continue;
    }
    if (typeof v !== "string") throw new Error(`<${type}> ${k} is not a string`);
    attrs[k] = v;
  }
  if (Object.keys(attrs).length) out.attrs = attrs;
  if (props.style) {
    // React leaves out undefined and null values
    const style = Object.entries(props.style).filter(([, v]) => v !== undefined && v !== null);
    if (style.length) out.style = Object.fromEntries(style.map(([k, v]) => [kebab(k), String(v)]));
  }
  const children = flatten(props.children, ctx);
  if (children.length) out.children = children;
  if (Object.keys(on).length) out.on = on;
  return [out];
};

const settle = () => new Promise((r) => setImmediate(r));

/** Renders `element` and runs each handler it binds on a fresh event. */
const run = async (up, element, env) => {
  const ctx = { iconNames: env.iconNames, handlers: [] };
  let effects = [];
  globalThis.__menu = { ...env.hooks, effect: (e) => effects.push(json(e)) };
  const tree = flatten(element, ctx);
  const warn = console.warn;
  console.warn = (message) => globalThis.__menu.effect({ warn: message });
  try {
    for (const h of ctx.handlers) {
      effects = [];
      const event = {
        defaultPrevented: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
      };
      await h.handler(event);
      await settle();
      h.on[h.event] = effects;
    }
  } finally {
    console.warn = warn;
  }
  return tree;
};

const environment = (up, iconNames, c) => {
  const appState = {
    ...up.getDefaultAppState(),
    openMenu: "canvas",
    ...(c.state ?? {}),
  };
  const canvasActions = { ...CANVAS_ACTIONS, ...(c.canvasActions ?? {}) };
  const props = {
    UIOptions: { canvasActions },
    ...(c.viewModeProp ? { viewModeEnabled: true } : {}),
    ...(c.noThemeHandler ? {} : { onThemeChange: (value) => globalThis.__menu.effect({ onThemeChange: value }) }),
  };
  const app = { props, isInteractionEnabled: () => true, isNavigationEnabled: () => true };
  const elements = Array.from({ length: c.elements ?? 0 }, (_, i) => ({ id: `e${i}`, isDeleted: false }));
  const actionManager = {
    isActionEnabled: (action) => !action.predicate || action.predicate(elements, appState, props, app),
    executeAction: (action) => globalThis.__menu.effect({ executeAction: action.name }),
    renderAction: (name) => ({ type: "action-slot", props: { name } }),
  };
  const hooks = {
    appState,
    appProps: props,
    elements,
    actionManager,
    editorInterface: { formFactor: c.phone ? "phone" : "desktop" },
  };
  return { iconNames, hooks };
};

/** renderAction's slot as `{ action }`. */
const slots = (tree) =>
  tree.map((n) => {
    if (typeof n !== "object" || !n.tag) return n;
    if (n.tag === "action-slot") return { action: n.attrs.name };
    return n.children ? { ...n, children: slots(n.children) } : n;
  });

const defaultMainMenu = (up, UIOptions) => {
  const { MainMenu } = up;
  const D = MainMenu.DefaultItems;
  // LayerUI.tsx:111-136, as JSX compiles it.
  const el = (type, props = {}, ...children) => ({
    type,
    props: { ...props, ...(children.length ? { children: children.length === 1 ? children[0] : children } : {}) },
  });
  return el(
    MainMenu,
    { __fallback: true },
    el(D.LoadScene),
    el(D.SaveToActiveFile),
    UIOptions.canvasActions.export && el(D.Export),
    UIOptions.canvasActions.saveAsImage && el(D.SaveAsImage),
    el(D.SearchMenu),
    el(D.Help),
    el(D.ClearCanvas),
    el(MainMenu.Separator),
    el(MainMenu.Group, { title: "Excalidraw links" }, el(D.Socials)),
    el(MainMenu.Separator),
    el(D.ToggleTheme, { allowSystemTheme: false }),
    el(D.ChangeCanvasBackground),
  );
};

/** A DefaultItems component inside an open DropdownMenu content, as the
 * MainMenu content holds it (its onSelect closes the menu). */
const itemInMenu = (up, component, props) => {
  const { MainMenu, DropdownMenu } = up;
  const item = { type: MainMenu.DefaultItems[component], props };
  if (component === "LiveCollaborationTrigger") item.props = { ...props, onSelect: () => globalThis.__menu.effect({ onSelect: null }) };
  return {
    type: DropdownMenu,
    props: {
      open: true,
      children: {
        type: DropdownMenu.Content,
        props: {
          className: "main-menu",
          align: "start",
          onSelect: () => globalThis.__menu.effect({ setAppState: { openMenu: null } }),
          children: item,
        },
      },
    },
  };
};

// -- stylesheets --------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/main-menu.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom(ORIGIN);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const iconNames = new Map();
  for (const [name, value] of Object.entries(up.icons)) {
    if (value && typeof value === "object" && "type" in value && !iconNames.has(value)) iconNames.set(value, name);
  }
  const menus = [];
  for (const [id, c] of MENU_CASES) {
    const env = environment(up, iconNames, c);
    const tree = slots(await run(up, defaultMainMenu(up, env.hooks.appProps.UIOptions), env));
    menus.push({
      id,
      appState: json(c.state ?? {}),
      canvasActions: json({ ...CANVAS_ACTIONS, ...(c.canvasActions ?? {}) }),
      elements: c.elements ?? 0,
      phone: Boolean(c.phone),
      tree,
    });
  }
  const items = [];
  for (const [id, component, props, c] of ITEM_CASES) {
    const env = environment(up, iconNames, c);
    const tree = slots(await run(up, itemInMenu(up, component, props), env));
    items.push({
      id,
      component,
      props: json(props),
      appState: json(c.state ?? {}),
      phone: Boolean(c.phone),
      viewModeProp: Boolean(c.viewModeProp),
      themeHandler: !c.noThemeHandler,
      tree,
    });
  }
  delete globalThis.__menu;
  window.close();
  const fixture = {
    description:
      "Upstream MainMenu, DefaultMainMenu (LayerUI.tsx:111-136) and main-menu/DefaultItems.tsx over the DropdownMenu components at the pinned commit (tools/goldens/main-menu.mjs): per case the rendered tree ({tag, class, attrs, style, children, on}, text, {icon} and {action} where renderAction is called) and, under `on`, the effects each handler has.",
    upstream: upstream.commit,
    locale: Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)])),
    menus,
    items,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`main-menu: ${error.message}\n`);
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
      process.stderr.write("main-menu goldens are out of date: run node tools/goldens/main-menu.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`main-menu goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, text] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
    process.stdout.write(`wrote ${relative(process.cwd(), path)} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
