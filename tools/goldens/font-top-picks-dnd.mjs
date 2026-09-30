#!/usr/bin/env node
// Font picker top-picks customisation goldens for excali-ui (ex-538):
// upstream's own FontPicker (packages/excalidraw/components/FontPicker/*,
// fontTopPicksDnD.ts) with the shared drag and drop
// (components/TopPicksDnD/topPicksDnD.tsx, TopPicksContextMenu.tsx and
// TopPicksTip.tsx) rendered by React 19.0.0 into jsdom 22.1.0 with radix-ui
// 1.4.3, driven by the host of font-picker.mjs (actionChangeFontFamily's
// PanelComponent and perform, actions/actionProperties.tsx:1160-1540).
//
//   node tools/goldens/font-top-picks-dnd.mjs            write the fixture
//   node tools/goldens/font-top-picks-dnd.mjs --check    exit 1 if it is stale
//   node tools/goldens/font-top-picks-dnd.mjs --out DIR  write (or --check) in DIR
//
// jsdom has no layout and no real clock, so the drags run on a fixed layout
// (`layout`: each strip pick's and the strip's client rect, every other
// element's) and the fake clock of color-top-picks-dnd.mjs
// (lib/top-picks-dnd.mjs), as FontPicker.test.tsx's "top picks drag & drop"
// cases lay the strip out.
//
// Writes crates/excali-ui/tests/fixtures/font-top-picks-dnd.json:
//
// - `locale`: the strings the strip's menu and the tip read;
// - `menus`: per case the host's state and the DOM React leaves in the
//   editor container after a right click on the strip (radix's
//   ContextMenu), then after clicking its item, and the host's calls;
// - `tips`: per case the host's calls and the DOM after clicking the tip's
//   reset link;
// - `drags`: per scenario the host's state, then per step (pointer down on
//   a strip pick or a list row, pointer moves and up at client points, time
//   passing, an animation frame, a mousemove over a list row, Escape, a
//   pointer cancel, the click a browser sends after the up) the clock, the
//   drag session the down started (`{value, origin}`, null when none), the
//   picker's callbacks it made (as font-picker.mjs records them, a callback
//   repeated back to back once), whether
//   the click reached its target, the host's state, the strip's DOM, each
//   ghost in document.body and whether the body has the drag class. A
//   ghost's tree reads its inline style as jsdom holds it (createFontGhost
//   copies computed values and clones), and its cloned icon is
//   `{icon, style}`.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  ENTRY as PICKER_ENTRY,
  F,
  installDom,
  makeTree,
  quietSetStateInRender,
  SHIMS,
  staticIcons,
  STUBS,
} from "./font-picker.mjs";
import { format } from "./lib/format.mjs";
import { installClock, installTouchCallout, patchBegin, pointerEventClass, START } from "./lib/top-picks-dnd.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "font-top-picks-dnd.json");

const DND_MODULE = "packages/excalidraw/components/TopPicksDnD/topPicksDnD";

