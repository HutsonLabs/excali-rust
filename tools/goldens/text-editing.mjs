#!/usr/bin/env node
// Text editing goldens for excali-editor and excali-ui (ex-512): upstream's
// own text editor overlay (packages/excalidraw/wysiwyg/textWysiwyg.tsx) and
// the App methods around it (App.startTextEditing, handleTextWysiwyg,
// getMaxTextWidth, ...; packages/excalidraw/components/App.tsx), run from
// the pinned checkout under jsdom, and redrawTextBoundingBox
// (packages/element/src/textElement.ts:51-152, with the sticky note fit of
// stickyNote.ts and arrow labels).
//
//   node tools/goldens/text-editing.mjs            write the fixture
//   node tools/goldens/text-editing.mjs --check    exit 1 if it is stale
//   node tools/goldens/text-editing.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/text-editing.json:
//
// - `constants`: TEXT_VIEWPORT_PADDING, TEXT_TO_CENTER_SNAP_THRESHOLD,
//   CARET_FOLLOW_PADDING and the textarea's attributes and fixed styles;
// - `transforms`: getTransform (textWysiwyg.tsx:88-104) on a grid of sizes,
//   angles and zooms (the CSS transform the editor is placed with);
// - `redraw`: redrawTextBoundingBox(text, container, scene) on scenes of
//   every container type (rectangle, ellipse, diamond, rotated, arrow
//   label with and without a label position, sticky note) and free texts:
//   every element after, and the original container cache;
// - `sessions`: editing sessions. Each starts with App.startTextEditing
//   (a text created at a point or in a container, or an existing text),
//   then replays steps the way a browser delivers them to the textarea:
//   typed characters (the selection replaced, an `input` event), keys
//   (a `keydown` the editor handles or, when it does not prevent the
//   default, the browser's own edit: Enter, Backspace, Delete), inserted
//   text, a selection change, and a blur. After the start and each step
//   the case records the textarea (value, selection, the style values the
//   editor assigned), every element, the selection and editing state, and
//   what the editor asked the app to do (executeAction, scheduleCapture,
//   viewport translations).
//
// The App methods are upstream's source, cut out of App.tsx at the pinned
// commit and compiled into a stand-in class whose other members are the
// App's state and services (see STAND_IN). The editor's style is recorded
// as the values upstream assigns (`Object.assign(editable.style, {...})`),
// each as the string the CSSOM stores for it (numbers through String());
// jsdom's own CSS parser, which is not a browser's, is kept out.
//
// Text measures 10 px per UTF-16 code unit (upstream's test metric).
// Deterministic: Math.random throws while generating, ids come from a
// counter (nanoid is shimmed), timestamps are fixed and every element is
// built after reseed().

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const OUT_FILE = "text-editing.json";

const ORIGIN = "http://localhost";
const RANDOM_SEED = 7;
const NOW = 1_700_000_000_000;

/** The ids randomId() has handed out in the current case. */
let ids = 0;

// -- the App methods, from App.tsx ----------------------------------------------------

/** The members of App the text editor runs through, by how they start. */
const APP_MEMBERS = [
  "  public getEffectiveGridSize = () => {",
  "  private getTextCreationGridPoint = (x: number, y: number) => {",
  "  private getTextViewportOffsets = () => {",
  "  private getMaxTextWidth = () => {",
  "  private handleTextWysiwyg(",
  "  private deselectElements() {",
  "  private getSelectedTextElement(",
  "  public startTextEditing = ({",
  "  public insertNewElements = (elements: readonly ExcalidrawElement[]) => {",
  "  public insertNewElement = (element: ExcalidrawElement) => {",
  "  getTextWysiwygSnappedToCenterPosition(",
  "  public getCurrentItemStrokeWidth(elementType: ExcalidrawElement[\"type\"]) {",
];

/**
 * Cuts each member out of App.tsx: from its first line to the first line
 * after it that closes a class member (`  }` or `  };` at the class's
 * indentation).
 */
const cutMembers = (source) => {
  const lines = source.split("\n");
  return APP_MEMBERS.map((start) => {
    const from = lines.findIndex((line) => line.startsWith(start));
    if (from === -1) throw new Error(`App.tsx has no member starting ${JSON.stringify(start)}`);
    const to = lines.findIndex((line, i) => i > from && (line === "  }" || line === "  };"));
    if (to === -1) throw new Error(`unterminated App member ${JSON.stringify(start)}`);
    return lines.slice(from, to + 1).join("\n");
  });
};

/** App.tsx's value imports: name -> [module specifier, imported name]. */
const appImports = (source) => {
  const imports = new Map();
  const re = /^import\s+(?!type\b)([\s\S]*?)\s+from\s+"([^"]+)";/gm;
  for (const [, clause, spec] of source.matchAll(re)) {
    const named = clause.match(/\{([\s\S]*)\}/);
    if (named) {
      for (const part of named[1].split(",")) {
        const item = part.trim();
        if (!item || item.startsWith("type ")) continue;
        const [imported, local = imported] = item.split(/\s+as\s+/).map((s) => s.trim());
        imports.set(local, [spec, imported]);
      }
    }
    const head = clause.replace(/\{[\s\S]*\}/, "").replace(/,\s*$/, "").trim();
    if (head.startsWith("* as ")) imports.set(head.slice(5).trim(), [spec, "*"]);
    else if (head) imports.set(head, [spec, "default"]);
  }
  return imports;
};

/** A module specifier of App.tsx, resolved from the checkout root. */
const fromRoot = (spec) =>
  spec.startsWith("../")
    ? `./packages/excalidraw/${spec.slice(3)}`
    : spec.startsWith("./")
      ? `./packages/excalidraw/components/${spec.slice(2)}`
      : spec;

/**
 * The entry: upstream's modules the generator calls, and `StandInApp`, a
 * class holding the App members cut out of App.tsx with the imports they
 * use (App.tsx's own import lines, resolved from the root).
 */
