#!/usr/bin/env node
// AppState goldens for excali-core (ex-106): upstream's own
// getDefaultAppState, APP_STATE_STORAGE_CONF cleaners and restoreAppState,
// and the colors.ts helpers restoreAppState relies on (colorToHex,
// isTransparent, both on tinycolor2 1.6.0), run from the pinned checkout
// under plain Node.
//
//   node tools/goldens/app-state.mjs            write the fixture
//   node tools/goldens/app-state.mjs --check    exit 1 if it is stale
//   node tools/goldens/app-state.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/app-state.json:
//
// - defaults: getDefaultAppState() (packages/excalidraw/appState.ts:24-147)
//   for several devicePixelRatio values (exportScale) and build modes
//   (currentItemRoundness is "sharp" in upstream's test mode);
// - storage: which keys clearAppStateForLocalStorage, cleanAppStateForExport
//   and clearAppStateForDatabase (appState.ts:293-339) keep, for every key
//   of APP_STATE_STORAGE_CONF and for unknown ones;
// - allowedActiveTools: AllowedExcalidrawActiveTools (data/restore.ts:220);
// - colors: colorToHex and isTransparent (packages/common/src/colors.ts)
//   for CSS colour strings in every notation tinycolor2 parses;
// - restore: restoreAppState(appState, localAppState) (data/restore.ts:
//   1254-1372) for the cases of upstream's tests (tests/data/restore.test.ts,
//   tests/colorTopPicks.test.ts, tests/fontTopPicks.test.ts) and for every
//   branch of its legacy handling and sanitising. Each result is written as
//   a diff against the dpr-1 production defaults: `changed` holds the keys
//   whose JSON differs, `keys` the key order when it differs, `removed` the
//   default keys missing. A case that throws records the error message.
//
// Every input is JSON: it is round-tripped through JSON.stringify/parse
// before upstream sees it, so the Rust side reads exactly what upstream read.
// Results are JSON.stringify's view (a NaN is null, an undefined property is
// absent).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FILE = "app-state.json";

const ENTRY = `
export { restoreAppState, AllowedExcalidrawActiveTools } from "./packages/excalidraw/data/restore";
export {
  getDefaultAppState,
  cleanAppStateForExport,
  clearAppStateForLocalStorage,
  clearAppStateForDatabase,
} from "./packages/excalidraw/appState";
export { colorToHex, isTransparent } from "./packages/common/src/colors";
`;

/** The environments getDefaultAppState is captured in. */
const ENVIRONMENTS = [
  { devicePixelRatio: 1, mode: "production" },
  { devicePixelRatio: 2, mode: "production" },
  { devicePixelRatio: 3, mode: "production" },
  { devicePixelRatio: 1.5, mode: "production" },
  { devicePixelRatio: 1, mode: "test" },
];

