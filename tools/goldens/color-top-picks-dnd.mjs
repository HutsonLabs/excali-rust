#!/usr/bin/env node
// Colour picker top-picks customisation goldens for excali-ui (ex-536):
// upstream's own ColorPicker with `customizableTopPicks`
// (packages/excalidraw/components/ColorPicker/*, colorTopPicksDnD.ts) and
// the shared drag and drop (components/TopPicksDnD/topPicksDnD.tsx, its
// TopPicksContextMenu.tsx and TopPicksTip.tsx) rendered by React 19.0.0
// into jsdom 22.1.0 with radix-ui 1.4.3, driven by a host that applies
// `updateData` to its app state and renders again, and TopPicksDnD.scss
// compiled with sass 1.51.0.
//
//   node tools/goldens/color-top-picks-dnd.mjs            write the fixture and CSS
//   node tools/goldens/color-top-picks-dnd.mjs --check    exit 1 if either is stale
//   node tools/goldens/color-top-picks-dnd.mjs --out DIR  write (or --check) in DIR
//
// jsdom has no layout and no real clock, so the drags run on a fixed
// layout (`layout`: each strip pick's and the strip's client rect, every
// other element's) and a fake clock: `performance.now`, `window.setTimeout`
// and `requestAnimationFrame` are the generator's, and time moves only when
// a step says so. PointerEvent is jsdom's MouseEvent with a pointerId.
//
// Writes crates/excali-ui/tests/fixtures/color-top-picks-dnd.json:
//
// - `locale`: the strings the strip, its menu and the tip read;
// - `constants`: the drag thresholds and class names of topPicksDnD.tsx;
// - `isSameColor`: colorTopPicksDnD.ts's value equality per pair;
// - `reorderOffset`: getTopPickReorderOffset per drag state and index;
// - `cases`: per case the picker's props and app state (the
//   `colorTopPicks` slot it customises) and the DOM React leaves in the
//   editor container, as color-picker.mjs records it;
// - `menus`: per case the container after a right click on the strip
//   (radix's ContextMenu), then after clicking its item, and the host's
//   calls;
// - `tips`: per case the host's calls after clicking the tip's reset link;
// - `drags`: per scenario the picker's props and app state, then per step
//   (pointer down on a strip pick or another swatch, pointer moves and up
//   at client points, time passing, an animation frame, Escape, a pointer
//   cancel, the click a browser sends after the up) the clock, the drag
//   session the down started (`{value, origin}`, null when none), the
//   host's calls it made (`onChange`, and `updateData` but for the
//   popover's `openPopup`, which is ex-523's), whether the click reached
//   its target, the strip's DOM, each
//   ghost in document.body (`{tag, attrs, style, children}`) and whether
//   the body has the drag class.
//
// and crates/excali-ui/src/top_picks_dnd/top_picks_dnd.css:
// TopPicksDnD.scss compiled (expanded).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { ENTRY as PICKER_ENTRY, installDom, makeTree, SHIMS, staticIcons, STUBS } from "./color-picker.mjs";
import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "color-top-picks-dnd.json");
export const STYLESHEET = join("src", "top_picks_dnd", "top_picks_dnd.css");

const SCSS = "components/TopPicksDnD/TopPicksDnD.scss";
const DND_MODULE = "packages/excalidraw/components/TopPicksDnD/topPicksDnD";
const COLOR_DND_MODULE = "packages/excalidraw/components/ColorPicker/colorTopPicksDnD";

const ENTRY = `${PICKER_ENTRY}
export { getTopPickReorderOffset } from "./${DND_MODULE}";
export { isSameColor } from "./${COLOR_DND_MODULE}";
`;

// Records each drag session begin() starts (after its early returns): the
// value and origin upstream's pickers pass.
const BEGIN_ANCHOR = `      window.addEventListener("pointermove", onPointerMove, true);
      window.addEventListener("pointerup", onPointerUp, true);`;

