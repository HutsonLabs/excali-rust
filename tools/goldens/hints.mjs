#!/usr/bin/env node
// Hints and welcome screen goldens for excali-ui (ex-528): upstream's own
// HintViewer (packages/excalidraw/components/HintViewer.tsx), CursorHint and
// CursorHints (components/CursorHint.tsx, positionElementBesideCursor.ts)
// and WelcomeScreen (components/welcome-screen/*, with ExcalidrawLogo.tsx),
// rendered by React 19.0.0 into jsdom 22.1.0 once per platform (isDarwin is
// read from the navigator when the modules load, so each platform is a
// fresh load of the bundle), and their stylesheets compiled with sass
// 1.51.0.
//
//   node tools/goldens/hints.mjs            write the fixture and CSS
//   node tools/goldens/hints.mjs --check    exit 1 if any is stale
//   node tools/goldens/hints.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/hints.json:
//
// - `locale`: the English strings the components read (every `hints.*` and
//   `welcomeScreen.defaults.*` key of locales/en.json, `keys.mmb`,
//   `buttons.load`, `helpDialog.title`);
// - `hintViewer`: per platform and case the input HintViewer reads (the
//   app state keys set over getDefaultAppState(), the selected elements as
//   upstream's newElement builds them, isMobile, editorInterface's
//   canFitSidebar, the host's gridModeEnabled prop and the transform
//   handle being dragged), the hint keys the case exercises (`hints`, the
//   case's label) and the DOM HintViewer renders (`null` for none):
//   `{tag, attrs, style, children}` with attributes and inline style sorted
//   by name and text as a string;
// - `cursorHints`: CursorHints' policy (cooldown and bypasses) on event
//   sequences, per event the icons.tsx export it shows or null; `position`:
//   positionElementBesideCursor over a grid of cursors; `cursorHint`: the
//   DOM CursorHint renders for an arrow icon, before and after its fade-out
//   delay, and its timing constants;
// - `welcome`: per platform and case (form factor, view mode) the DOM of
//   WelcomeScreen's four parts (Center, MenuHint, ToolbarHint, HelpHint;
//   the tunnels pass their children through) with icons as `{icon: name}`
//   and the logo as `{logo: true}`, and the actions its menu items execute.
//
// and crates/excali-ui/src/hints/hints.css (HintViewer.scss and
// CursorHint.scss), crates/excali-ui/src/welcome_screen/welcome_screen.css
// (WelcomeScreen.scss and ExcalidrawLogo.scss), and
// crates/excali-ui/src/welcome_screen/excalidraw_logo.html (the markup of
// `<ExcalidrawLogo withText />`).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "hints.json");
export const HINTS_CSS = join("src", "hints", "hints.css");
export const WELCOME_CSS = join("src", "welcome_screen", "welcome_screen.css");
export const LOGO = join("src", "welcome_screen", "excalidraw_logo.html");