const usage = () => {
  process.stderr.write("usage: font-top-picks-dnd.mjs [--check] [--out DIR]\n");
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

// -- layout -----------------------------------------------------------------------

/** The client rects jsdom does not compute: the strip's three picks 32px
 * wide every 40px (FontPicker.test.tsx's SLOT_SIZE and SLOT_SPAN), the
 * strip around them, and every other element. */
const LAYOUT = {
  strip: { left: 20, top: 20, width: 112, height: 32 },
  pick: { left: 20, top: 20, width: 32, height: 32, step: 40 },
  other: { left: 300, top: 200, width: 32, height: 32 },
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

// -- host -------------------------------------------------------------------------

/** Mounts the FontPicker under font-picker.mjs's host; `calls` collects the
 * picker's callbacks. */
const mount = async (up, window, c) => {
  const { React, act, createRoot, FontPicker } = up;
  const { document } = window;
  document.body.innerHTML = "";
  document.body.className = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const mode = c.mode ?? "full";
  const out = { container, calls: [], unmounted: false };
  const record = (call) => out.calls.push(call);
  const initial = {
    openPopup: c.open ? "fontFamily" : null,
    selectedFontFamily: c.selected,
    currentHoveredFontFamily: null,
    fontTopPicks: c.topPicks ?? null,
  };
  let setHost = null;
  out.state = initial;
  globalThis.__ui = {
    app: {
      ownerWindow: window,
      ownerDocument: document,
      fonts: { getSceneFamilies: () => c.scene ?? [] },
      state: { editingTextElement: null },
    },
    appProps: { showDeprecatedFonts: false },
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: "desktop", desktopUIMode: "full", userAgent: {}, isTouchScreen: false, isLandscape: false },
    stylesPanelMode: mode,
    appState: { theme: "light" },
    setAppState: (update) => {
      const next = typeof update === "function" ? update({ openPopup: out.state.openPopup }) : update;
      record(["setAppState", next]);
      setHost((s) => ({ ...s, ...next }));
    },
  };
  const Host = () => {
    const [s, set] = React.useState(initial);
    setHost = set;
    out.state = s;
    return React.createElement(FontPicker, {
      isOpened: s.openPopup === "fontFamily",
      selectedFontFamily: s.selectedFontFamily,
      hoveredFontFamily: s.currentHoveredFontFamily,
      topPicks: s.fontTopPicks,
      compactMode: mode !== "full",
      onTopPicksChange: (fontTopPicks) => {
        record(["topPicksChange", fontTopPicks]);
        set((p) => ({ ...p, fontTopPicks }));
      },
      onSelect: (fontFamily) => {
        record(["select", fontFamily]);
        set((p) => ({ ...p, openPopup: null, currentHoveredFontFamily: null, selectedFontFamily: fontFamily }));
      },
      onHover: (fontFamily) => {
        record(["hover", fontFamily]);
        set((p) => ({ ...p, currentHoveredFontFamily: fontFamily }));
      },
      onLeave: () => {
        record(["leave"]);
        set((p) => ({ ...p, currentHoveredFontFamily: null }));
      },
      onPopupChange: (open) => {
        record(["popupChange", open]);
        set((p) => (open ? { ...p, openPopup: "fontFamily" } : { ...p, currentHoveredFontFamily: null }));
      },
    });
  };
  const root = createRoot(container);
  await act(async () => root.render(React.createElement(Host)));
  out.root = root;
  // radix's dismissal and the list's unmount both close the popup; whether
  // both land in one step depends on timers, so a callback repeated back to
  // back is recorded once (the host's state is the same either way)
  out.take = () => {
    const calls = out.calls.filter((call, i) => i === 0 || JSON.stringify(call) !== JSON.stringify(out.calls[i - 1]));
    out.calls = [];
    return calls;
  };
  out.unmount = async () => act(async () => root.unmount());
  return out;
};

const describe = (c) => ({
  name: c.name,
  initial: {
    openPopup: c.open ? "fontFamily" : null,
    selectedFontFamily: c.selected,
    currentHoveredFontFamily: null,
    fontTopPicks: c.topPicks ?? null,
  },
  mode: c.mode ?? "full",
  scene: c.scene ?? [],
});

// -- menus and tips ---------------------------------------------------------------

const MENU_CASES = [
  { name: "defaults", selected: F.Excalifont },
  { name: "customized", selected: F.LilitaOne, topPicks: [F.LilitaOne] },
  { name: "open-customized", selected: F.Excalifont, open: true, scene: [F.Excalifont], topPicks: [F.LilitaOne, F.Nunito] },
];

const menuCase = async (up, window, iconNames, c) => {
  const { act } = up;
  const host = await mount(up, window, c);
  const { container } = host;
  host.take();
  const strip = container.querySelector(".FontPicker__top-picks");
  await act(async () =>
    strip.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 60, clientY: 36 })),
  );
  const tree = () => [...container.childNodes].map(makeTree(iconNames, new Map()));
  const opened = tree();
  const openedCalls = host.take();
  const item = container.querySelector(".top-picks-dnd__context-menu-item");
  await act(async () => item.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
  const after = tree();
  const calls = host.take();
  const state = { ...host.state };
  await host.unmount();
  return { ...describe(c), openedCalls, opened, calls, state, after };
};

const TIP_CASES = [
  { name: "open-customized", selected: F.Excalifont, open: true, scene: [F.Excalifont], topPicks: [F.LilitaOne, F.Nunito] },
];

const tipCase = async (up, window, iconNames, c) => {
  const { act } = up;
  const host = await mount(up, window, c);
  const { container } = host;
  host.take();
  const reset = container.querySelector(".FontPicker__tip .top-picks-dnd__tip-reset");
  await act(async () => reset.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })));
  const after = [...container.childNodes].map(makeTree(iconNames, new Map()));
  const calls = host.take();
  const state = { ...host.state };
  await host.unmount();
  return { ...describe(c), calls, state, after };
};

// -- drags ------------------------------------------------------------------------

// The strip's slot centres are x = 36, 76, 116 at y = 36; every other
// element sits at (300, 200) 32x32, centre (316, 216).
const row = (value) => ({ row: value });