const patchBegin = (source) => {
  const at = source.indexOf(BEGIN_ANCHOR);
  if (source.split(BEGIN_ANCHOR).length !== 2) {
    throw new Error("topPicksDnD.tsx: begin()'s listeners not found once");
  }
  return `${source.slice(0, at)}      globalThis.__dndBegin?.(value, origin);\n${source.slice(at)}`;
};

const usage = () => {
  process.stderr.write("usage: color-top-picks-dnd.mjs [--check] [--out DIR]\n");
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

// -- layout and clock -----------------------------------------------------------

/** The client rects jsdom does not compute: the strip's five picks 24px wide
 * every 28px, the strip around them, and every other element. */
const LAYOUT = {
  strip: { left: 20, top: 20, width: 136, height: 24 },
  pick: { left: 20, top: 20, width: 24, height: 24, step: 28 },
  other: { left: 300, top: 200, width: 28, height: 28 },
};

// radix's ContextMenu.Trigger styles the strip `WebkitTouchCallout: "none"`
// (react-context-menu's ContextMenuTrigger); jsdom's CSSStyleDeclaration
// has no such property, so React's write would be an expando: route it
// through setProperty, which the recorder notes, as WebKit keeps it.
const installTouchCallout = (window) => {
  Object.defineProperty(window.CSSStyleDeclaration.prototype, "WebkitTouchCallout", {
    configurable: true,
    get() {
      return this.getPropertyValue("-webkit-touch-callout");
    },
    set(value) {
      this.setProperty("-webkit-touch-callout", value);
    },
  });
};

const installLayout = (window) => {
  const rect = ({ left, top, width, height }) => new window.DOMRect(left, top, width, height);
  window.Element.prototype.getBoundingClientRect = function () {
    const index = this.getAttribute("data-top-pick-index");
    if (index !== null) {
      const { step, ...pick } = LAYOUT.pick;
      return rect({ ...pick, left: pick.left + Number(index) * step });
    }
    if (this.classList.contains("top-picks-dnd")) return rect(LAYOUT.strip);
    return rect(LAYOUT.other);
  };
};

const START = 1000;

const installClock = (window, act) => {
  const clock = { now: START, seq: 0, timers: [], frames: [] };
  window.setTimeout = (fn, ms = 0, ...args) => {
    const id = ++clock.seq;
    clock.timers.push({ id, due: clock.now + Math.max(0, Number(ms) || 0), fn: () => fn(...args) });
    return id;
  };
  window.clearTimeout = (id) => {
    clock.timers = clock.timers.filter((t) => t.id !== id);
  };
  globalThis.requestAnimationFrame = (fn) => {
    clock.frames.push(fn);
    return clock.frames.length;
  };
  Object.defineProperty(globalThis.performance, "now", { value: () => clock.now, configurable: true, writable: true });
  clock.advance = async (ms) => {
    const target = clock.now + ms;
    for (;;) {
      const due = clock.timers.filter((t) => t.due <= target).sort((a, b) => a.due - b.due || a.id - b.id)[0];
      if (!due) break;
      clock.timers = clock.timers.filter((t) => t !== due);
      clock.now = due.due;
      await act(async () => due.fn());
    }
    clock.now = target;
  };
  clock.frame = async () => {
    const frames = clock.frames;
    clock.frames = [];
    await act(async () => frames.forEach((f) => f(clock.now)));
  };
  return clock;
};

const pointerEventClass = (window) =>
  class PointerEvent extends window.MouseEvent {
    constructor(type, init = {}) {
      super(type, init);
      Object.defineProperty(this, "pointerId", { value: init.pointerId ?? 1 });
      Object.defineProperty(this, "pointerType", { value: "mouse" });
    }
  };

// -- host -------------------------------------------------------------------------

const LABELS = { elementStroke: "labels.stroke", elementBackground: "labels.background" };

const PICKERS = {
  stroke: {
    type: "elementStroke",
    palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE",
    topPicks: "DEFAULT_ELEMENT_STROKE_PICKS",
    customizableTopPicks: "elementStroke",
  },
  background: {
    type: "elementBackground",
    palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE",
    topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS",
    customizableTopPicks: "elementBackground",
  },
  bucketFill: {
    type: "elementBackground",
    palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE",
    topPicks: "BUCKET_FILL_BACKGROUND_PICKS",
    customizableTopPicks: "bucketFill",
    excludedColors: ["transparent"],
  },
  stickyStroke: {
    type: "elementStroke",
    palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE",
    topPicks: "STICKY_NOTE_STROKE_PICKS",
    customizableTopPicks: "stickyNoteStroke",
  },
};

/** Mounts a ColorPicker whose host applies updateData and renders again. */
const mount = async (up, window, c) => {
  const { React, act, createRoot, ColorPicker, colors } = up;
  const { document } = window;
  document.body.innerHTML = "";
  document.body.className = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  globalThis.__ui = {
    app: { ownerWindow: window, ownerDocument: document },
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: "desktop", desktopUIMode: "full", userAgent: {}, isTouchScreen: false, isLandscape: false },
    stylesPanelMode: c.mode ?? "full",
  };
  const p = PICKERS[c.picker];
  const property = p.type === "elementStroke" ? "strokeColor" : "backgroundColor";
  const elements = (c.custom ?? []).map((color) => ({
    strokeColor: property === "strokeColor" ? color : "#1e1e1e",
    backgroundColor: property === "backgroundColor" ? color : "transparent",
    isDeleted: false,
  }));
  const host = {
    calls: [],
    color: c.color,
    appState: {
      openPopup: c.open ? p.type : null,
      theme: c.theme ?? "light",
      colorTopPicks: c.colorTopPicks ?? {},
      editingTextElement: null,
    },
  };
  const root = createRoot(container);
  // radix's focus scope closes the popup on a (real) timer after the case
  // unmounted it
  const render = () =>
    !host.unmounted &&
    root.render(
      React.createElement(ColorPicker, {
        type: p.type,
        color: host.color,
        onChange: (color) => host.calls.push({ onChange: color }),
        label: up.t(LABELS[p.type]),
        elements,
        appState: host.appState,
        palette: colors[p.palette],
        topPicks: colors[p.topPicks],
        updateData: (data) => {
          host.calls.push({ updateData: data });
          host.appState = { ...host.appState, ...data };
          render();
        },
        ...(p.excludedColors ? { excludedColors: p.excludedColors } : {}),
        ...(c.customizable === false ? {} : { customizableTopPicks: p.customizableTopPicks }),
      }),
    );
  await act(async () => render());
  return { container, host, root, elements };
};