const ENTRY = `
export { HintViewer } from "./packages/excalidraw/components/HintViewer";
export { CursorHint, CursorHints, cursorHintAtom, CURSOR_HINT_COOLDOWN } from "./packages/excalidraw/components/CursorHint";
export { positionElementBesideCursor } from "./packages/excalidraw/components/positionElementBesideCursor";
export { default as WelcomeScreen } from "./packages/excalidraw/components/welcome-screen/WelcomeScreen";
export { ExcalidrawLogo } from "./packages/excalidraw/components/ExcalidrawLogo";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export {
  newElement,
  newArrowElement,
  newImageElement,
  newLinearElement,
  newStickyNoteElement,
  newFreeDrawElement,
} from "./packages/element/src/newElement";
export { reseed } from "./packages/common/src/random";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks; the shim answers them from
// globalThis.__ui. The tunnels render their children in place (In) so each
// welcome screen part lands where it is rendered; the editor's jotai atoms
// read from globalThis.__ui.atoms.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useExcalidrawSetAppState: () => () => {},
      useExcalidrawActionManager: () => ({
        executeAction: (action) => globalThis.__ui.executed.push(action.name),
      }),
    };`,
  "packages/excalidraw/context/tunnels": `
    const tunnel = () => ({ In: ({ children }) => children, Out: () => null });
    module.exports = {
      useTunnels: () => ({
        WelcomeScreenMenuHintTunnel: tunnel(),
        WelcomeScreenToolbarHintTunnel: tunnel(),
        WelcomeScreenHelpHintTunnel: tunnel(),
        WelcomeScreenCenterTunnel: tunnel(),
      }),
    };`,
  "packages/excalidraw/editor-jotai": `
    const atom = (init) => ({ init });
    // one setter for every render, as jotai's (CursorHint's effects depend
    // on it)
    const noop = () => {};
    const read = (a) => (globalThis.__ui && globalThis.__ui.atoms.has(a) ? globalThis.__ui.atoms.get(a) : a.init);
    module.exports = {
      atom,
      useSetAtom: () => noop,
      useAtomValue: read,
      useAtom: (a) => [read(a), noop],
      editorJotaiStore: { get: read, set: () => {}, sub: () => () => {} },
    };`,
  "packages/excalidraw/components/LibraryMenu": `module.exports = { isLibraryMenuOpenAtom: { init: false } };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
};

// Packages the actions index reaches that these components never run.
const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: hints.mjs [--check] [--out DIR]\n");
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

// -- platforms ------------------------------------------------------------------

const CHROME = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
const PLATFORMS = [
  { name: "linux", platform: "Linux x86_64", userAgent: CHROME },
  { name: "darwin", platform: "MacIntel", userAgent: CHROME },
];

// -- hint viewer cases ------------------------------------------------------------

/** The elements a case selects, built by upstream's newElement functions
 * with fixed ids and seeds (reseed() first; `created` and `updated` fixed). */
const ELEMENTS = {
  rectangle: (up) => up.newElement({ type: "rectangle", id: "rect", x: 0, y: 0, width: 100, height: 50, seed: 1 }),
  lockedRectangle: (up) =>
    up.newElement({ type: "rectangle", id: "locked", x: 0, y: 0, width: 100, height: 50, seed: 2, locked: true }),
  diamond: (up) => up.newElement({ type: "diamond", id: "diamond", x: 0, y: 0, width: 60, height: 60, seed: 3 }),
  ellipse: (up) => up.newElement({ type: "ellipse", id: "ellipse", x: 0, y: 0, width: 80, height: 40, seed: 4 }),
  stickynote: (up) => up.newStickyNoteElement({ id: "note", x: 0, y: 0, width: 200, height: 200, seed: 5 }),
  embeddable: (up) => up.newElement({ type: "embeddable", id: "embed", x: 0, y: 0, width: 300, height: 200, seed: 6 }),
  frame: (up) => ({ ...up.newElement({ type: "frame", id: "frame", x: 0, y: 0, width: 300, height: 200, seed: 7 }), name: null }),
  image: (up) => up.newImageElement({ type: "image", id: "image", x: 0, y: 0, width: 100, height: 100, seed: 8 }),
  line: (up) => up.newLinearElement({ type: "line", id: "line", x: 0, y: 0, seed: 9, points: [[0, 0], [100, 40]] }),
  line3: (up) =>
    up.newLinearElement({ type: "line", id: "line3", x: 0, y: 0, seed: 10, points: [[0, 0], [50, 40], [100, 0]] }),
  arrow: (up) =>
    up.newArrowElement({ type: "arrow", id: "arrow", x: 0, y: 0, seed: 11, points: [[0, 0], [100, 40]], elbowed: false }),
  arrow3: (up) =>
    up.newArrowElement({
      type: "arrow",
      id: "arrow3",
      x: 0,
      y: 0,
      seed: 12,
      points: [[0, 0], [50, 40], [100, 0]],
      elbowed: false,
    }),
  freedraw: (up) => up.newFreeDrawElement({ type: "freedraw", id: "draw", x: 0, y: 0, seed: 13, points: [[0, 0], [5, 5]], simulatePressure: true }),
  // newTextElement measures its text on a canvas; the hint reads only the
  // type, so a rectangle's props with the text fields restore writes
  text: (up) => ({
    ...up.newElement({ type: "rectangle", id: "text", x: 0, y: 0, width: 40, height: 25, seed: 14 }),
    type: "text",
    fontSize: 20,
    fontFamily: 5,
    text: "hi",
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    originalText: "hi",
    autoResize: true,
    lineHeight: 1.25,
  }),
};

const tool = (type) => ({ activeTool: { type, customType: null, locked: false, fromSelection: false, lastActiveTool: null } });
const linear = (elementId, extra) => ({
  selectedLinearElement: {
    elementId,
    isDragging: false,
    isEditing: false,
    selectedPointsIndices: null,
    hoverPointIndex: -1,
    ...extra,
  },
});

/** `{name, hints, appState, selected, isMobile, canFitSidebar,
 * gridModeEnabled, activeResizeHandle}`; `hints` is the label: the keys
 * upstream's getHints returns, in order ([] for none). */
const HINT_CASES = [
  { name: "selection-empty", hints: ["canvasPanning"] },
  { name: "selection-empty-mobile", hints: [], isMobile: true },
  { name: "show-hints-off", hints: [], appState: { showHints: false } },
  {
    name: "search-matches",
    hints: ["dismissSearch"],
    appState: { openSidebar: { name: "default", tab: "search" }, searchMatches: { focusedId: null, matches: [{ id: "x" }] } },
  },
  {
    name: "search-no-matches",
    hints: ["canvasPanning"],
    appState: { openSidebar: { name: "default", tab: "search" }, searchMatches: { focusedId: null, matches: [] } },
  },
  { name: "sidebar-cannot-fit", hints: [], appState: { openSidebar: { name: "default", tab: "library" } }, canFitSidebar: false },
  { name: "sidebar-fits", hints: ["canvasPanning"], appState: { openSidebar: { name: "default", tab: "library" } } },
  { name: "eraser", hints: ["eraserRevert"], appState: tool("eraser") },
  { name: "arrow-point-drag", hints: ["arrowBindModifiers"], selected: ["arrow"], appState: linear("arrow", { isDragging: true }) },
  { name: "line-point-drag", hints: ["lineEditor_line_info"], selected: ["line"], appState: linear("line", { isDragging: true }) },
  { name: "arrow-tool", hints: ["arrowTool"], appState: tool("arrow") },
  { name: "arrow-tool-multi", hints: ["linearElementMulti"], appState: { ...tool("arrow"), multiElement: { id: "arrow" } } },
  { name: "line-tool", hints: ["linearElement"], appState: tool("line") },
  { name: "line-tool-multi", hints: ["linearElementMulti"], appState: { ...tool("line"), multiElement: { id: "line" } } },
  { name: "freedraw-tool", hints: ["freeDraw"], appState: tool("freedraw") },
  { name: "text-tool", hints: ["text"], appState: tool("text") },
  { name: "embeddable-tool", hints: ["embeddable"], appState: tool("embeddable") },
  { name: "stickynote-tool", hints: ["stickynote"], appState: tool("stickynote") },
  { name: "autoshape-tool", hints: ["autoshape"], appState: tool("autoshape") },
  { name: "rectangle-tool", hints: [], appState: tool("rectangle") },
  { name: "hand-tool", hints: [], appState: tool("hand") },
  { name: "laser-tool", hints: [], appState: tool("laser") },
  { name: "lasso-tool", hints: [], appState: tool("lasso") },
  { name: "resize-line", hints: ["lockAngle"], selected: ["line"], appState: { isResizing: true } },
  { name: "resize-line3", hints: ["resize"], selected: ["line3"], appState: { isResizing: true } },
  { name: "resize-rectangle", hints: ["resize"], selected: ["rectangle"], appState: { isResizing: true } },
  { name: "resize-rectangle-touch", hints: ["bindTextToElement", "createFlowchart"], selected: ["rectangle"], appState: { isResizing: true, lastPointerDownWith: "touch" } },
  { name: "resize-image", hints: ["resizeImage"], selected: ["image"], appState: { isResizing: true } },
  { name: "resize-note-corner", hints: ["resizeStickyNote"], selected: ["stickynote"], appState: { isResizing: true }, activeResizeHandle: "se" },
  { name: "resize-note-edge", hints: ["resize"], selected: ["stickynote"], appState: { isResizing: true }, activeResizeHandle: "e" },
  { name: "resize-two", hints: [], selected: ["rectangle", "ellipse"], appState: { isResizing: true } },
  { name: "rotate", hints: ["rotate"], selected: ["rectangle"], appState: { isRotating: true } },
  { name: "rotate-pen", hints: ["bindTextToElement", "createFlowchart"], selected: ["rectangle"], appState: { isRotating: true, lastPointerDownWith: "pen" } },
  { name: "text-selected", hints: ["text_selected"], selected: ["text"] },
  { name: "text-editing", hints: ["text_editing"], appState: { editingTextElement: { id: "text" } } },
  { name: "cropping", hints: ["leaveCropEditor"], selected: ["image"], appState: { croppingElementId: "image" } },
  { name: "image-selected", hints: ["enterCropEditor"], selected: ["image"] },
  { name: "box-select", hints: ["deepBoxSelect"], appState: { selectionElement: { id: "selection" } } },
  { name: "box-select-lasso", hints: [], appState: { ...tool("lasso"), selectionElement: { id: "selection" } } },
  { name: "grid-drag", hints: ["disableSnapping"], selected: ["rectangle"], appState: { gridModeEnabled: true, selectedElementsAreBeingDragged: true } },
  { name: "grid-prop-drag", hints: ["disableSnapping"], selected: ["rectangle"], appState: { selectedElementsAreBeingDragged: true }, gridModeEnabled: true },
  { name: "grid-prop-off-drag", hints: [], selected: ["rectangle"], appState: { gridModeEnabled: true, selectedElementsAreBeingDragged: true }, gridModeEnabled: false },
  { name: "drag-no-grid", hints: [], selected: ["rectangle"], appState: { selectedElementsAreBeingDragged: true } },
  { name: "arrow-hover-start", hints: ["toggleArrowhead"], selected: ["arrow"], appState: linear("arrow", { hoverPointIndex: 0 }) },
  { name: "arrow-hover-end", hints: ["toggleArrowhead"], selected: ["arrow3"], appState: linear("arrow3", { hoverPointIndex: 2 }) },
  { name: "arrow-hover-middle", hints: ["lineEditor_info"], selected: ["arrow3"], appState: linear("arrow3", { hoverPointIndex: 1 }) },
  { name: "line-hover-start", hints: ["lineEditor_line_info"], selected: ["line"], appState: linear("line", { hoverPointIndex: 0 }) },
  { name: "line-editing-point", hints: ["lineEditor_pointSelected"], selected: ["line"], appState: linear("line", { isEditing: true, selectedPointsIndices: [1] }) },
  { name: "line-editing-none", hints: ["lineEditor_nothingSelected"], selected: ["line"], appState: linear("line", { isEditing: true }) },
  { name: "arrow-editing-point", hints: ["lineEditor_pointSelected"], selected: ["arrow"], appState: linear("arrow", { isEditing: true, selectedPointsIndices: [0] }) },
  { name: "line-selected", hints: ["lineEditor_line_info"], selected: ["line"] },
  { name: "arrow-selected", hints: ["lineEditor_info"], selected: ["arrow"] },
  { name: "rectangle-selected", hints: ["bindTextToElement", "createFlowchart"], selected: ["rectangle"] },
  { name: "locked-selected", hints: ["bindTextToElement", "createFlowchart"], selected: ["lockedRectangle"] },
  { name: "diamond-selected", hints: ["bindTextToElement", "createFlowchart"], selected: ["diamond"] },
  { name: "ellipse-selected", hints: ["bindTextToElement", "createFlowchart"], selected: ["ellipse"] },
  { name: "note-selected", hints: ["bindTextToElement", "createFlowchart"], selected: ["stickynote"] },
  { name: "rectangle-new-element", hints: [], selected: ["rectangle"], appState: { newElement: { id: "rect" } } },
  { name: "embeddable-selected", hints: [], selected: ["embeddable"] },
  { name: "frame-selected", hints: [], selected: ["frame"] },
  { name: "freedraw-selected", hints: [], selected: ["freedraw"] },
  { name: "two-selected", hints: [], selected: ["rectangle", "ellipse"] },
  { name: "two-selected-mobile", hints: [], selected: ["rectangle", "ellipse"], isMobile: true },
  { name: "rectangle-selected-mobile", hints: ["bindTextToElement", "createFlowchart"], selected: ["rectangle"], isMobile: true },
];

// -- DOM ----------------------------------------------------------------------

const installDom = (p) => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  Object.defineProperty(window.navigator, "platform", { value: p.platform, configurable: true });
  Object.defineProperty(window.navigator, "userAgent", { value: p.userAgent, configurable: true });
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
    if (node.localName === "div" && node.classList.contains("ExcalidrawLogo")) return { logo: true };
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

const newHost = (window) => {
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  return container;
};

// -- hint viewer ----------------------------------------------------------------

const renderHint = async (up, window, c) => {
  up.reseed(7);
  // JSON keeps what a scene file holds (no undefined customData)
  const selected = (c.selected ?? []).map((k) => JSON.parse(JSON.stringify({ ...ELEMENTS[k](up), updated: 1, created: 1 })));
  const appState = { ...up.getDefaultAppState(), ...(c.appState ?? {}) };
  const app = {
    scene: {
      getSelectedElements: () => selected,
      getNonDeletedElementsMap: () => new Map(selected.map((e) => [e.id, e])),
    },
    props: c.gridModeEnabled === undefined ? {} : { gridModeEnabled: c.gridModeEnabled },
    state: appState,
    activeResizeHandle: c.activeResizeHandle ?? null,
  };
  const container = newHost(window);
  globalThis.__ui = { atoms: new Map(), executed: [] };
  const root = up.createRoot(container);
  await up.act(async () =>
    root.render(
      up.React.createElement(up.HintViewer, {
        appState,
        isMobile: !!c.isMobile,
        editorInterface: { canFitSidebar: c.canFitSidebar ?? true },
        app,
      }),
    ),
  );
  const dom = container.firstElementChild ? makeTree(new Map())(container.firstElementChild) : null;
  await up.act(async () => root.unmount());
  return {
    name: c.name,
    hints: c.hints,
    input: {
      appState: c.appState ?? {},
      selected,
      isMobile: !!c.isMobile,
      canFitSidebar: c.canFitSidebar ?? true,
      gridModeEnabled: c.gridModeEnabled ?? null,
      activeResizeHandle: c.activeResizeHandle ?? null,
    },
    dom,
  };
};

// -- cursor hint ------------------------------------------------------------------

const iconOf = (up, content) => {
  for (const [name, value] of Object.entries(up.icons)) if (value === content) return name;
  throw new Error("a cursor hint that is no icons.tsx export");
};

/** Event sequences: `{at, x, y, arrowType, event}`, `event` being
 * `["cycled", arrowType]` or `["tool", tool, source]`. */
const T = 1_700_000_000_000;

const COOLDOWN_SEQUENCES = (cooldown) => [
  {
    // lastShownAt starts at 0, so a clock this close to the epoch is still
    // in the first cooldown
    name: "near-epoch",
    events: [
      { at: 1000, x: 10, y: 10, arrowType: "round", event: ["tool", "arrow", "letter"] },
      { at: cooldown, x: 10, y: 10, arrowType: "round", event: ["tool", "arrow", "letter"] },
    ],
  },
  {
    name: "letter-then-cooldown",
    events: [
      { at: T, x: 10, y: 10, arrowType: "round", event: ["tool", "arrow", "letter"] },
      { at: T + 1000, x: 10, y: 10, arrowType: "round", event: ["tool", "arrow", "letter"] },
      { at: T + 1000, x: 10, y: 10, arrowType: "round", event: ["tool", "line", "letter"] },
      { at: T + cooldown - 1, x: 10, y: 10, arrowType: "round", event: ["tool", "line", "letter"] },
      { at: T + cooldown, x: 10, y: 10, arrowType: "elbow", event: ["tool", "arrow", "letter"] },
      { at: T + cooldown + 1, x: 10, y: 10, arrowType: "elbow", event: ["cycled", "sharp"] },
      { at: T + 2 * cooldown + 1, x: 10, y: 10, arrowType: "sharp", event: ["tool", "line", "letter"] },
    ],
  },
  {
    name: "digits-always",
    events: [
      { at: T + 5000, x: 3, y: 4, arrowType: "sharp", event: ["tool", "arrow", "digit"] },
      { at: T + 5001, x: 3, y: 4, arrowType: "elbow", event: ["tool", "arrow", "digit"] },
      { at: T + 5002, x: 3, y: 4, arrowType: "elbow", event: ["tool", "line", "digit"] },
      { at: T + 5003, x: 3, y: 4, arrowType: "elbow", event: ["tool", "arrow", "letter"] },
    ],
  },
  {
    name: "cycled-always",
    events: [
      { at: 100000, x: 50, y: 60, arrowType: "sharp", event: ["cycled", "round"] },
      { at: 100001, x: 50, y: 60, arrowType: "round", event: ["cycled", "elbow"] },
      { at: 100002, x: 50, y: 60, arrowType: "elbow", event: ["cycled", "sharp"] },
      { at: 100003, x: 50, y: 60, arrowType: "sharp", event: ["tool", "line", "letter"] },
    ],
  },
  {
    name: "no-pointer-yet",
    events: [
      { at: 1000, x: 0, y: 0, arrowType: "sharp", event: ["cycled", "round"] },
      { at: 1001, x: 0, y: 0, arrowType: "sharp", event: ["tool", "line", "digit"] },
      { at: 1002, x: 0, y: 5, arrowType: "sharp", event: ["tool", "line", "letter"] },
      { at: 1003, x: 5, y: 0, arrowType: "sharp", event: ["tool", "arrow", "letter"] },
      { at: 1004, x: 5, y: 0, arrowType: "sharp", event: ["tool", "arrow", "digit"] },
    ],
  },
];

const cursorHintPolicy = (up) => {
  const out = [];
  const realNow = Date.now;
  try {
    for (const seq of COOLDOWN_SEQUENCES(up.CURSOR_HINT_COOLDOWN)) {
      const shown = [];
      const app = {
        viewport: { lastPosition: { x: 0, y: 0 } },
        state: { currentItemArrowType: "sharp" },
        updateEditorAtom: (_atom, value) => shown.push(value),
      };
      const hints = new up.CursorHints(app);
      const results = [];
      for (const e of seq.events) {
        Date.now = () => e.at;
        app.viewport.lastPosition = { x: e.x, y: e.y };
        app.state.currentItemArrowType = e.arrowType;
        const before = shown.length;
        if (e.event[0] === "cycled") hints.onArrowTypeCycled(e.event[1]);
        else hints.onToolShortcut(e.event[1], e.event[2]);
        results.push(shown.length > before ? iconOf(up, shown.at(-1).content) : null);
      }
      out.push({ name: seq.name, events: seq.events, shown: results });
    }
  } finally {
    Date.now = realNow;
  }
  return out;
};

const positions = (up) => {
  const out = [];
  const containers = [
    { left: 0, top: 0, width: 800, height: 600 },
    { left: 100, top: 50, width: 300, height: 200 },
    { left: 0, top: 0, width: 20, height: 20 },
  ];
  const elements = [
    { width: 28, height: 28 },
    { width: 200, height: 150 },
  ];
  const cursors = [
    { x: 0, y: 0 },
    { x: 10, y: 20 },
    { x: 150, y: 120 },
    { x: 380, y: 240 },
    { x: 700, y: 590 },
    { x: 790.5, y: 12.25 },
    { x: -30, y: 900 },
  ];
  for (const container of containers)
    for (const element of elements)
      for (const cursor of cursors) {
        const gap = 16;
        out.push({ cursor, element, container, gap, ...up.positionElementBesideCursor({ cursor, element, container, gap }) });
      }
  return out;
};

const cursorHintDom = async (up, window, iconNames) => {
  const container = newHost(window);
  const hint = { content: up.icons.roundArrowIcon, nonce: 0.5 };
  globalThis.__ui = {
    atoms: new Map([[up.cursorHintAtom, hint]]),
    executed: [],
    app: { viewport: { lastPosition: { x: 40, y: 30 } } },
    container: { container, id: "excalidraw-id" },
  };
  const root = up.createRoot(container);
  await up.act(async () => root.render(up.React.createElement(up.CursorHint)));
  const shown = makeTree(iconNames)(container.firstElementChild);
  // CURSOR_HINT_DURATION (700 ms) later the fade-out class is on
  await up.act(async () => new Promise((r) => window.setTimeout(r, 750)));
  const fading = makeTree(iconNames)(container.firstElementChild);
  await up.act(async () => root.unmount());
  return { shown, fading };
};

// -- welcome screen -------------------------------------------------------------

const WELCOME_CASES = [
  { name: "desktop" },
  { name: "phone", formFactor: "phone" },
  { name: "tablet", formFactor: "tablet" },
  { name: "view-mode", viewModeEnabled: true },
];

const renderWelcome = async (up, window, iconNames, c) => {
  const container = newHost(window);
  globalThis.__ui = {
    atoms: new Map(),
    executed: [],
    editorInterface: { formFactor: c.formFactor ?? "desktop" },
  };
  const root = up.createRoot(container);
  const appState = { ...up.getDefaultAppState(), viewModeEnabled: !!c.viewModeEnabled };
  await up.act(async () =>
    root.render(
      up.React.createElement(up.UIAppStateContext.Provider, { value: appState }, up.React.createElement(up.WelcomeScreen)),
    ),
  );
  const tree = makeTree(iconNames);
  const [center, menuHint, toolbarHint, helpHint] = [...container.children].map(tree);
  const clicks = [];
  for (const item of container.querySelectorAll(".welcome-screen-menu-item")) {
    globalThis.__ui.executed = [];
    await up.act(async () => item.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
    clicks.push({ text: item.querySelector(".welcome-screen-menu-item__text").textContent, executed: globalThis.__ui.executed });
  }
  await up.act(async () => root.unmount());
  return {
    name: c.name,
    formFactor: c.formFactor ?? "desktop",
    viewModeEnabled: !!c.viewModeEnabled,
    center,
    menuHint,
    toolbarHint,
    helpHint,
    clicks,
  };
};

const logoMarkup = async (up, window) => {
  const host = window.document.createElement("div");
  const root = up.createRoot(host);
  await up.act(async () => root.render(up.React.createElement(up.ExcalidrawLogo, { withText: true })));
  const markup = host.innerHTML;
  await up.act(async () => root.unmount());
  return `${markup}\n`;
};

// -- stylesheets ----------------------------------------------------------------

const compile = async (upstream, modules, generator) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString(modules.map((m) => `@use "${m}";\n`).join(""), {
    loadPaths: [root],
    style: "expanded",
  }).css;
  return (
    `/* Generated by tools/goldens/${generator}; do not edit. Upstream's\n` +
    modules.map((m) => ` * packages/excalidraw/${m}.scss\n`).join("") +
    " * at the pin, compiled with sass 1.51.0 (expanded). */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const localeKeys = (upstream) => {
  const en = JSON.parse(readFileSync(join(upstream.dir, "packages", "excalidraw", "locales", "en.json"), "utf8"));
  return [
    ...Object.keys(en.hints).map((k) => `hints.${k}`),
    ...Object.keys(en.welcomeScreen.defaults).map((k) => `welcomeScreen.defaults.${k}`),
    "keys.mmb",
    "buttons.load",
    "helpDialog.title",
  ];
};