const entry = (upstreamDir) => {
  const app = readFileSync(join(upstreamDir, "packages", "excalidraw", "components", "App.tsx"), "utf8");
  const members = cutMembers(app);
  const body = members.join("\n\n");
  const imports = appImports(app);
  const used = [...imports.keys()].filter((name) => new RegExp(`\\b${name}\\b`).test(body));
  const lines = used.map((name) => {
    const [spec, imported] = imports.get(name);
    if (imported === "*") return `import * as ${name} from "${fromRoot(spec)}";`;
    if (imported === "default") return `import ${name} from "${fromRoot(spec)}";`;
    return `import { ${imported === name ? name : `${imported} as ${name}`} } from "${fromRoot(spec)}";`;
  });
  return `
${lines.join("\n")}
import type { ExcalidrawElement } from "./packages/element/src/types";

export class StandInApp {
  [key: string]: any;
${body}
}

export {
  newElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newStickyNoteElement,
  newFrameElement,
} from "./packages/element/src/newElement";
export {
  redrawTextBoundingBox,
  DEFAULT_BOUND_TEXT_LABEL_POSITION,
} from "./packages/element/src/textElement";
export { originalContainerCache } from "./packages/element/src/containerCache";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { Scene } from "./packages/element/src/Scene";
export { AppViewport } from "./packages/excalidraw/components/App.viewport";
export { textWysiwyg, CARET_FOLLOW_PADDING } from "./packages/excalidraw/wysiwyg/textWysiwyg";
export {
  actionResetZoom,
  actionZoomIn,
  actionZoomOut,
} from "./packages/excalidraw/actions/actionCanvas";
export {
  actionDecreaseFontSize,
  actionIncreaseFontSize,
} from "./packages/excalidraw/actions/actionProperties";
export { actionSaveFileToDisk, actionSaveToActiveFile } from "./packages/excalidraw/actions/actionExport";
export {
  KEYS,
  TEXT_VIEWPORT_PADDING,
  TEXT_TO_CENTER_SNAP_THRESHOLD,
} from "@excalidraw/common";
export { mutateElement, newElementWith } from "./packages/element/src/mutateElement";
`;
};

// actionExport.tsx, actionProperties.tsx and actionCanvas.tsx render React
// panels; the editor only calls their keyTest (and the app performs the
// zoom actions), which never reach them.
const STUBS = [
  "react",
  "react/jsx-runtime",
  "packages/excalidraw/analytics",
  "packages/excalidraw/components/App",
  "packages/excalidraw/components/CheckboxItem",
  "packages/excalidraw/components/ColorPicker/ColorPicker",
  "packages/excalidraw/components/DarkModeToggle",
  "packages/excalidraw/components/FontPicker/FontPicker",
  "packages/excalidraw/components/IconButton",
  "packages/excalidraw/components/IconPicker",
  "packages/excalidraw/components/ProjectName",
  "packages/excalidraw/components/RadioSelection",
  "packages/excalidraw/components/Range",
  "packages/excalidraw/components/Toast",
  "packages/excalidraw/components/Tooltip",
  "packages/excalidraw/components/icons",
  "packages/excalidraw/data",
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/data/resave",
  "packages/excalidraw/fonts",
  "packages/excalidraw/hooks/useTextEditorFocus",
  "packages/excalidraw/i18n",
  "packages/excalidraw/shortcut",
];

const SHIMS = {
  // React's batching only groups renders; flushSync runs its callback
  "packages/excalidraw/reactUtils":
    "module.exports = { withBatchedUpdates: (fn) => fn, withBatchedUpdatesThrottled: (fn) => fn };",
  "react-dom": "module.exports = { flushSync: (fn) => fn() };",
  // register() only adds the action to the registry and returns it
  "packages/excalidraw/actions/register": "module.exports = { register: (action) => action };",
  // randomId() outside tests (nanoid): ids from the generator's counter
  nanoid: "module.exports = { nanoid: () => globalThis.__textEditingNextId() };",
};

// textWysiwyg.tsx imports actionSaveToActiveFile and actionSaveFileToDisk
// from the actions index, which re-exports them from actionExport.tsx
const REDIRECTS = { "packages/excalidraw/actions": "packages/excalidraw/actions/actionExport.tsx" };