const describe = (c, elements) => ({
  name: c.name,
  picker: c.picker,
  ...PICKERS[c.picker],
  ...(c.customizable === false ? { customizableTopPicks: null } : {}),
  excludedColors: PICKERS[c.picker].excludedColors ?? [],
  color: c.color,
  theme: c.theme ?? "light",
  open: !!c.open,
  mode: c.mode ?? "full",
  colorTopPicks: c.colorTopPicks ?? {},
  elements,
});

// -- cases ------------------------------------------------------------------------

const PINNED = ["#123456", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];

const CASES = [
  { name: "stroke", picker: "stroke", color: "#1e1e1e" },
  { name: "stroke-customized", picker: "stroke", color: "#123456", colorTopPicks: { elementStroke: PINNED } },
  { name: "stroke-empty-slot", picker: "stroke", color: "#1e1e1e", colorTopPicks: { elementStroke: [] } },
  { name: "stroke-other-slot", picker: "stroke", color: "#1e1e1e", colorTopPicks: { elementBackground: ["#123456"] } },
  { name: "stroke-customized-dark", picker: "stroke", color: "#123456", colorTopPicks: { elementStroke: PINNED }, theme: "dark" },
  { name: "open-stroke", picker: "stroke", color: "#1e1e1e", open: true },
  { name: "open-stroke-customized", picker: "stroke", color: "#123456", open: true, colorTopPicks: { elementStroke: PINNED }, custom: ["#123456", "#abcdef"] },
  { name: "open-background", picker: "background", color: "#a5d8ff", open: true },
  { name: "open-bucket-fill-customized", picker: "bucketFill", color: "#ffec99", open: true, colorTopPicks: { bucketFill: ["#ffec99", "#123456"] } },
  { name: "sticky-stroke", picker: "stickyStroke", color: "#1e1e1e", colorTopPicks: { stickyNoteStroke: ["#e03131", "#1e1e1e"] } },
  { name: "compact-stroke", picker: "stroke", color: "#1e1e1e", mode: "compact", colorTopPicks: { elementStroke: PINNED } },
  { name: "open-compact-stroke", picker: "stroke", color: "#1e1e1e", mode: "compact", open: true, colorTopPicks: { elementStroke: PINNED } },
  { name: "open-stroke-not-customizable", picker: "stroke", color: "#1e1e1e", open: true, customizable: false, colorTopPicks: { elementStroke: PINNED } },
];