const usage = () => {
  process.stderr.write("usage: app-state.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: FIXTURES_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

/** What JSON.stringify writes, read back. */
const json = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

const load = (upstream, { devicePixelRatio, mode }) =>
  loadUpstream(upstream, {
    entry: ENTRY,
    define: {
      devicePixelRatio: JSON.stringify(devicePixelRatio),
      "import.meta.env.MODE": JSON.stringify(mode),
    },
  });

// -- storage --------------------------------------------------------------------

/** Every key of APP_STATE_STORAGE_CONF: the default keys plus the four
 * getDefaultAppState omits (appState.ts:24-27). */
const storageInput = (up) => ({
  ...json(up.getDefaultAppState()),
  offsetTop: 10,
  offsetLeft: 20,
  width: 800,
  height: 600,
});

const storageCases = (up) => {
  const all = storageInput(up);
  const inputs = [
    ["every-key", all],
    ["every-key-reversed", Object.fromEntries(Object.entries(all).reverse())],
    ["empty", {}],
    [
      "exported-keys-and-unknown",
      {
        futureKey: 1,
        lockedMultiSelections: { g1: true },
        viewBackgroundColor: "#000000",
        isSidebarDocked: true,
        gridModeEnabled: true,
        gridStep: 10,
        currentItemStrokeWidth: 4,
        gridSize: 40,
        theme: "dark",
        name: "n",
      },
    ],
    [
      "object-prototype-names",
      JSON.parse(
        '{"__proto__": 1, "constructor": 2, "toString": 3, "hasOwnProperty": 4, "valueOf": 5, "gridSize": 6}',
      ),
    ],
    ["null-and-odd-values", { gridSize: null, gridStep: "x", viewBackgroundColor: 7, zoom: 2, scrollX: null }],
  ];
  return inputs.map(([id, input]) => ({
    id,
    input,
    browser: Object.keys(up.clearAppStateForLocalStorage(input)),
    export: Object.keys(up.cleanAppStateForExport(input)),
    server: Object.keys(up.clearAppStateForDatabase(input)),
    exportValue: json(up.cleanAppStateForExport(input)),
  }));
};

// -- colors ---------------------------------------------------------------------

const COLORS = [
  "#FFF", "#fff", "#ffffff", "#FFFFFF", "white", "WHITE", " white ", "\twhite\n",
  "rgb(255,255,255)", "rgb(255, 255, 255)", "rgba(255,255,255,1)", "rgb 255 255 255",
  "rgb(255 255 255)", "rgb(100%, 100%, 100%)", "rgb(1.0, 0, 0)", "rgb(100.0%, 0, 0)",
  "hsl(0, 0%, 100%)", "hsv(0, 0%, 100%)", "hsla(120, 50%, 50%, 0.3)", "hsva(240, 100%, 100%, 0.5)",
  "hsl(400, 150%, 50%)", "hsl(0.5, 0.5, 0.5)", "hsv(360, 100%, 100%)", "hsv(0.07, 1, 1)",
  "hsl(-30, 50%, 50%)", "hsl(1.0, 1.0, 0.5)",
  "#ffff", "#ffffffff", "#12345678", "#1234", "#123", "#123456", "123456", "abc", "#ab", "ff",
  "#12345600", "#0000", "#00000080",
  "transparent", "TRANSPARENT", "  transparent  ", "rgba(0,0,0,0)", "rgba(0, 0, 0, 0)",
  "rgba(0,0,0,0%)", "rgba(0,0,0,-0)", "rgba(0,0,0,0.0)", "rgba 0 0 0 0", "RGBA(0,0,0,0)",
  "hsla(0,0%,0%,0)", "hsva(0, 0, 0, 0)", "rgba(0,0,0,0.5)", "rgba(0,0,0,1.5)", "rgba(0,0,0,50%)",
  "rgba(10, 20, 30, .25)", "rgba(10, 20, 30, +.25)", "rgba(10,20,30)", "rgb(10,20,30,0)",
  "rgb(1.5, 2.5, 3.5)", "rgb(10%, 20%, 30%)", "rgb(50.5%, 0, 0)", "rgb(300,-5,0)",
  "rgb(0.0000001%, 0, 0)", "rgb(0.5, 0.25, 0.75)", "rgb(.5, 1, 254.5)", "rgb(1., 2, 3)",
  "xrgb(1,2,3)", "rgb(1,2,3", "rgb(1|2|3)", "rgb((1,,2 3))", "rgb(1,2)", "rgb(a,b,c)",
  "red", "Red", "rebeccapurple", "aliceblue", "yellowgreen", "cornflowerblue", "not-a-color",
  "NOT-A-COLOR", "", " ", "#", "constructor", "toString", "__proto__", "hasOwnProperty",
  "#a5d8ff", "rgb(165, 216, 255)", "#eebefa", "#1e1e1e", "#ffdf6b", "#FCC2D7",
];

const colorCases = (up) =>
  COLORS.map((input) => ({ input, hex: up.colorToHex(input), transparent: up.isTransparent(input) }));

// -- restore --------------------------------------------------------------------

const diff = (result, base) => {
  const out = {};
  const keys = Object.keys(result);
  const baseKeys = Object.keys(base);
  if (JSON.stringify(keys) !== JSON.stringify(baseKeys)) out.keys = keys;
  const changed = {};
  for (const k of keys) {
    if (!(k in base) || JSON.stringify(result[k]) !== JSON.stringify(base[k])) changed[k] = result[k];
  }
  out.changed = changed;
  const removed = baseKeys.filter((k) => !(k in result));
  if (removed.length) out.removed = removed;
  return out;
};

/**
 * [id, appState, localAppState] triples. `defaults` is a fresh JSON copy of
 * getDefaultAppState() (what upstream's tests start from); an `undefined`
 * property is dropped by the JSON round trip, as absent as upstream's
 * `undefined`.
 */
const restoreInputs = (up) => {
  const defaults = () => json(up.getDefaultAppState());
  const with_ = (patch) => ({ ...defaults(), ...patch });
  const cases = [];
  const add = (id, appState, local = null) => cases.push([id, appState, local]);

  // tests/data/restore.test.ts, describe("restoreAppState")
  add("upstream-freedraw-variability-constant", { currentItemStrokeVariability: "constant" });
  add("upstream-freedraw-variability-variable", { currentItemStrokeVariability: "variable" });
  add("upstream-null-appstate-uses-local", null, with_({ cursorButton: "down", name: "local app state" }));
  add("upstream-null-local-uses-imported", with_({ cursorButton: "down", name: "imported app state" }), null);
  add(
    "upstream-imported-and-local",
    with_({ cursorButton: "down", name: "imported app state", activeTool: { ...defaults().activeTool, type: "selection" } }),
    with_({ cursorButton: "up", name: "local app state", activeTool: { ...defaults().activeTool, type: "rectangle" } }),
  );
  {
    const legacy = { ...defaults(), currentItemStrokeWidth: 4 };
    delete legacy.currentItemStrokeWidthKey;
    add("upstream-legacy-stroke-width", legacy);
  }
  add("upstream-transparent-sticky-stroke", { currentItemStickynoteStrokeColor: "transparent" });
  {
    const imported = defaults();
    delete imported.cursorButton;
    delete imported.name;
    add("upstream-undefined-imported-uses-local", imported, with_({ cursorButton: "down", name: "local app state" }));
  }
  {
    const imported = defaults();
    delete imported.cursorButton;
    const local = defaults();
    delete local.cursorButton;
    add("upstream-undefined-both-uses-default", imported, local);
  }
  add("upstream-both-null", null, null);
  add("upstream-active-tool-string", with_({ activeTool: "not allowed Excalidraw Element Types" }), defaults());
  add("upstream-zoom-number", with_({ zoom: 10 }), defaults());
  add("upstream-zoom-object", with_({ zoom: { value: 10 } }), defaults());
  add("upstream-zoom-null", with_({ zoom: null }), defaults());
  add("upstream-open-sidebar-empty", {});
  add("upstream-open-sidebar-library-string", { openSidebar: "library" });
  add("upstream-open-sidebar-other-string", { openSidebar: "xxx" });
  add("upstream-open-sidebar-library-object", { openSidebar: { name: "library" } });
  add("upstream-open-sidebar-default-tab", { openSidebar: { name: "default", tab: "ola" } });
  // tests/data/restore.test.ts, describe("restoreElements")
  add("upstream-sticky-defaults-and-top-picks", {
    currentItemStickynoteBackgroundColor: "transparent",
    currentItemStickynoteStrokeColor: "transparent",
    colorTopPicks: { stickyNoteBackground: ["#fcc2d7", "#b2f2bb"] },
  });
  // tests/colorTopPicks.test.ts
  add("upstream-color-top-picks-dedupe", {
    colorTopPicks: {
      elementStroke: null,
      elementBackground: [
        "#FFF", "#ffffff", "white", "#a5d8ff", "rgb(165, 216, 255)", "#eebefa", 42,
        "transparent", "rgba(0, 0, 0, 0)",
      ],
    },
  });
  add("upstream-color-top-picks-cap", {
    colorTopPicks: {
      elementStroke: Array.from({ length: 20 }, (_, i) => `#0000${String(i).padStart(2, "0")}`),
      elementBackground: "junk",
    },
  });
  // tests/fontTopPicks.test.ts
  add("upstream-font-top-picks-dedupe", { fontTopPicks: [7, "7", 7, 4, 10, 100, 1] });
  add("upstream-font-top-picks-cap", { fontTopPicks: [6, 5, 7, 8, 1] });
  add("upstream-font-top-picks-junk", { fontTopPicks: "junk" });
  add("upstream-font-top-picks-no-valid", { fontTopPicks: [4, "5"] });

  // Merge order: supplied, then local, then default; null is a value.
  add("merge-supplied-wins", { theme: "dark", name: "a" }, { theme: "light", name: "b" });
  add("merge-local-fills-gaps", { name: "a" }, { theme: "dark", scrollX: 5 });
  add("merge-null-is-kept", { name: null, theme: null }, { name: "local" });
  add("merge-odd-values-pass-through", { theme: 123, viewBackgroundColor: ["x"], gridModeEnabled: "yes", lockedMultiSelections: { g: true } });
  add("merge-unknown-keys-dropped", { futureKey: 1, width: 10, height: 20, offsetTop: 1, offsetLeft: 2 }, { otherFuture: 2, width: 5 });
  add("merge-empty-array-appstate", []);
  add("merge-exported-keys", { gridSize: 40, gridStep: 10, gridModeEnabled: true, viewBackgroundColor: "#000000", lockedMultiSelections: { a: true } });
  add("merge-editing-frame-reset", { editingFrame: "frame1" }, { editingFrame: "frame2" });
  add("merge-collaborators", { collaborators: { a: 1 } });

  // Legacy isSidebarDocked (restore.ts:1187-1204): migrated first, so its
  // target key moves to the front; the main loop then sets its value.
  add("legacy-sidebar-docked-true", { isSidebarDocked: true });
  add("legacy-sidebar-docked-false-with-new-key", { isSidebarDocked: false, defaultSidebarDockedPreference: true });
  add("legacy-sidebar-docked-null", { isSidebarDocked: null }, { defaultSidebarDockedPreference: true });
  add("legacy-sidebar-docked-local-only", {}, { isSidebarDocked: true });

  // Legacy currentItemStrokeWidth (restore.ts:1320-1325).
  for (const [id, width] of [["thin", 1], ["medium", 2], ["bold", 4], ["extra-bold", 8], ["string", "4"], ["null", null], ["float", 2.5]]) {
    add(`legacy-stroke-width-${id}`, { currentItemStrokeWidth: width, currentItemStrokeWidthKey: "thin" });
  }
  add("legacy-stroke-width-local-ignored", {}, { currentItemStrokeWidth: 4 });
  add("stroke-width-key-kept", { currentItemStrokeWidthKey: "bold" });

  // zoom (restore.ts:1346-1352): from the supplied state only.
  const zooms = [
    ["small", 0.05], ["large", 31], ["digits", 1.23456789], ["negative", -1], ["zero", 0],
    ["half-micro", 1.0000005], ["string", "2"], ["true", true], ["false", false], ["array", []],
    ["object", {}], ["value-string", { value: "5" }], ["value-true", { value: true }],
    ["value-false", { value: false }], ["value-array", { value: [] }], ["value-array-2", { value: [2] }],
    ["value-object", { value: {} }], ["value-null", { value: null }], ["value-missing", { other: 1 }],
    ["value-large", { value: 100 }], ["value-string-junk", { value: "abc" }],
    ["value-string-space", { value: " 3 " }], ["value-to-string-key", { value: { toString: 1 } }],
    ["value-value-of-key", { value: { valueOf: 1 } }], ["value-nested-array", { value: [[1, 2], null] }],
    ["tiny", 1e-7],
  ];
  for (const [id, zoom] of zooms) add(`zoom-${id}`, { zoom });
  add("zoom-local-ignored", {}, { zoom: { value: 3 } });

  // openSidebar (restore.ts:1353-1357).
  add("open-sidebar-empty-string", { openSidebar: "" });
  add("open-sidebar-null", { openSidebar: null }, { openSidebar: { name: "local" } });
  add("open-sidebar-number", { openSidebar: 5 });
  add("open-sidebar-local-string", {}, { openSidebar: "library" });
  add("open-sidebar-local-object", {}, { openSidebar: { name: "custom", tab: "t" } });

  // gridSize / gridStep (restore.ts:1358-1363): supplied finite numbers only.
  for (const [id, v] of [["zero", 0], ["fraction", 0.4], ["half", 2.5], ["large", 150], ["mid", 33.5], ["negative", -10], ["string", "30"], ["null", null], ["true", true], ["hundred", 100]]) {
    add(`grid-size-${id}`, { gridSize: v });
    add(`grid-step-${id}`, { gridStep: v });
  }
  add("grid-local-ignored", {}, { gridSize: 50, gridStep: 2 });

  // activeTool (restore.ts:1334-1344, utils.ts updateActiveTool).
  const tools = [
    ["rectangle", { type: "rectangle" }],
    ["eraser", { type: "eraser" }],
    ["laser", { type: "laser" }],
    ["autoshape", { type: "autoshape" }],
    ["magicframe", { type: "magicframe" }],
    ["bucketfill", { type: "bucketfill" }],
    ["stickynote", { type: "stickynote", locked: true }],
    ["frame", { type: "frame" }],
    ["embeddable", { type: "embeddable" }],
    ["lasso", { type: "lasso" }],
    ["hand", { type: "hand", locked: true, fromSelection: true, lastActiveTool: { type: "text" }, customType: "x" }],
    ["custom", { type: "custom", customType: "foo" }],
    ["custom-no-custom-type", { type: "custom" }],
    ["custom-locked", { type: "custom", customType: null, locked: true, fromSelection: true }],
    ["unknown", { type: "hexagon", locked: true }],
    ["to-string", { type: "toString" }],
    ["constructor", { type: "constructor" }],
    ["proto", { type: "__proto__" }],
    ["has-own-property", { type: "hasOwnProperty" }],
    ["value-of", { type: "valueOf" }],
    ["array-type", { type: ["selection"] }],
    ["array-type-custom", { type: [["custom"]], customType: "c" }],
    ["array-type-two", { type: ["a", "b"] }],
    ["number-type", { type: 1 }],
    ["zero-type", { type: 0, locked: 5 }],
    ["empty-type", { type: "" }],
    ["object-type", { type: {} }],
    ["object-type-to-string-key", { type: { toString: "x" } }],
    ["true-type", { type: true }],
    ["no-type", { locked: true }],
    ["locked-null", { type: "text", locked: null }],
    ["locked-string", { type: "text", locked: "yes" }],
    ["from-selection-null", { type: "text", fromSelection: null }],
    ["number", 5],
    ["string", "rectangle"],
    ["true", true],
    ["array", []],
    ["empty-object", {}],
    ["null", null],
  ];
  for (const [id, activeTool] of tools) add(`active-tool-${id}`, { activeTool });
  add("active-tool-local", {}, { activeTool: { type: "arrow", locked: true } });
  add("active-tool-local-null", {}, { activeTool: null });

  // cursorButton (restore.ts:1329): local, when truthy.
  for (const [id, v] of [["down", "down"], ["empty", ""], ["zero", 0], ["null", null], ["number", 5], ["object", {}]]) {
    add(`cursor-button-local-${id}`, {}, { cursorButton: v });
  }
  add("cursor-button-supplied-ignored", { cursorButton: "down" });

  // penDetected (restore.ts:1331-1333).
  add("pen-mode-and-detected", { penMode: true, penDetected: true });
  add("pen-mode-only", { penMode: true });
  add("pen-detected-without-mode", { penMode: false, penDetected: true });
  add("pen-mode-truthy-detected-null", { penMode: "yes", penDetected: null });
  add("pen-mode-number-detected-number", { penMode: 1, penDetected: 5 });
  add("pen-detected-local", { penMode: true, penDetected: false }, { penDetected: true });
  add("pen-detected-local-null", { penMode: true, penDetected: true }, { penDetected: null });
  add("pen-mode-local-only", {}, { penMode: true, penDetected: true });

  // boxSelectionMode (restore.ts:1294-1298).
  add("box-selection-supplied", { boxSelectionMode: "overlap" });
  add("box-selection-null-local", { boxSelectionMode: null }, { boxSelectionMode: "overlap" });
  add("box-selection-null-no-local", { boxSelectionMode: null });
  add("box-selection-local", {}, { boxSelectionMode: "overlap" });
  add("box-selection-both-null", { boxSelectionMode: null }, { boxSelectionMode: null });

  // colorTopPicks (restore.ts:1206-1230, 1300-1317).
  for (const [id, v] of [["number", 5], ["string", "x"], ["array", []], ["null", null], ["true", true], ["empty", {}]]) {
    add(`color-top-picks-${id}`, { colorTopPicks: v });
  }
  add("color-top-picks-notations", {
    colorTopPicks: {
      elementStroke: ["WHITE", " white ", "rgb(255,255,255)", "hsl(0,0%,100%)", "#ffff", "#ffffffff", "red", "#F00"],
      elementBackground: ["transparent", "#0000", "rgba(0,0,0,0)", "#00000000", "rgba(0,0,0,0.5)", "#00000080", "hsla(120, 50%, 50%, 0.3)"],
      bucketFill: ["not-a-color", "NOT-A-COLOR", "", "rgb 1 2 3", "rgb(1,2,3)", "xrgb(1,2,3)", "#010203"],
      stickyNoteStroke: ["rgb(1.0, 0, 0)", "rgb(100%, 0, 0)", "#ff0000", "rgb(0.0000001%, 0, 0)", "#000000", "rgb(50.5%, 0, 0)", "#800000"],
      stickyNoteBackground: [null, true, {}, [], 5, "#fff", "fff", "#fff"],
    },
  });
  add("color-top-picks-extra-key", { colorTopPicks: { elementStroke: ["#fff"], future: ["#000"] } });
  add("color-top-picks-local", {}, { colorTopPicks: { elementStroke: ["#abc", "#AABBCC"] } });
  add("color-top-picks-constructor", { colorTopPicks: { elementStroke: ["constructor", "toString", "Constructor"] } });

  // fontTopPicks (restore.ts:1232-1252).
  add("font-top-picks-private-and-fallbacks", { fontTopPicks: [9, 10, 100, 998, 999, 1000, 4, 0, -1, 1.5, 2] });
  add("font-top-picks-duplicates", { fontTopPicks: [1, 1.0, 3, 3, 2] });
  add("font-top-picks-non-numbers", { fontTopPicks: [null, true, {}, [], "1", 5] });
  add("font-top-picks-object", { fontTopPicks: { 0: 1 } });
  add("font-top-picks-empty", { fontTopPicks: [] });
  add("font-top-picks-local", {}, { fontTopPicks: [8, 8, 6] });

  // Sticky note colours (packages/element/src/stickyNote.ts:67-81).
  const stickies = [
    ["empty", ""], ["transparent", "transparent"], ["hex4-zero", "#0000"], ["rgba-zero", "rgba(1,2,3,0)"],
    ["rgba-zero-percent", "rgba(1,2,3,0%)"], ["half", "rgba(1,2,3,0.5)"], ["red", "red"], ["junk", "junk"],
    ["zero", 0], ["false", false], ["null", null], ["number", 5], ["true", true], ["array", [0]],
    ["object-a-zero", { a: 0 }], ["object-a-string-zero", { a: "0" }], ["object-a-array", { a: [0] }],
    ["object-a-array-half", { a: [0.5, 1] }], ["object-a-null", { a: null }], ["object-a-true", { a: true }],
    ["object-a-junk-prefix", { a: "0.0abc" }], ["object-a-space", { a: "  0" }], ["object-a-tiny", { a: 1e-7 }],
    ["object-a-empty-array", { a: [] }], ["object-a-object", { a: {} }], ["object-rgb-a-zero", { r: 1, g: 2, b: 3, a: 0 }],
    ["object-hsv-a-zero", { h: "1", s: 0.5, v: 1, a: 0 }], ["object-no-a", { r: 0, g: 0, b: 0 }],
    ["object-a-to-string-key", { a: { toString: 0 } }], ["object-r-to-string-key", { r: { toString: 0 }, a: 0 }],
    ["object-h-to-string-key", { r: "x", h: { toString: 0 }, a: 0 }],
    ["object-a-nested-array", { a: [[0, 1]] }], ["object-a-null-first", { a: [null, 0] }],
  ];
  for (const [id, v] of stickies) {
    add(`sticky-stroke-${id}`, { currentItemStickynoteStrokeColor: v });
    add(`sticky-background-${id}`, { currentItemStickynoteBackgroundColor: v });
  }
  add("sticky-local", {}, { currentItemStickynoteStrokeColor: "transparent", currentItemStickynoteBackgroundColor: "#abcdef" });

  return cases;
};

const restoreCases = (up, base) =>
  restoreInputs(up).map(([id, appStateIn, localIn]) => {
    const appState = json(appStateIn);
    const localAppState = json(localIn);
    const out = { id, appState, localAppState };
    try {
      out.result = diff(json(up.restoreAppState(appState, localAppState)), base);
    } catch (error) {
      out.error = `${error.constructor.name}: ${error.message}`;
    }
    return out;
  });

// -- main -----------------------------------------------------------------------

const build = async (upstream) => {
  const loaded = [];
  for (const env of ENVIRONMENTS) loaded.push({ env, up: await load(upstream, env) });
  const up = loaded[0].up;
  const base = json(up.getDefaultAppState());
  const ids = new Set();
  const restore = restoreCases(up, base);
  for (const c of restore) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  const fixture = {
    description:
      "Upstream AppState behaviour at the pinned commit (tools/goldens/app-state.mjs): getDefaultAppState per environment, the keys each storage cleaner keeps, AllowedExcalidrawActiveTools, colorToHex/isTransparent, and restoreAppState results as diffs against defaults[0].appState.",
    upstream: upstream.commit,
    defaults: loaded.map(({ env, up }) => ({ ...env, appState: json(up.getDefaultAppState()) })),
    storage: storageCases(up),
    allowedActiveTools: json(up.AllowedExcalidrawActiveTools),
    colors: colorCases(up),
    restore,
  };
  return format(fixture);
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating AppState goldens");
  };
  try {
    return await fn();
  } finally {
    Math.random = random;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`app-state: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\nAppState goldens are out of date: run node tools/goldens/app-state.mjs\n`);
      process.exit(1);
    }
    process.stdout.write(`AppState goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