const usage = () => {
  process.stderr.write("usage: text-editing.mjs [--check] [--out DIR]\n");
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

/** JSON round trip: what the fixture holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

/**
 * An element as the fixture holds it: every key but the drawn ones
 * (`seed`, `versionNonce` and `updated` come from the random generator and
 * the clock, which a port draws from its own environment).
 */
const elementOut = (element) => {
  const { seed, versionNonce, updated, ...rest } = plain(element);
  return rest;
};

/**
 * The elements that differ from `before` (by id, compared as JSON), and
 * the scene's order when it changed: a step records only what it changed.
 */
const sceneDiff = (before, after) => {
  const prev = new Map(before.map((e) => [e.id, JSON.stringify(e)]));
  const changed = after.filter((e) => prev.get(e.id) !== JSON.stringify(e));
  const order = after.map((e) => e.id);
  const sameOrder = order.length === before.length && order.every((id, i) => before[i].id === id);
  return { changed, order: sameOrder ? null : order };
};

/** The keys of `after` whose values differ from `before`'s (as JSON). */
const changedKeys = (before, after) =>
  Object.fromEntries(Object.entries(after).filter(([k, v]) => JSON.stringify(before[k]) !== JSON.stringify(v)));

// -- the textarea's style, as assigned ---------------------------------------------------

/**
 * The textarea's `style`, recording the values upstream assigns, each as
 * the string the CSSOM keeps (a number through String()). Reading `font`'s
 * longhands gives what a browser's shorthand parse gives for upstream's
 * font strings (`${size}px ${families}`): `fontSize` the size and
 * `fontFamily` the family list (a browser quotes names with spaces, which
 * textPropertiesUpdated strips).
 */
const recordingStyle = () => {
  const assigned = {};
  const longhands = {};
  const style = new Proxy(
    {},
    {
      set(_, key, value) {
        assigned[key] = String(value);
        if (key === "font") {
          const font = String(value);
          const space = font.indexOf(" ");
          longhands.fontSize = font.slice(0, space);
          longhands.fontFamily = font.slice(space + 1);
        }
        return true;
      },
      get(_, key) {
        if (key === "fontSize" || key === "fontFamily") return longhands[key] ?? "";
        return assigned[key] ?? "";
      },
    },
  );
  return { assigned, style };
};

// -- the stand-in App ------------------------------------------------------------------------

/** The app state keys a case sets and the fixture records. */
export const STATE_KEYS = [
  "zoom",
  "scrollX",
  "scrollY",
  "width",
  "height",
  "offsetLeft",
  "offsetTop",
  "theme",
  "activeTool",
  "gridModeEnabled",
  "gridSize",
  "currentItemStrokeColor",
  "currentItemBackgroundColor",
  "currentItemFillStyle",
  "currentItemStrokeWidthKey",
  "currentItemStrokeStyle",
  "currentItemRoughness",
  "currentItemOpacity",
  "currentItemFontSize",
  "currentItemFontFamily",
  "currentItemTextAlign",
  "selectedElementIds",
  "selectedGroupIds",
  "editingGroupId",
  "activeEmbeddable",
  "editingTextElement",
  "newElement",
  "multiElement",
];

/** The recorded app state: an element in the state by its id. */
const stateOut = (state) =>
  Object.fromEntries(
    STATE_KEYS.map((key) => {
      const value = state[key];
      if (key === "zoom") return [key, value.value];
      if (["editingTextElement", "newElement", "multiElement"].includes(key)) return [key, value ? value.id : null];
      return [key, value === undefined ? null : plain(value)];
    }),
  );

const BASE_STATE = { width: 1000, height: 800, offsetLeft: 0, offsetTop: 0 };

/**
 * A stand-in App: the members cut out of App.tsx over upstream's Scene and
 * AppViewport, with the rest of what they call answered here. `log`
 * records, in order, what the editor asks of the app.
 */
const makeApp = (up, window, c, elements) => {
  const { document } = window;
  const log = [];
  const container = document.createElement("div");
  container.className = "excalidraw";
  const editorBox = document.createElement("div");
  editorBox.className = "excalidraw-textEditorContainer";
  container.append(editorBox);
  document.body.append(container);

  const app = new up.StandInApp();
  app.state = {
    ...up.getDefaultAppState(),
    ...BASE_STATE,
    ...(c.state ?? {}),
    zoom: { value: c.state?.zoom ?? 1 },
  };
  const scrollListeners = new Set();
  app.props = {};
  app.setState = (update, callback) => {
    const prev = app.state;
    const next = typeof update === "function" ? update(prev, app.props) : update;
    if (next) app.state = { ...prev, ...next };
    // componentDidUpdate (App.tsx:4356-4372)
    if (
      prev.zoom.value !== app.state.zoom.value ||
      prev.scrollX !== app.state.scrollX ||
      prev.scrollY !== app.state.scrollY
    ) {
      for (const listener of [...scrollListeners]) listener(app.state.scrollX, app.state.scrollY, app.state.zoom);
    }
    callback?.();
  };
  app.scene = new up.Scene(elements);
  app.store = { scheduleCapture: () => log.push("scheduleCapture") };
  app.isToolLocked = () => app.state.activeTool.locked;
  app.cursor = { applyForTool: () => log.push("applyToolCursor") };
  app.focusContainer = () => log.push("focusContainer");
  app.editorInterface = { isTouchScreen: false, formFactor: "desktop" };
  // texts at arrow endpoints (App.arrowText) belong to arrow binding
  app.arrowText = { getTextBinding: () => null };
  // hit testing is the case's: the text and the frame under the point
  app.getTextElementAtPosition = () => (c.hitText ? app.scene.getNonDeletedElement(c.hitText) : null);
  app.getTopLayerFrameAtSceneCoords = () => (c.hitFrame ? app.scene.getNonDeletedElement(c.hitFrame) : null);
  app.updateFrameToHighlight = () => {};
  app.canvas = document.createElement("canvas");
  app.excalidrawContainerRef = { current: container };
  const changeListeners = new Set();
  app.onChangeEmitter = {
    on: (listener) => {
      changeListeners.add(listener);
      return () => changeListeners.delete(listener);
    },
    // App's onChange after a render (App.tsx componentDidUpdate)
    trigger: () => {
      for (const listener of [...changeListeners]) listener(app.scene.getElementsIncludingDeleted());
    },
  };
  app.onScrollChangeEmitter = {
    on: (listener) => {
      scrollListeners.add(listener);
      return () => scrollListeners.delete(listener);
    },
  };
  app.pan = { start: () => log.push("panStart"), isActive: () => false, flushMove: () => {} };
  app.requestUnfollow = () => {};
  app.resetShouldCacheIgnoreZoomDebounced = () => {};
  app.unmounted = true;
  app.isNavigationEnabled = () => true;
  app.actionManager = {
    executeAction: (action) => {
      log.push(`executeAction:${action.name}`);
      // the zoom actions change what the editor reads back; the others
      // (font size, save) are the app's and recorded only
      if (["zoomIn", "zoomOut", "resetZoom"].includes(action.name)) {
        const result = action.perform(app.scene.getElementsIncludingDeleted(), app.state, null, app);
        app.setState(result.appState);
      }
    },
  };
  app.textWysiwygSubmitHandler = null;
  app.viewport = new up.AppViewport(app, {
    getContainer: () => null,
    getStylesPanelMode: () => "full",
    isGestureActive: () => false,
  });
  if (c.sidebar) app.viewport.getSidebarInsets = () => ({ ...c.sidebar });
  const translate = app.viewport.translate.bind(app.viewport);
  app.viewport.translate = (update) => {
    log.push("translate");
    return translate(update);
  };
  return { app, log, container, editorBox };
};

// -- the browser's part: timers, events and default edits -----------------------------------

/**
 * Timers and animation frames queued instead of scheduled, run when the
 * generator flushes them (after each step, as a browser runs them before
 * the next input).
 */
const installClock = (window) => {
  const queue = [];
  window.setTimeout = (fn) => queue.push(fn);
  window.requestAnimationFrame = (fn) => queue.push(fn);
  return () => {
    while (queue.length) queue.shift()();
  };
};

const CHAR_CODES = { " ": "Space", "=": "Equal", "-": "Minus", ".": "Period", ",": "Comma", "[": "BracketLeft", "]": "BracketRight" };

/** The keydown a US keyboard sends for a typed character. */
const charEvent = (ch) => ({
  key: ch,
  code: /^[a-z]$/i.test(ch) ? `Key${ch.toUpperCase()}` : /^[0-9]$/.test(ch) ? `Digit${ch}` : (CHAR_CODES[ch] ?? ""),
  shiftKey: /^[A-Z]$/.test(ch),
  altKey: false,
  ctrlKey: false,
  metaKey: false,
});

/** The keys a case presses: code -> [key, key with shift]. */
const KEYS_BY_CODE = {
  Enter: ["Enter", "Enter"],
  Escape: ["Escape", "Escape"],
  Tab: ["Tab", "Tab"],
  Backspace: ["Backspace", "Backspace"],
  Delete: ["Delete", "Delete"],
  Equal: ["=", "+"],
  Minus: ["-", "_"],
  Digit0: ["0", ")"],
  Period: [".", ">"],
  Comma: [",", "<"],
  BracketLeft: ["[", "{"],
  BracketRight: ["]", "}"],
  KeyS: ["s", "S"],
};

/**
 * "CtrlOrCmd+Shift+Period" → the keydown's key, code and modifiers.
 * CtrlOrCmd is ctrlKey here (jsdom's platform is not a Mac, KEYS.CTRL_OR_CMD);
 * the fixture keeps the name, so a port presses its platform's key.
 */
const keyEvent = (spec) => {
  const parts = spec.split("+");
  const code = parts.pop();
  const mods = new Set(parts);
  const keys = KEYS_BY_CODE[code];
  if (!keys) throw new Error(`no key ${code}`);
  return {
    key: keys[mods.has("Shift") ? 1 : 0],
    code,
    shiftKey: mods.has("Shift"),
    altKey: mods.has("Alt"),
    ctrlKey: mods.has("CtrlOrCmd"),
    metaKey: false,
  };
};

/** Replaces the textarea's selection with `text`, the caret after it. */
const insert = (editable, text) => {
  const { selectionStart, selectionEnd, value } = editable;
  editable.value = value.slice(0, selectionStart) + text + value.slice(selectionEnd);
  editable.selectionStart = editable.selectionEnd = selectionStart + text.length;
};

/** The browser's Backspace (-1) and Delete (1) in a textarea of ASCII text. */
const deleteBy = (editable, direction) => {
  const { selectionStart, selectionEnd, value } = editable;
  if (selectionStart !== selectionEnd) {
    insert(editable, "");
    return true;
  }
  if (direction < 0 && selectionStart > 0) {
    editable.value = value.slice(0, selectionStart - 1) + value.slice(selectionEnd);
    editable.selectionStart = editable.selectionEnd = selectionStart - 1;
    return true;
  }
  if (direction > 0 && selectionEnd < value.length) {
    editable.value = value.slice(0, selectionStart) + value.slice(selectionEnd + 1);
    editable.selectionStart = editable.selectionEnd = selectionStart;
    return true;
  }
  return false;
};

/**
 * A keydown the way a browser delivers it: to the editor's handler, then,
 * unless the handler prevented it or closed the editor, the browser's own
 * edit (Enter inserts a line break, Backspace and Delete delete, a
 * printable key without ctrl/cmd/alt inserts itself) and its `input`.
 */
const keydown = (window, editable, event) => {
  const e = new window.KeyboardEvent("keydown", { ...event, bubbles: true, cancelable: true });
  editable.dispatchEvent(e);
  if (e.defaultPrevented || !editable.isConnected) return;
  if (event.ctrlKey || event.metaKey || event.altKey) return;
  let changed = false;
  if (event.key === "Enter") {
    insert(editable, "\n");
    changed = true;
  } else if (event.key === "Backspace") {
    changed = deleteBy(editable, -1);
  } else if (event.key === "Delete") {
    changed = deleteBy(editable, 1);
  } else if (event.key.length === 1) {
    insert(editable, event.key);
    changed = true;
  }
  if (changed) editable.dispatchEvent(new window.Event("input"));
};

// -- scenes ------------------------------------------------------------------------------------

const FONT = { fontSize: 20, fontFamily: 5 };

const NO_ELEMENTS = new Map();

/** Binds `text` to `container` as upstream's model has it. */
const bind = (up, container, text) => {
  up.mutateElement(container, NO_ELEMENTS, {
    boundElements: [...(container.boundElements ?? []), { type: "text", id: text.id }],
  });
  up.mutateElement(text, NO_ELEMENTS, { containerId: container.id });
};

/** A text as newTextElement builds it. */
const text = (up, id, content, x, y, opts = {}) => up.newTextElement({ id, text: content, x, y, ...FONT, ...opts });

const shape = (up, type, id, x, y, width, height, opts = {}) =>
  up.newElement({ type, id, x, y, width, height, strokeColor: "#1e1e1e", ...opts });

const arrow = (up, id, x, y, points) => {
  const a = up.newArrowElement({ type: "arrow", id, x, y, points });
  up.mutateElement(a, NO_ELEMENTS, { points });
  return a;
};

const sticky = (up, id, x, y, size) =>
  up.newStickyNoteElement({ type: "stickynote", id, x, y, width: size, height: size });

/** A centred label bound to a container. */
const label = (up, container, id, content, opts = {}) => {
  const t = text(up, id, content, 0, 0, { textAlign: "center", verticalAlign: "middle", ...opts });
  bind(up, container, t);
  return t;
};

/** A sticky note's label as the app creates it (the note's ink, a ceiling). */
const stickyLabel = (up, note, id, content) => {
  const t = text(up, id, content, 0, 0, {
    textAlign: "center",
    verticalAlign: "middle",
    fontSize: 28,
    baseFontSize: 28,
    strokeColor: note.strokeColor,
  });
  bind(up, note, t);
  return t;
};

/** Scene builders by name, each run after reseed(). */
export const SCENES = {
  empty: () => [],
  freeText: (up) => [text(up, "t", "hello", 100, 100)],
  multiline: (up) => [text(up, "t", "one\ntwo\nthree", 100, 100)],
  indented: (up) => [text(up, "t", "    four\n  two\nnone", 100, 100)],
  fixedWidth: (up) => {
    const t = text(up, "t", "a fixed width text", 100, 100);
    up.mutateElement(t, NO_ELEMENTS, { autoResize: false, width: 60 });
    return [t];
  },
  styled: (up) => [text(up, "t", "styled", 40, 60, { strokeColor: "#1971c2", opacity: 60, angle: 0.3 })],
  rightAligned: (up) => [text(up, "t", "right", 300, 100, { textAlign: "right" })],
  rectangle: (up) => [shape(up, "rectangle", "r", 50, 50, 160, 80)],
  smallRectangle: (up) => [shape(up, "rectangle", "r", 50, 50, 20, 10)],
  rectangleLabel: (up) => {
    const r = shape(up, "rectangle", "r", 50, 50, 160, 80);
    return [r, label(up, r, "t", "label")];
  },
  ellipseLabel: (up) => {
    const e = shape(up, "ellipse", "e", 30, 40, 200, 120);
    return [e, label(up, e, "t", "ellipse")];
  },
  diamondRotated: (up) => {
    const d = shape(up, "diamond", "d", 80, 60, 180, 140, { angle: 0.5 });
    return [d, label(up, d, "t", "turn", { angle: 0.5 })];
  },
  topLeftLabel: (up) => {
    const r = shape(up, "rectangle", "r", 50, 50, 200, 100);
    return [r, label(up, r, "t", "corner", { textAlign: "left", verticalAlign: "top" })];
  },
  arrow: (up) => [arrow(up, "a", 100, 100, [[0, 0], [200, 60]])],
  arrowLabel: (up) => {
    const a = arrow(up, "a", 100, 100, [[0, 0], [120, 40], [240, 0]]);
    return [a, label(up, a, "t", "via", { labelPosition: 0.25 })];
  },
  arrowLabelMiddle: (up) => {
    const a = arrow(up, "a", 60, 200, [[0, 0], [300, -80]]);
    return [a, label(up, a, "t", "mid")];
  },
  sticky: (up) => [sticky(up, "s", 100, 100, 200)],
  stickyLabel: (up) => {
    const s = sticky(up, "s", 100, 100, 200);
    return [s, stickyLabel(up, s, "t", "note")];
  },
  twoTexts: (up) => [text(up, "t", "first", 100, 100), text(up, "u", "second", 100, 200)],
  frame: (up) => [up.newFrameElement({ id: "f", x: 0, y: 0, width: 600, height: 400 })],
};

// -- redrawTextBoundingBox ------------------------------------------------------------------------

/**
 * [name, scene, text id, container id or null, updates made to the scene
 * first (by id), as the undone or redone delta leaves them].
 */
const REDRAW_CASES = [
  ["rectangle-grows-to-text", "rectangleLabel", "t", "r", { t: { originalText: "a much longer label than fits" } }],
  ["rectangle-narrowed", "rectangleLabel", "t", "r", { r: { width: 40 } }],
  ["rectangle-rotated", "rectangleLabel", "t", "r", { r: { angle: 1.2 } }],
  ["rectangle-top-left", "topLeftLabel", "t", "r", { t: { originalText: "top\nleft" } }],
  ["rectangle-moved", "rectangleLabel", "t", "r", { r: { x: 200, y: 300 } }],
  ["ellipse", "ellipseLabel", "t", "e", { t: { originalText: "an ellipse label that wraps" } }],
  ["diamond-rotated", "diamondRotated", "t", "d", { t: { originalText: "diamond label" } }],
  ["arrow-label-position", "arrowLabel", "t", "a", { t: { originalText: "a label on an arrow" } }],
  ["arrow-label-middle", "arrowLabelMiddle", "t", "a", { t: { originalText: "middle" } }],
  ["arrow-label-moved", "arrowLabel", "t", "a", { a: { x: 10, y: 20 } }],
  ["sticky-fits", "stickyLabel", "t", "s", { t: { originalText: "a sticky note label" } }],
  [
    "sticky-shrinks-font",
    "stickyLabel",
    "t",
    "s",
    { t: { originalText: "many words on a sticky note that will not fit at the ceiling size at all" } },
  ],
  [
    "sticky-grows",
    "stickyLabel",
    "t",
    "s",
    { t: { originalText: "line\nline\nline\nline\nline\nline\nline\nline\nline\nline\nline\nline" } },
  ],
  ["sticky-blank", "stickyLabel", "t", "s", { t: { originalText: "   " } }],
  ["sticky-resized", "stickyLabel", "t", "s", { s: { width: 120 } }],
  ["free-auto-resize", "freeText", "t", null, { t: { originalText: "grown\ntext" } }],
  ["free-fixed-width", "fixedWidth", "t", null, { t: { originalText: "wraps at its own width" } }],
];

const redraw = (up) =>
  REDRAW_CASES.map(([name, scene, textId, containerId, updates]) => {
    ids = 0;
    up.reseed(RANDOM_SEED);
    for (const key of Object.keys(up.originalContainerCache)) delete up.originalContainerCache[key];
    const s = new up.Scene(SCENES[scene](up));
    for (const [id, update] of Object.entries(updates)) {
      up.mutateElement(s.getElement(id), NO_ELEMENTS, update);
    }
    const before = s.getElementsIncludingDeleted().map(elementOut);
    up.redrawTextBoundingBox(s.getElement(textId), containerId ? s.getElement(containerId) : null, s);
    const after = s.getElementsIncludingDeleted().map(elementOut);
    return {
      name,
      text: textId,
      container: containerId,
      elements: before,
      changed: sceneDiff(before, after).changed,
      containerCache: plain(up.originalContainerCache),
    };
  });

// -- editing sessions -----------------------------------------------------------------------------

const TEXT_TOOL = { type: "text", customType: null, locked: false, fromSelection: false, lastActiveTool: null };

/**
 * Each session: the scene, the app state it sets, what the hit tests
 * answer, how editing starts (startTextEditing's arguments, elements by
 * id) and the steps.
 */
export const SESSIONS = [
  {
    name: "free-text-typed",
    scene: "freeText",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ press: "Delete" }, { type: "Hi" }, { press: "Enter" }, { type: "there" }, { press: "Escape" }],
  },
  {
    name: "free-text-created",
    scene: "empty",
    state: { activeTool: TEXT_TOOL },
    start: { sceneX: 200, sceneY: 150 },
    steps: [{ type: "abc" }, { press: "Enter" }, { type: "de" }, { press: "Backspace" }, { press: "Escape" }],
  },
  {
    name: "free-text-created-then-blurred-empty",
    scene: "empty",
    state: { activeTool: TEXT_TOOL },
    start: { sceneX: 40, sceneY: 30 },
    steps: [{ blur: true }],
  },
  {
    name: "free-text-cleared",
    scene: "freeText",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ press: "Backspace" }, { press: "Escape" }],
  },
  {
    name: "free-text-blurred",
    scene: "freeText",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ select: [5, 5] }, { type: " world" }, { blur: true }],
  },
  {
    name: "indent-and-outdent",
    scene: "multiline",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [
      { press: "Tab" },
      { select: [5, 9] },
      { press: "Tab" },
      { press: "Shift+Tab" },
      { press: "CtrlOrCmd+BracketRight" },
      { press: "CtrlOrCmd+BracketLeft" },
      { select: [0, 13] },
      { press: "Shift+Tab" },
      { press: "CtrlOrCmd+Enter" },
    ],
  },
  {
    name: "outdent-partial-tabs",
    scene: "indented",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ select: [6, 16] }, { press: "Shift+Tab" }, { select: [2, 2] }, { press: "Shift+Tab" }, { press: "Escape" }],
  },
  {
    name: "inserted-tabs-normalized",
    scene: "freeText",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ select: [5, 5] }, { insertText: "\tand\ttabs" }, { press: "Escape" }],
  },
  {
    name: "fixed-width-wraps",
    scene: "fixedWidth",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ select: [18, 18] }, { type: " that keeps going" }, { press: "Escape" }],
  },
  {
    name: "styled-rotated-zoomed-dark",
    scene: "styled",
    state: { zoom: 1.5, scrollX: -30, scrollY: 20, offsetLeft: 10, offsetTop: 5, theme: "dark" },
    start: { sceneX: 45, sceneY: 65, textElement: "t" },
    steps: [
      { select: [6, 6] },
      { type: "!" },
      { press: "CtrlOrCmd+Equal" },
      { press: "CtrlOrCmd+Minus" },
      { press: "CtrlOrCmd+Minus" },
      { press: "CtrlOrCmd+Digit0" },
      { press: "Shift+Equal" },
      { press: "Escape" },
    ],
  },
  {
    name: "right-aligned-grows-left",
    scene: "rightAligned",
    start: { sceneX: 310, sceneY: 110, textElement: "t" },
    steps: [{ select: [0, 0] }, { type: "to the " }, { press: "Escape" }],
  },
  {
    name: "wraps-at-the-view-width",
    scene: "empty",
    state: { width: 300, height: 200, scrollX: -100, activeTool: TEXT_TOOL },
    sidebar: { left: 0, right: 40 },
    start: { sceneX: 150, sceneY: 60 },
    steps: [{ type: "this line is long enough to wrap" }, { type: " again and again" }, { press: "Escape" }],
  },
  {
    name: "grid-snapped-creation",
    scene: "empty",
    state: { gridModeEnabled: true, gridSize: 20, activeTool: TEXT_TOOL },
    start: { sceneX: 57, sceneY: 43 },
    steps: [{ type: "grid" }, { press: "Escape" }],
  },
  {
    name: "created-in-frame",
    scene: "frame",
    hitFrame: "f",
    state: { activeTool: TEXT_TOOL, currentItemFontSize: 28, currentItemTextAlign: "center", currentItemStrokeColor: "#e03131" },
    start: { sceneX: 100, sceneY: 100 },
    steps: [{ type: "framed" }, { press: "Escape" }],
  },
  {
    name: "rectangle-label-created",
    scene: "rectangle",
    start: { sceneX: 130, sceneY: 90, container: "r" },
    steps: [
      { type: "a label long enough to wrap" },
      { press: "Enter" },
      { type: "and grow" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Backspace" },
      { press: "Escape" },
    ],
  },
  {
    name: "small-rectangle-label-created",
    scene: "smallRectangle",
    start: { sceneX: 60, sceneY: 55, container: "r" },
    steps: [{ type: "tiny" }, { press: "Escape" }],
  },
  {
    name: "rectangle-label-off-centre",
    scene: "rectangle",
    state: { activeTool: TEXT_TOOL },
    start: { sceneX: 60, sceneY: 60, container: "r" },
    steps: [{ type: "off" }, { press: "Escape" }],
  },
  {
    name: "rectangle-label-edited",
    scene: "rectangleLabel",
    state: { selectedElementIds: { r: true } },
    start: { sceneX: 130, sceneY: 90, container: "r" },
    steps: [{ select: [5, 5] }, { type: " with more words" }, { press: "Enter" }, { type: "x" }, { press: "CtrlOrCmd+Enter" }],
  },
  {
    name: "rectangle-label-cleared",
    scene: "rectangleLabel",
    state: { selectedElementIds: { r: true } },
    start: { sceneX: 130, sceneY: 90, container: "r" },
    steps: [{ press: "Delete" }, { press: "Escape" }],
  },
  {
    name: "ellipse-label",
    scene: "ellipseLabel",
    state: { selectedElementIds: { e: true } },
    start: { sceneX: 130, sceneY: 100, container: "e" },
    steps: [{ select: [7, 7] }, { type: " grows taller as lines are added" }, { blur: true }],
  },
  {
    name: "rotated-diamond-label",
    scene: "diamondRotated",
    state: { selectedElementIds: { d: true } },
    start: { sceneX: 170, sceneY: 130, container: "d" },
    steps: [{ select: [4, 4] }, { type: " around" }, { press: "Enter" }, { type: "twice" }, { press: "Escape" }],
  },
  {
    name: "top-left-label",
    scene: "topLeftLabel",
    state: { selectedElementIds: { r: true } },
    start: { sceneX: 150, sceneY: 100, container: "r" },
    steps: [{ select: [6, 6] }, { press: "Enter" }, { type: "below" }, { press: "Escape" }],
  },
  {
    name: "arrow-label-created",
    scene: "arrow",
    start: { sceneX: 200, sceneY: 130, container: "a" },
    steps: [{ type: "on the arrow" }, { press: "Escape" }],
  },
  {
    name: "arrow-label-edited",
    scene: "arrowLabel",
    start: { sceneX: 160, sceneY: 120, container: "a" },
    steps: [{ select: [3, 3] }, { type: " the quarter" }, { press: "Escape" }],
  },
  {
    name: "sticky-note-label-created",
    scene: "sticky",
    start: { sceneX: 200, sceneY: 200, container: "s" },
    steps: [
      { type: "a note" },
      { type: " with enough words to make the font step down" },
      { press: "Enter" },
      { type: "and more" },
      { press: "Escape" },
    ],
  },
  {
    name: "sticky-note-label-edited",
    scene: "stickyLabel",
    state: { selectedElementIds: { s: true } },
    start: { sceneX: 200, sceneY: 200, container: "s" },
    steps: [
      { select: [4, 4] },
      { press: "Enter" },
      { type: "a" },
      { press: "Enter" },
      { type: "b" },
      { press: "Enter" },
      { type: "c" },
      { press: "Enter" },
      { type: "d" },
      { press: "Enter" },
      { type: "e" },
      { press: "Enter" },
      { type: "f" },
      { press: "Enter" },
      { type: "g" },
      { press: "Escape" },
    ],
  },
  {
    name: "selected-text-resolved",
    scene: "twoTexts",
    state: { selectedElementIds: { u: true } },
    start: { sceneX: 500, sceneY: 500 },
    steps: [{ type: "replaced" }, { press: "Escape" }],
  },
  {
    name: "text-under-the-pointer-resolved",
    scene: "twoTexts",
    hitText: "t",
    start: { sceneX: 110, sceneY: 110 },
    steps: [{ press: "Delete" }, { press: "Escape" }],
  },
  {
    name: "locked-tool-keyboard-submit",
    scene: "empty",
    state: { activeTool: { ...TEXT_TOOL, locked: true } },
    start: { sceneX: 300, sceneY: 300 },
    steps: [{ type: "kept" }, { press: "Escape" }],
  },
  {
    name: "caret-placed-at-the-point",
    scene: "multiline",
    start: { sceneX: 110, sceneY: 110, textElement: "t", initialCaretSceneCoords: { x: 112, y: 131 } },
    steps: [{ type: "X" }, { press: "Escape" }],
  },
  {
    name: "caret-placed-in-a-rotated-label",
    scene: "diamondRotated",
    state: { selectedElementIds: { d: true } },
    start: { sceneX: 170, sceneY: 130, container: "d", initialCaretSceneCoords: { x: 160, y: 128 } },
    steps: [{ type: "Y" }, { press: "Escape" }],
  },
  {
    name: "caret-placed-below-the-last-line",
    scene: "indented",
    start: { sceneX: 110, sceneY: 110, textElement: "t", initialCaretSceneCoords: { x: 400, y: 190 } },
    steps: [{ press: "Escape" }],
  },
  {
    name: "editor-box-scrolled-to-the-caret",
    scene: "freeText",
    state: { zoom: 2, scrollX: 10, scrollY: -5 },
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [{ boxScroll: [30, 0] }, { boxScroll: [0, 12] }, { boxScroll: [7, 9] }, { boxScroll: [0, 0] }, { press: "Escape" }],
  },
  {
    name: "theme-changed-while-editing",
    scene: "styled",
    start: { sceneX: 45, sceneY: 65, textElement: "t" },
    steps: [{ theme: "dark" }, { theme: "light" }, { press: "Escape" }],
  },
  {
    name: "resized-while-editing",
    scene: "empty",
    state: { activeTool: TEXT_TOOL },
    start: { sceneX: 20, sceneY: 30 },
    steps: [{ resize: [260, 200] }, { type: "typed until it wraps at the new width" }, { press: "Escape" }],
  },
  {
    name: "container-moved-while-editing",
    scene: "rectangleLabel",
    state: { selectedElementIds: { r: true } },
    start: { sceneX: 130, sceneY: 90, container: "r" },
    steps: [{ mutate: ["r", { x: 300, y: 200 }] }, { type: "!" }, { mutate: ["r", { angle: 0.4 }] }, { press: "Escape" }],
  },
  {
    name: "app-shortcuts-while-editing",
    scene: "freeText",
    start: { sceneX: 110, sceneY: 110, textElement: "t" },
    steps: [
      { press: "CtrlOrCmd+Shift+Period" },
      { press: "CtrlOrCmd+Shift+Comma" },
      { press: "CtrlOrCmd+KeyS" },
    ],
  },
];