const OUTLINE = (node) => node.getAttribute("class") === "top-picks-dnd__outline";

const renderCase = async (up, window, iconNames, c) => {
  const { container, host, root, elements } = await mount(up, window, c);
  const dom = [...container.childNodes].map(makeTree(iconNames, new Map(), OUTLINE));
  host.unmounted = true;
  await up.act(async () => root.unmount());
  return { ...describe(c, elements), dom };
};

const MENU_CASES = [
  { name: "stroke", picker: "stroke", color: "#1e1e1e" },
  { name: "stroke-customized", picker: "stroke", color: "#123456", colorTopPicks: { elementStroke: PINNED, elementBackground: ["#abcdef"] } },
  { name: "open-stroke-customized", picker: "stroke", color: "#123456", open: true, colorTopPicks: { elementStroke: PINNED } },
];

const menuCase = async (up, window, iconNames, c) => {
  const { act } = up;
  const { container, host, root, elements } = await mount(up, window, c);
  const strip = container.querySelector(".color-picker__top-picks");
  await act(async () =>
    strip.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 60, clientY: 32 })),
  );
  const tree = () => [...container.childNodes].map(makeTree(iconNames, new Map(), OUTLINE));
  const opened = tree();
  const item = container.querySelector(".top-picks-dnd__context-menu-item");
  await act(async () => item.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
  const after = tree();
  const calls = [...host.calls];
  host.unmounted = true;
  await act(async () => root.unmount());
  return { ...describe(c, elements), opened, calls, after };
};

const TIP_CASES = [
  { name: "open-stroke-customized", picker: "stroke", color: "#123456", open: true, colorTopPicks: { elementStroke: PINNED, elementBackground: ["#abcdef"] } },
];