export const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
  const hintsCss = await compile(upstream, ["components/HintViewer", "components/CursorHint"], "hints.mjs");
  const welcomeCss = await compile(
    upstream,
    ["components/welcome-screen/WelcomeScreen", "components/ExcalidrawLogo"],
    "hints.mjs",
  );
  const hintViewer = [];
  const welcome = [];
  let locale;
  let cursorHints;
  let position;
  let cursorHint;
  let logo;
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
    const darwin = /Mac|iPod|iPhone|iPad/.test(p.platform);
    const cases = [];
    for (const c of HINT_CASES) cases.push(await renderHint(up, window, c));
    hintViewer.push({ platform: p.name, darwin, cases });
    const welcomeCases = [];
    for (const c of WELCOME_CASES) welcomeCases.push(await renderWelcome(up, window, iconNames, c));
    welcome.push({ platform: p.name, darwin, cases: welcomeCases });
    if (p === PLATFORMS[0]) {
      locale = Object.fromEntries(localeKeys(upstream).map((k) => [k, up.t(k)]));
      cursorHints = cursorHintPolicy(up);
      position = positions(up);
      cursorHint = {
        cooldown: up.CURSOR_HINT_COOLDOWN,
        ...(await cursorHintDom(up, window, iconNames)),
      };
      logo = await logoMarkup(up, window);
    }
    window.close();
  }
  const fixture = { upstream: upstream.commit, locale, hintViewer, cursorHints, position, cursorHint, welcome };
  return { [FIXTURE]: format(fixture), [HINTS_CSS]: hintsCss, [WELCOME_CSS]: welcomeCss, [LOGO]: logo };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`hints: ${error.message}\n`);
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
      process.stderr.write("hints goldens are out of date: run node tools/goldens/hints.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`hints goldens up to date: ${Object.keys(files).length} files\n`);
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