const DRAGS = [
  {
    name: "pin-list-font",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [
      { down: row(F.LilitaOne), x: 316, y: 216 },
      { wait: 150 },
      { move: [200, 120] },
      { frame: true },
      { hover: F.Nunito },
      { move: [78, 37] },
      { up: [78, 37] },
      { click: true },
      { wait: 160 },
      { wait: 180 },
    ],
  },
  {
    name: "pin-duplicate",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [
      { down: row(F.Nunito), x: 316, y: 216 },
      { wait: 200 },
      { move: [250, 150] },
      { move: [36, 36] },
      { up: [36, 36] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "pads-short-list",
    selected: F.LilitaOne,
    open: true,
    scene: [F.LilitaOne],
    topPicks: [F.LilitaOne],
    steps: [
      { down: row(F.Nunito), x: 316, y: 216 },
      { wait: 150 },
      { move: [36, 36] },
      { up: [36, 36] },
      { click: true },
      { wait: 400 },
      { down: row(F.ComicShanns), x: 316, y: 216 },
      { wait: 150 },
      { move: [76, 36] },
      { up: [76, 36] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "reorder-forward",
    selected: F.Excalifont,
    steps: [
      { down: { pick: 0 }, x: 36, y: 36 },
      { wait: 120 },
      { move: [50, 36] },
      { frame: true },
      { move: [116, 36] },
      { up: [116, 36] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "reorder-backward-customized",
    selected: F.Nunito,
    topPicks: [F.LilitaOne, F.Excalifont, F.Nunito],
    steps: [
      { down: { pick: 2 }, x: 116, y: 36 },
      { wait: 120 },
      { move: [80, 36] },
      { move: [37, 38] },
      { up: [37, 38] },
      { click: true },
    ],
  },
  {
    name: "reorder-glyph-sample",
    selected: F.Nunito,
    topPicks: [F.Cascadia, F.ComicShanns],
    steps: [
      { down: { pick: 0 }, x: 36, y: 36 },
      { wait: 150 },
      { move: [116, 36] },
      { frame: true },
      { up: [116, 36] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "drop-outside",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [
      { down: row(F.LilitaOne), x: 316, y: 216 },
      { wait: 150 },
      { move: [76, 36] },
      { move: [76, 90] },
      { up: [76, 90] },
      { click: true },
      { wait: 160 },
      { wait: 180 },
    ],
  },
  {
    name: "escape-cancels",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [
      { down: row(F.LilitaOne), x: 316, y: 216 },
      { wait: 150 },
      { move: [76, 36] },
      { key: "Escape" },
      { up: [76, 36] },
      { click: true },
      { wait: 400 },
    ],
  },
  {
    name: "pointer-cancel",
    selected: F.Excalifont,
    steps: [{ down: { pick: 2 }, x: 116, y: 36 }, { wait: 150 }, { move: [30, 36] }, { cancel: true }, { up: [30, 36] }, { wait: 400 }],
  },
  {
    name: "hit-area-edges",
    selected: F.Excalifont,
    steps: [
      { down: { pick: 1 }, x: 76, y: 36 },
      { wait: 150 },
      { move: [5, 36] },
      { move: [6, 36] },
      { move: [146, 36] },
      { move: [147, 36] },
      { move: [76, 5] },
      { move: [76, 6] },
      { move: [76, 66] },
      { move: [76, 67] },
      { move: [96, 36] },
      { move: [96.5, 36] },
      { up: [96.5, 36] },
    ],
  },
  {
    name: "quick-click-pick",
    selected: F.Excalifont,
    steps: [{ down: { pick: 1 }, x: 76, y: 36 }, { wait: 30 }, { move: [90, 36] }, { wait: 20 }, { up: [90, 36] }, { click: true }, { wait: 200 }],
  },
  {
    name: "quick-click-row",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [{ down: row(F.LilitaOne), x: 316, y: 216 }, { wait: 30 }, { up: [316, 216] }, { click: true }, { wait: 200 }],
  },
  {
    name: "right-button",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [{ down: row(F.LilitaOne), x: 316, y: 216, button: 2 }, { wait: 150 }, { move: [76, 36] }, { up: [76, 36] }],
  },
  {
    name: "compact-mode",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    mode: "compact",
    steps: [{ down: row(F.LilitaOne), x: 316, y: 216 }, { wait: 150 }, { move: [76, 36] }, { up: [76, 36] }],
  },
];

const OUTLINE = (node) => node.getAttribute("class") === "top-picks-dnd__outline";

const selectorOf = (on) =>
  on.pick !== undefined ? `.FontPicker__top-picks [data-top-pick-index="${on.pick}"]` : `.dropdown-menu.fonts button[value="${on.row}"]`;

const sortedObject = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

/** The inline style jsdom holds (createFontGhost writes computed values
 * and clones elements, past the recorder). */
const heldStyle = (node) => {
  const style = node.style;
  const out = [];
  for (let i = 0; i < (style?.length ?? 0); i++) out.push([style[i], style.getPropertyValue(style[i])]);
  return sortedObject(out);
};

/** A ghost as document.body holds it; the cloned icon is `{icon, style}`. */
const ghostTree = (iconNames) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const bare = node.cloneNode(true);
      bare.removeAttribute("style");
      const name = iconNames.get(bare.outerHTML);
      if (!name) throw new Error(`a ghost svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      return { icon: name, style: heldStyle(node) };
    }
    return {
      tag: node.localName,
      attrs: sortedObject([...node.attributes].filter((a) => a.name !== "style").map((a) => [a.name, a.value])),
      style: heldStyle(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

const runDrag = async (up, window, iconNames, c) => {
  const { act } = up;
  const clock = installClock(window, act);
  const PointerEvent = pointerEventClass(window);
  const host = await mount(up, window, c);
  const { container } = host;
  host.take();
  const { document } = window;
  let target = null;
  const tree = makeTree(iconNames, new Map(), OUTLINE);
  const ghost = ghostTree(iconNames);
  const out = [];
  for (const step of c.steps) {
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
      target = step.down;
      await act(async () => pointer("pointerdown", [step.x, step.y], el));
    } else if (step.wait !== undefined) {
      await clock.advance(step.wait);
    } else if (step.move) {
      await act(async () => pointer("pointermove", step.move));
    } else if (step.up) {
      await act(async () => pointer("pointerup", step.up));
    } else if (step.frame) {
      await clock.frame();
    } else if (step.hover !== undefined) {
      const el = container.querySelector(`.dropdown-menu.fonts button[value="${step.hover}"]`);
      await act(async () => el.dispatchEvent(new window.MouseEvent("mousemove", { bubbles: true })));
    } else if (step.key) {
      await act(async () =>
        document.dispatchEvent(new window.KeyboardEvent("keydown", { key: step.key, bubbles: true, cancelable: true })),
      );
    } else if (step.cancel) {
      await act(async () => pointer("pointercancel", [0, 0]));
    } else if (step.click) {
      // the click a browser sends after the up, on the element the down
      // hit (React keeps it, or renders its successor); a drop suppresses it
      const el = container.querySelector(selectorOf(target));
      let reached = false;
      const seen = () => (reached = true);
      el.addEventListener("click", seen);
      await act(async () => el.dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true, clientX: 0, clientY: 0 })));
      el.removeEventListener("click", seen);
      clickReached = reached;
    } else {
      throw new Error(`${c.name}: unknown step ${JSON.stringify(step)}`);
    }
    // the host's batched update, then what the picker does with it; radix
    // dismisses the popover on Node's own timers (the fake clock is the
    // window's), so let those run out within the step that set them, or
    // under load they land in the next one
    await act(async () => {});
    await act(async () => new Promise((resolve) => globalThis.setTimeout(resolve, 20)));
    await act(async () => {});
    globalThis.__dndBegin = undefined;
    const strip = container.querySelector(".FontPicker__top-picks");
    out.push({
      step,
      now: clock.now - START,
      ...(step.down ? { session } : {}),
      ...(step.click ? { clickReached } : {}),
      calls: host.take(),
      state: { ...host.state },
      strip: strip ? tree(strip) : null,
      ghosts: [...document.body.querySelectorAll(":scope > .excalidraw-top-picks-dnd-ghost")].map(ghost),
      bodyActive: document.body.classList.contains("excalidraw-top-picks-dnd-active"),
    });
  }
  // run what the scenario left pending so it cannot reach the next one
  await clock.advance(1000);
  await clock.frame();
  await host.unmount();
  return { ...describe(c), steps: out };
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = ["fontList.topPicksTip", "fontList.resetTopPicks", "buttons.reset"];

export const build = async (upstream) => {
  const restoreConsole = quietSetStateInRender();
  const window = installDom();
  installLayout(window);
  installTouchCallout(window);
  const up = await loadUpstream(upstream, {
    entry: PICKER_ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    fontUris: true,
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
  const menus = [];
  for (const c of MENU_CASES) menus.push(await menuCase(up, window, iconNames, c));
  const tips = [];
  for (const c of TIP_CASES) tips.push(await tipCase(up, window, iconNames, c));
  const drags = [];
  for (const c of DRAGS) drags.push(await runDrag(up, window, iconNames, c));
  window.close();
  restoreConsole();
  const fixture = {
    upstream: upstream.commit,
    locale: Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)])),
    layout: LAYOUT,
    menus,
    tips,
    drags,
  };
  return { [FIXTURE]: format(fixture) };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`font-top-picks-dnd: ${error.message}\n`);
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
      process.stderr.write("font top-picks goldens are out of date: run node tools/goldens/font-top-picks-dnd.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`font top-picks goldens up to date: ${Object.keys(files).length} files\n`);
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