const tipCase = async (up, window, iconNames, c) => {
  const { act } = up;
  const { container, host, root, elements } = await mount(up, window, c);
  const reset = container.querySelector(".top-picks-dnd__tip-reset");
  await act(async () => reset.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
  const after = [...container.childNodes].map(makeTree(iconNames, new Map(), OUTLINE));
  const calls = [...host.calls];
  host.unmounted = true;
  await act(async () => root.unmount());
  return { ...describe(c, elements), calls, after };
};

// -- drags ------------------------------------------------------------------------

// The strip's slot centres are x = 32, 60, 88, 116, 144 at y = 32; every
// other swatch sits at (300, 200) 28x28, centre (314, 214). Hit area:
// x 6..170, y 6..58.
const PALETTE_VIOLET = { source: '[data-testid="color-violet"]' };
const PALETTE_RED = { source: '[data-testid="color-red"]' };
const TRIGGER = { source: ".active-color" };
const CUSTOM_FIRST = { source: ".color-picker-content--default > button" };
const SHADE_FOURTH = { source: ".color-picker-content--default.shades > button:nth-child(4)" };

const DRAGS = [
  {
    name: "pin-palette-colour",
    picker: "stroke",
    color: "#1e1e1e",
    open: true,
    steps: [
      { down: PALETTE_VIOLET, x: 314, y: 214 },
      { wait: 150 },
      { move: [200, 120] },
      { frame: true },
      { move: [62, 33] },
      { up: [62, 33] },
      { click: true },
      { wait: 160 },
      { wait: 180 },
    ],
  },
  {
    name: "pin-active-colour",
    picker: "stroke",
    color: "#123456",
    open: false,
    steps: [{ down: TRIGGER, x: 310, y: 210 }, { wait: 120 }, { move: [150, 40] }, { up: [150, 40] }, { click: true }, { wait: 400 }],
  },
  {
    name: "pin-mixed-selection-trigger",
    picker: "stroke",
    color: null,
    open: false,
    steps: [{ down: TRIGGER, x: 310, y: 210 }, { wait: 120 }, { move: [150, 40] }, { up: [150, 40] }, { click: true }],
  },
  {
    name: "pin-shade",
    picker: "background",
    color: "#a5d8ff",
    open: true,
    steps: [{ down: SHADE_FOURTH, x: 314, y: 214 }, { wait: 101 }, { move: [33, 30] }, { up: [33, 30] }, { click: true }],
  },
  {
    name: "pin-custom-colour-onto-customized",
    picker: "stroke",
    color: "#abcdef",
    open: true,
    custom: ["#abcdef", "#123456"],
    colorTopPicks: { elementStroke: PINNED, elementBackground: ["#fedcba"] },
    steps: [{ down: CUSTOM_FIRST, x: 314, y: 214 }, { wait: 100 }, { move: [120, 35] }, { up: [120, 35] }, { click: true }],
  },
  {
    name: "pin-duplicate",
    picker: "stroke",
    color: "#1e1e1e",
    open: true,
    steps: [
      { down: PALETTE_RED, x: 314, y: 214 },
      { wait: 200 },
      { move: [250, 150] },
      { move: [140, 30] },
      { move: [60, 30] },
      { up: [60, 30] },
      { click: true },
      { wait: 160 },
      { wait: 180 },
    ],
  },
  {
    name: "pin-duplicate-other-notation",
    picker: "stroke",
    color: "#E03131",
    open: true,
    custom: ["#E03131"],
    steps: [{ down: CUSTOM_FIRST, x: 314, y: 214 }, { wait: 200 }, { move: [100, 32] }, { up: [100, 32] }, { click: true }],
  },
  {
    name: "pin-bucket-fill",
    picker: "bucketFill",
    color: "#ffec99",
    open: true,
    colorTopPicks: { elementStroke: PINNED },
    steps: [{ down: PALETTE_VIOLET, x: 314, y: 214 }, { wait: 200 }, { move: [32, 32] }, { up: [32, 32] }, { click: true }],
  },
  {
    name: "reorder-forward",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 0 }, x: 32, y: 32 },
      { wait: 120 },
      { move: [45, 32] },
      { frame: true },
      { move: [88, 32] },
      { move: [116, 34] },
      { up: [116, 34] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "reorder-backward-customized",
    picker: "stroke",
    color: "#123456",
    colorTopPicks: { elementStroke: PINNED },
    steps: [
      { down: { pick: 4 }, x: 144, y: 32 },
      { wait: 120 },
      { move: [100, 30] },
      { move: [74, 30] },
      { move: [73, 30] },
      { up: [73, 30] },
      { click: true },
    ],
  },
  {
    name: "reorder-back-to-its-slot",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 2 }, x: 88, y: 32 },
      { wait: 150 },
      { move: [88, 120] },
      { move: [90, 40] },
      { up: [90, 40] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "drop-outside",
    picker: "stroke",
    color: "#1e1e1e",
    open: true,
    steps: [
      { down: PALETTE_VIOLET, x: 314, y: 214 },
      { wait: 150 },
      { move: [60, 32] },
      { move: [60, 90] },
      { up: [60, 90] },
      { click: true },
      { wait: 160 },
      { wait: 180 },
    ],
  },
  {
    name: "hit-area-edges",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 1 }, x: 60, y: 32 },
      { wait: 150 },
      { move: [5, 32] },
      { move: [6, 32] },
      { move: [170, 32] },
      { move: [171, 32] },
      { move: [60, 5] },
      { move: [60, 6] },
      { move: [60, 58] },
      { move: [60, 59] },
      { move: [74, 32] },
      { move: [74.5, 32] },
      { up: [74.5, 32] },
    ],
  },
  {
    name: "escape-cancels",
    picker: "stroke",
    color: "#1e1e1e",
    open: true,
    steps: [
      { down: PALETTE_VIOLET, x: 314, y: 214 },
      { wait: 150 },
      { move: [60, 32] },
      { key: "Escape" },
      { up: [60, 32] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "escape-before-activation",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [{ down: { pick: 1 }, x: 60, y: 32 }, { key: "Escape" }, { up: [60, 32] }, { click: true }],
  },
  {
    name: "pointer-cancel",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [{ down: { pick: 3 }, x: 116, y: 32 }, { wait: 150 }, { move: [30, 32] }, { cancel: true }, { up: [30, 32] }, { wait: 400 }],
  },
  {
    name: "quick-click",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [{ down: { pick: 1 }, x: 60, y: 32 }, { wait: 30 }, { move: [75, 32] }, { wait: 20 }, { up: [75, 32] }, { click: true }, { wait: 200 }],
  },
  {
    name: "flick-and-hold",
    picker: "stroke",
    color: "#1e1e1e",
    open: true,
    steps: [{ down: PALETTE_VIOLET, x: 314, y: 214 }, { wait: 30 }, { move: [300, 200] }, { wait: 69 }, { wait: 1 }, { move: [116, 32] }, { up: [116, 32] }, { click: true }],
  },
  {
    name: "flick-then-back",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 0 }, x: 32, y: 32 },
      { wait: 30 },
      { move: [50, 32] },
      { wait: 20 },
      { move: [36, 32] },
      { wait: 200 },
      { up: [36, 32] },
      { click: true },
    ],
  },
  {
    name: "flick-back-after-the-last-move",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 0 }, x: 32, y: 32 },
      { wait: 30 },
      { move: [50, 32] },
      { wait: 20 },
      { move: [60, 32] },
      { wait: 100 },
      { move: [88, 32] },
      { up: [88, 32] },
      { click: true },
    ],
  },
  {
    name: "slow-small-move",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [{ down: { pick: 1 }, x: 60, y: 32 }, { wait: 500 }, { move: [66, 36] }, { up: [66, 36] }, { click: true }],
  },
  {
    name: "other-pointer-ignored",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 1 }, x: 60, y: 32 },
      { wait: 150 },
      { move: [116, 32], pointerId: 2 },
      { move: [116, 32] },
      { up: [116, 32], pointerId: 2 },
      { cancel: true, pointerId: 2 },
      { up: [116, 32] },
    ],
  },
  {
    name: "second-down-ignored",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [
      { down: { pick: 1 }, x: 60, y: 32 },
      { wait: 150 },
      { move: [116, 32] },
      { down: { pick: 0 }, x: 32, y: 32, pointerId: 2 },
      { up: [116, 32] },
    ],
  },
  {
    name: "right-button",
    picker: "stroke",
    color: "#1e1e1e",
    steps: [{ down: { pick: 1 }, x: 60, y: 32, button: 2 }, { wait: 150 }, { move: [116, 32] }, { up: [116, 32] }],
  },
  {
    name: "compact-mode",
    picker: "stroke",
    color: "#1e1e1e",
    mode: "compact",
    open: true,
    steps: [{ down: PALETTE_VIOLET, x: 314, y: 214 }, { wait: 150 }, { move: [60, 32] }, { up: [60, 32] }],
  },
  {
    name: "not-customizable",
    picker: "stroke",
    color: "#1e1e1e",
    customizable: false,
    steps: [{ down: { pick: 1 }, x: 60, y: 32 }, { wait: 150 }, { move: [116, 32] }, { up: [116, 32] }],
  },
];