/** startTextEditing's arguments, elements by id. */
const startArgs = (app, start) => {
  const args = { sceneX: start.sceneX, sceneY: start.sceneY };
  if (start.container) args.container = app.scene.getElement(start.container);
  if (start.textElement) args.textElement = app.scene.getElement(start.textElement);
  if (start.insertAtParentCenter !== undefined) args.insertAtParentCenter = start.insertAtParentCenter;
  if (start.initialCaretSceneCoords) args.initialCaretSceneCoords = start.initialCaretSceneCoords;
  return args;
};

const session = (up, window, flush, c) => {
  ids = 0;
  up.reseed(RANDOM_SEED);
  for (const key of Object.keys(up.originalContainerCache)) delete up.originalContainerCache[key];
  const { document } = window;
  const elements = SCENES[c.scene](up);
  const { app, log, container, editorBox } = makeApp(up, window, c, elements);

  // the editor's textarea, with its style recorded
  let editable = null;
  let assigned = null;
  const createElement = document.createElement;
  document.createElement = function create(tag, ...rest) {
    const el = createElement.call(this, tag, ...rest);
    if (tag === "textarea") {
      const recorded = recordingStyle();
      Object.defineProperty(el, "style", { value: recorded.style });
      editable = el;
      assigned = recorded.assigned;
    }
    return el;
  };

  let scene = app.scene.getElementsIncludingDeleted().map(elementOut);
  let state = stateOut(app.state);
  let style = {};
  const initial = { state, elements: scene };
  let logged = 0;
  /** What changed since the last record: elements, style and state keys. */
  const record = (step) => {
    const now = app.scene.getElementsIncludingDeleted().map(elementOut);
    const { changed, order } = sceneDiff(scene, now);
    scene = now;
    const nextState = stateOut(app.state);
    const stateChanged = changedKeys(state, nextState);
    state = nextState;
    const nextStyle = assigned ? { ...assigned } : {};
    const styleChanged = changedKeys(style, nextStyle);
    style = nextStyle;
    const out = {
      step,
      open: !!editable?.isConnected,
      value: editable?.value ?? null,
      selection: editable ? [editable.selectionStart, editable.selectionEnd] : null,
      style: styleChanged,
      changed,
      order,
      state: stateChanged,
      calls: log.slice(logged),
      containerCache: plain(up.originalContainerCache),
    };
    logged = log.length;
    return out;
  };

  try {
    app.startTextEditing(startArgs(app, c.start));
    flush();
    const records = [record(null)];
    const attributes = editable && {
      tagName: editable.tagName.toLowerCase(),
      dir: editable.dir,
      tabIndex: editable.tabIndex,
      dataType: editable.dataset.type,
      wrap: editable.wrap,
      className: editable.className,
      parentClassName: editable.parentElement?.className ?? null,
    };
    for (const step of c.steps) {
      if (step.type !== undefined) {
        for (const ch of step.type) keydown(window, editable, charEvent(ch));
      } else if (step.press) {
        keydown(window, editable, keyEvent(step.press));
      } else if (step.insertText !== undefined) {
        insert(editable, step.insertText);
        editable.dispatchEvent(new window.Event("input"));
      } else if (step.select) {
        editable.setSelectionRange(step.select[0], step.select[1]);
      } else if (step.blur) {
        editable.blur();
      } else if (step.boxScroll) {
        // the browser revealing the caret by scrolling the editor's box
        for (const [key, value] of [
          ["scrollLeft", step.boxScroll[0]],
          ["scrollTop", step.boxScroll[1]],
        ]) {
          Object.defineProperty(editorBox, key, { value, writable: true, configurable: true });
        }
        editorBox.dispatchEvent(new window.Event("scroll"));
      } else if (step.theme) {
        app.setState({ theme: step.theme });
        app.onChangeEmitter.trigger();
      } else if (step.resize) {
        // the canvas resized: no ResizeObserver in jsdom, so the window's
        // resize listener the editor falls back to
        app.setState({ width: step.resize[0], height: step.resize[1] });
        window.dispatchEvent(new window.Event("resize"));
      } else if (step.mutate) {
        // a change from elsewhere (a collaborator): the scene's update
        const [id, updates] = step.mutate;
        app.scene.mutateElement(app.scene.getElement(id), updates);
      } else {
        throw new Error(`unknown step ${JSON.stringify(step)}`);
      }
      flush();
      records.push(record(step));
    }
    return {
      name: c.name,
      scene: c.scene,
      sidebar: c.sidebar ?? { left: 0, right: 0 },
      hitText: c.hitText ?? null,
      hitFrame: c.hitFrame ?? null,
      start: c.start,
      ids: Array.from({ length: ids }, (_, i) => `id${i}`),
      initial,
      attributes,
      records,
    };
  } finally {
    document.createElement = createElement;
    // an editor left open (no submit step) is closed with the scene
    app.textWysiwygSubmitHandler = null;
    container.remove();
  }
};