const selectorOf = (on) => (on.pick !== undefined ? `[data-top-pick-index="${on.pick}"]` : on.source);

const runDrag = async (up, window, iconNames, c) => {
  const { act } = up;
  const clock = installClock(window, act);
  const PointerEvent = pointerEventClass(window);
  const { container, host, root, elements } = await mount(up, window, c);
  const { document } = window;
  let target = null;
  const tree = makeTree(iconNames, new Map(), OUTLINE);
  const out = [];
  for (const step of c.steps) {
    host.calls = [];
    let session = null;
    let clickReached;
    globalThis.__dndBegin = (value, origin) => {
      session = { value, origin: { ...origin } };
    };
    const pointer = (type, [x, y], el = document) =>
      el.dispatchEvent(
        new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, pointerId: step.pointerId ?? 1, button: step.button ?? 0 }),
      );
    if (step.down) {
      const el = container.querySelector(selectorOf(step.down));
      if (!el) throw new Error(`${c.name}: no ${selectorOf(step.down)}`);
      if ((step.pointerId ?? 1) === 1) target = el;
      await act(async () => pointer("pointerdown", [step.x, step.y], el));
    } else if (step.wait !== undefined) {
      await clock.advance(step.wait);
    } else if (step.move) {
      await act(async () => pointer("pointermove", step.move));
    } else if (step.up) {
      await act(async () => pointer("pointerup", step.up));
    } else if (step.frame) {
      await clock.frame();
    } else if (step.key) {
      await act(async () =>
        document.dispatchEvent(new window.KeyboardEvent("keydown", { key: step.key, bubbles: true, cancelable: true })),
      );
    } else if (step.cancel) {
      await act(async () => pointer("pointercancel", [0, 0]));
    } else if (step.click) {
      // the click a browser sends after the up, on the element the down hit
      // (or, when React replaced it, its successor); a drop suppresses it
      const el = target.isConnected ? target : container.querySelector(selectorOf(c.steps[0].down));
      let reached = false;
      const seen = () => (reached = true);
      el.addEventListener("click", seen);
      await act(async () => el.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true, clientX: 0, clientY: 0 })));
      el.removeEventListener("click", seen);
      clickReached = reached;
    } else {
      throw new Error(`${c.name}: unknown step ${JSON.stringify(step)}`);
    }
    globalThis.__dndBegin = undefined;
    const strip = container.querySelector(".color-picker__top-picks");
    out.push({
      step,
      now: clock.now - START,
      ...(step.down ? { session } : {}),
      ...(step.click ? { clickReached } : {}),
      // the popover's own openPopup updates (radix dismissing it under
      // jsdom's focus handling, and the trigger's toggle) are ex-523's
      calls: host.calls.filter((call) => !call.updateData || Object.keys(call.updateData).some((k) => k !== "openPopup")),
      strip: strip ? tree(strip) : null,
      ghosts: [...document.body.querySelectorAll(":scope > .excalidraw-top-picks-dnd-ghost")].map(tree),
      bodyActive: document.body.classList.contains("excalidraw-top-picks-dnd-active"),
    });
  }
  // run what the scenario left pending (a drop's click suppression is
  // removed on a timer) so it cannot reach the next one
  await clock.advance(1000);
  await clock.frame();
  host.unmounted = true;
  await act(async () => root.unmount());
  return { ...describe(c, elements), steps: out };
};

// -- helpers ----------------------------------------------------------------------

const SAME_COLOR = [
  ["#ffffff", "#ffffff"],
  ["#ffffff", "#FFFFFF"],
  ["#fff", "#ffffff"],
  ["white", "#ffffff"],
  ["WHITE", "#fff"],
  ["#e03131", "#E03131"],
  ["#e03131", "#e03132"],
  ["transparent", "TRANSPARENT"],
  ["transparent", "#00000000"],
  ["transparent", "#ffffff00"],
  ["rgb(255, 0, 0)", "#ff0000"],
  ["red", "#f00"],
  ["#ff000080", "#ff0000"],
  ["#ff000080", "#FF000080"],
  ["not a color", "NOT A COLOR"],
  ["not a color", "also not"],
  ["", ""],
  ["#12", "#12"],
  ["#12", "#120"],
];

const reorderOffsets = (up) => {
  const states = [
    null,
    { origin: { kind: "source" }, overIndex: 2, slotSpan: 28 },
    { origin: { kind: "pick", index: 1 }, overIndex: null, slotSpan: 28 },
    { origin: { kind: "pick", index: 1 }, overIndex: 1, slotSpan: 28 },
    { origin: { kind: "pick", index: 0 }, overIndex: 3, slotSpan: 28 },
    { origin: { kind: "pick", index: 4 }, overIndex: 1, slotSpan: 28 },
    { origin: { kind: "pick", index: 2 }, overIndex: 0, slotSpan: -28 },
    { origin: { kind: "pick", index: 1 }, overIndex: 4, slotSpan: 32.5 },
  ];
  return states.map((state) => ({
    state,
    offsets: [0, 1, 2, 3, 4].map((i) =>
      up.getTopPickReorderOffset(state && { value: "#000000", duplicateIndex: null, ...state }, i),
    ),
  }));
};