// -- running upstream --------------------------------------------------------------------------

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  globalThis.Event = window.Event;
  globalThis.KeyboardEvent = window.KeyboardEvent;
  globalThis.__textEditingNextId = () => `id${ids++}`;
  const flush = installClock(window);
  const up = await loadUpstream(upstream, {
    entry: entry(upstream.dir),
    stubs: STUBS,
    shims: SHIMS,
    redirects: REDIRECTS,
    define: {
      // the production build: no test-environment branches, and nanoid ids
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (t) => t.length * 10 });
  const redraws = redraw(up);
  const sessions = SESSIONS.map((c) => session(up, window, flush, c));
  return format({
    description:
      "Upstream's text editor at the pinned commit (tools/goldens/text-editing.mjs): redrawTextBoundingBox on scenes of every container type, and editing sessions through App.startTextEditing, handleTextWysiwyg and textWysiwyg (cut out of App.tsx and run on a stand-in App under jsdom), step by step: the textarea's value, selection and assigned style, the elements each step changed, the app state and what the editor asked of the app. Elements leave out seed, versionNonce and updated. Text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    constants: {
      TEXT_VIEWPORT_PADDING: up.TEXT_VIEWPORT_PADDING,
      TEXT_TO_CENTER_SNAP_THRESHOLD: up.TEXT_TO_CENTER_SNAP_THRESHOLD,
      CARET_FOLLOW_PADDING: up.CARET_FOLLOW_PADDING,
      DEFAULT_BOUND_TEXT_LABEL_POSITION: up.DEFAULT_BOUND_TEXT_LABEL_POSITION,
    },
    stateKeys: STATE_KEYS,
    redraw: redraws,
    sessions,
  });
};

/** Runs fn with Math.random disabled and the clock fixed. */
const deterministic = async (fn) => {
  const random = Math.random;
  const now = Date.now;
  Math.random = () => {
    throw new Error("Math.random called while generating text editing goldens");
  };
  Date.now = () => NOW;
  try {
    return await fn();
  } finally {
    Math.random = random;
    Date.now = now;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`text-editing: ${error.message}\n`);
    process.exit(1);
  }
  const out = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== out) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("text editing goldens are out of date: run node tools/goldens/text-editing.mjs\n");
      process.exit(1);
    }
    process.stdout.write("text editing goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, out);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