// -- stylesheet ---------------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString(`@use "${SCSS.replace(/\.scss$/, "")}";\n`, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/color-top-picks-dnd.mjs; do not edit. Upstream's\n" +
    ` * packages/excalidraw/${SCSS}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded). */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = ["colorPicker.topPicksTip", "colorPicker.resetTopPicks", "buttons.reset"];

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  installLayout(window);
  installTouchCallout(window);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    expose: {
      "packages/excalidraw/components/EyeDropper": ["eyeDropperCursor"],
      [COLOR_DND_MODULE]: ["isSameColor"],
    },
    patch: { [DND_MODULE]: patchBegin },
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const iconNames = new Map();
  for (const [name, value] of staticIcons(up.icons)) {
    const host = window.document.createElement("div");
    const root = up.createRoot(host);
    await up.act(async () => root.render(value));
    const markup = host.innerHTML;
    await up.act(async () => root.unmount());
    if (!iconNames.has(markup)) iconNames.set(markup, name);
  }
  const cases = [];
  for (const c of CASES) cases.push(await renderCase(up, window, iconNames, c));
  const menus = [];
  for (const c of MENU_CASES) menus.push(await menuCase(up, window, iconNames, c));
  const tips = [];
  for (const c of TIP_CASES) tips.push(await tipCase(up, window, iconNames, c));
  const drags = [];
  for (const c of DRAGS) drags.push(await runDrag(up, window, iconNames, c));
  window.close();
  const fixture = {
    upstream: upstream.commit,
    locale: Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)])),
    layout: LAYOUT,
    isSameColor: SAME_COLOR.map(([a, b]) => ({ a, b, same: up.isSameColor(a, b) })),
    reorderOffset: reorderOffsets(up),
    cases,
    menus,
    tips,
    drags,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`color-top-picks-dnd: ${error.message}\n`);
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
      process.stderr.write("colour top-picks goldens are out of date: run node tools/goldens/color-top-picks-dnd.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`colour top-picks goldens up to date: ${Object.keys(files).length} files\n`);
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
