#!/usr/bin/env node
// Snapping fixtures for excali-editor (ex-509): upstream's own
// packages/excalidraw/snapping.ts and renderer/renderSnaps.ts, run from the
// pinned checkout under plain Node on hand-written and seeded random scenes.
//
//   node tools/goldens/snapping-fixtures.mjs            write the fixture
//   node tools/goldens/snapping-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/snapping-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/snapping.json:
//
//   { "description", "upstream", "snapDistance", "nonLinearSnappable",
//     "enabled", "cases", "renders" }
//
// - snapDistance: [{ zoom, result }], getSnapDistance(zoom).
// - nonLinearSnappable: [{ tool, result }], isActiveToolNonLinearSnappable.
// - enabled: [{ state, event, selected, result }], isSnappingEnabled with
//   app.state = state (objectsSnapModeEnabled, gridModeEnabled, activeTool
//   { type }, selectedElementsAreBeingDragged), app.props = {}, event null
//   or { ctrlOrCmd } (KEYS.CTRL_OR_CMD set to it), selectedElements the
//   elements listed.
// - cases: { id, elements, state, corners, ... }. `elements` is the scene
//   (non-deleted; the map is arrayToMap(elements)); `state` the app state
//   the functions read: zoom, scrollX, scrollY, width, height, offsetLeft,
//   offsetTop, objectsSnapModeEnabled, gridModeEnabled, activeTool,
//   selectedElementsAreBeingDragged. Then:
//   - corners: [{ ids, opts, result }], getElementsCorners(the elements of
//     ids, map, opts) (opts null: the default argument).
//   - selected: ids; referencePoints and visibleGaps:
//     getReferenceSnapPoints and getVisibleGaps(elements, selected, state,
//     map).
//   - drags: [{ event, cache, dragOffset, result: { dragOffset, snapOffset,
//     snapLines } }]: maybeCacheVisibleGaps and
//     maybeCacheReferenceSnapPoints(event, selected) as App.tsx calls them
//     before the drag (cache "app"; "none" leaves the cache empty), then
//     snapDraggedElements(elements, dragOffset, app, event, map) with
//     selectedElementIds = selected; result.dragOffset is the offset after
//     the call (snapDraggedElements rounds it in place).
//   - resizes: [{ event, latest, handle, dragOffset, result }]:
//     maybeCacheReferenceSnapPoints(event, originals) then
//     snapResizingElements(latest, originals, app, event, dragOffset,
//     handle) where originals are the selected elements and latest the
//     elements given (handle false: no handle).
//   - news: [{ event, element, origin, dragOffset, result }]:
//     maybeCacheReferenceSnapPoints(event, [element]) then
//     snapNewElement(element, app, event, origin, dragOffset, map).
//   - pointers: [{ event, pointer, result }]: getSnapLinesAtPointer(
//     elements, app, pointer, event, map).
//   The cache is destroyed (SnapCache.destroy) before every call.
// - renders: [{ state, snapLines, calls }]: renderSnaps(context, state) on a
//   context recording every call and assignment (lineWidth, strokeStyle as
//   calls of that name). state: theme, zenModeEnabled, zoom, scrollX,
//   scrollY.
//
// Text is measured with upstream's test metric (text.length * 10).
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case, and random
// scenes come from a Park-Miller generator seeded per case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "snapping.json";

const ENTRY = `
export {
  SnapCache,
  getSnapDistance,
  isSnappingEnabled,
  getElementsCorners,
  getVisibleGaps,
  getReferenceSnapPoints,
  snapDraggedElements,
  snapResizingElements,
  snapNewElement,
  getSnapLinesAtPointer,
  isActiveToolNonLinearSnappable,
} from "./packages/excalidraw/snapping";
export { renderSnaps } from "./packages/excalidraw/renderer/renderSnaps";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_VERTICAL_ALIGN,
  KEYS,
  ROUNDNESS,
  TOOL_TYPE,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
} from "./packages/common/src/index";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const usage = () => {
  process.stderr.write("usage: snapping-fixtures.mjs [--check] [--out DIR]\n");
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

const clone = (value) => (value === undefined ? null : JSON.parse(JSON.stringify(value)));

/** roughjs' Random (Park-Miller, multiplier 48271), seeded per case. */
const rng = (seed) => {
  let state = seed;
  const next = () => {
    state = Math.imul(48271, state);
    return ((2 ** 31 - 1) & state) / 2 ** 31;
  };
  return {
    next,
    int: (lo, hi) => lo + Math.floor(next() * (hi - lo + 1)),
    pick: (items) => items[Math.floor(next() * items.length)],
    chance: (p) => next() < p,
  };
};

/** Rounds to 1/64 so random geometry stays exactly representable. */
const q = (v) => Math.round(v * 64) / 64;

// -- the app -------------------------------------------------------------------------

const STATE = {
  zoom: 1,
  scrollX: 0,
  scrollY: 0,
  width: 1200,
  height: 900,
  offsetLeft: 0,
  offsetTop: 0,
  objectsSnapModeEnabled: true,
  gridModeEnabled: false,
  activeTool: "selection",
  selectedElementsAreBeingDragged: false,
};

const appOf = (state, selected = []) => ({
  props: {},
  state: {
    zoom: { value: state.zoom },
    scrollX: state.scrollX,
    scrollY: state.scrollY,
    width: state.width,
    height: state.height,
    offsetLeft: state.offsetLeft,
    offsetTop: state.offsetTop,
    objectsSnapModeEnabled: state.objectsSnapModeEnabled,
    gridModeEnabled: state.gridModeEnabled,
    activeTool: { type: state.activeTool },
    selectedElementsAreBeingDragged: state.selectedElementsAreBeingDragged,
    selectedElementIds: Object.fromEntries(selected.map((id) => [id, true])),
  },
});

const eventOf = (up, event) => (event === null ? null : { [up.KEYS.CTRL_OR_CMD]: event.ctrlOrCmd });

// App.maybeCacheVisibleGaps and maybeCacheReferenceSnapPoints
// (App.tsx:10635-10680) with recomputeAnyways false and an empty cache.
const maybeCacheVisibleGaps = (up, app, event, selectedElements, elements, map) => {
  if (up.isSnappingEnabled({ event, app, selectedElements }) && !up.SnapCache.getVisibleGaps()) {
    up.SnapCache.setVisibleGaps(up.getVisibleGaps(elements, selectedElements, app.state, map));
  }
};
const maybeCacheReferenceSnapPoints = (up, app, event, selectedElements, elements, map) => {
  if (up.isSnappingEnabled({ event, app, selectedElements }) && !up.SnapCache.getReferenceSnapPoints()) {
    up.SnapCache.setReferenceSnapPoints(up.getReferenceSnapPoints(elements, selectedElements, app.state, map));
  }
};

// -- elements ------------------------------------------------------------------------

const el = (up, opts) => apiCreateElement(up, { roughness: 0, roundness: null, ...opts });

/** A container and its bound text, centred. */
const boxWithText = (up, id, x, y, w, h, extra = {}) => {
  const box = el(up, { id, x, y, width: w, height: h, boundElements: [{ type: "text", id: `${id}-text` }], ...extra });
  const text = el(up, {
    type: "text",
    id: `${id}-text`,
    x: x + w / 2 - 20,
    y: y + h / 2 - 12.5,
    width: 40,
    height: 25,
    text: "abcd",
    containerId: id,
    groupIds: extra.groupIds ?? [],
  });
  return [box, text];
};

// -- recording ------------------------------------------------------------------------

const lines = (snapLines) => clone(snapLines);

const runDrag = (up, elements, state, selected, drag) => {
  const map = up.arrayToMap(elements);
  const app = appOf(state, selected);
  const event = eventOf(up, drag.event);
  const selectedElements = elements.filter((e) => selected.includes(e.id));
  up.SnapCache.destroy();
  if (drag.cache === "app") {
    maybeCacheVisibleGaps(up, app, event, selectedElements, elements, map);
    maybeCacheReferenceSnapPoints(up, app, event, selectedElements, elements, map);
  }
  const dragOffset = { ...drag.dragOffset };
  const { snapOffset, snapLines } = up.snapDraggedElements(elements, dragOffset, app, event, map);
  up.SnapCache.destroy();
  return { ...drag, result: { dragOffset: [dragOffset.x, dragOffset.y], snapOffset: [snapOffset.x, snapOffset.y], snapLines: lines(snapLines) } };
};

const runResize = (up, elements, state, selected, resize) => {
  const map = up.arrayToMap(elements);
  const app = appOf(state, selected);
  const event = eventOf(up, resize.event);
  const originals = elements.filter((e) => selected.includes(e.id));
  up.SnapCache.destroy();
  maybeCacheReferenceSnapPoints(up, app, event, originals, elements, map);
  const { snapOffset, snapLines } = up.snapResizingElements(
    resize.latest,
    originals,
    app,
    event,
    { x: resize.dragOffset[0], y: resize.dragOffset[1] },
    resize.handle,
  );
  up.SnapCache.destroy();
  return { ...resize, latest: clone(resize.latest), result: { snapOffset: [snapOffset.x, snapOffset.y], snapLines: lines(snapLines) } };
};

const runNew = (up, elements, state, item) => {
  const map = up.arrayToMap(elements);
  const app = appOf(state);
  const event = eventOf(up, item.event);
  up.SnapCache.destroy();
  maybeCacheReferenceSnapPoints(up, app, event, [item.element], elements, map);
  const { snapOffset, snapLines } = up.snapNewElement(
    item.element,
    app,
    event,
    { x: item.origin[0], y: item.origin[1] },
    { x: item.dragOffset[0], y: item.dragOffset[1] },
    map,
  );
  up.SnapCache.destroy();
  return { ...item, element: clone(item.element), result: { snapOffset: [snapOffset.x, snapOffset.y], snapLines: lines(snapLines) } };
};

const runPointer = (up, elements, state, item) => {
  const map = up.arrayToMap(elements);
  const app = appOf(state);
  const { originOffset, snapLines } = up.getSnapLinesAtPointer(
    elements,
    app,
    { x: item.pointer[0], y: item.pointer[1] },
    eventOf(up, item.event),
    map,
  );
  return { ...item, result: { originOffset: [originOffset.x, originOffset.y], snapLines: lines(snapLines) } };
};

const buildCase = (up, { id, elements, state: stateOverrides = {}, corners = [], selected = [], drags = [], resizes = [], news = [], pointers = [] }) => {
  const state = { ...STATE, ...stateOverrides };
  const map = up.arrayToMap(elements);
  const app = appOf(state, selected);
  const selectedElements = elements.filter((e) => selected.includes(e.id));
  return {
    id,
    elements: clone(elements),
    state,
    corners: corners.map(({ ids, opts }) => ({
      ids,
      opts: opts ?? null,
      result: clone(
        opts
          ? up.getElementsCorners(elements.filter((e) => ids.includes(e.id)), map, opts)
          : up.getElementsCorners(elements.filter((e) => ids.includes(e.id)), map),
      ),
    })),
    selected,
    referencePoints: clone(up.getReferenceSnapPoints(elements, selectedElements, app.state, map)),
    visibleGaps: clone(up.getVisibleGaps(elements, selectedElements, app.state, map)),
    drags: drags.map((d) => runDrag(up, elements, state, selected, { event: { ctrlOrCmd: false }, cache: "app", ...d })),
    resizes: resizes.map((r) => runResize(up, elements, state, selected, { event: { ctrlOrCmd: false }, ...r })),
    news: news.map((n) => runNew(up, elements, state, { event: { ctrlOrCmd: false }, ...n })),
    pointers: pointers.map((p) => runPointer(up, elements, state, { event: { ctrlOrCmd: false }, ...p })),
  };
};

// -- hand-written scenes ---------------------------------------------------------------

const handCases = () => [
  {
    id: "points-aligned-row",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "b", x: 200, y: 0, width: 100, height: 100 }),
        el(up, { id: "s", x: 400, y: 300, width: 100, height: 100 }),
      ],
      selected: ["s"],
      corners: [{ ids: ["a"] }, { ids: ["a", "b"] }, { ids: ["s"], opts: { dragOffset: { x: 3.4, y: -2.25 } } }],
      drags: [
        { dragOffset: { x: 3, y: -297 } },
        { dragOffset: { x: 3, y: -292 } },
        { dragOffset: { x: 3, y: -291.9 } },
        { dragOffset: { x: -8, y: -308 } },
        { dragOffset: { x: -8.5, y: -308.5 } },
        { dragOffset: { x: -250.3333333, y: -250.1234567 } },
        { dragOffset: { x: 3, y: -297 }, event: { ctrlOrCmd: true } },
        { dragOffset: { x: 3, y: -297 }, cache: "none" },
        { dragOffset: { x: 0, y: 0 } },
      ],
      pointers: [
        { pointer: [103, 57] },
        { pointer: [96, 100] },
        { pointer: [250, 4] },
        { pointer: [150, 250] },
        { pointer: [108, 108] },
        { pointer: [108.1, 91.9] },
      ],
      news: [
        { element: el(up, { id: "n", x: 97, y: 202, width: 30, height: 40 }), origin: [97, 202], dragOffset: [30, 40] },
        { element: el(up, { id: "n", x: 150, y: 150, width: 52, height: -46 }), origin: [150, 150], dragOffset: [52, -46] },
      ],
    }),
  },
  {
    id: "gaps-horizontal",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "b", x: 150, y: 20, width: 100, height: 60 }),
        el(up, { id: "c", x: 500, y: -40, width: 80, height: 100 }),
        el(up, { id: "s", x: 300, y: 400, width: 100, height: 50 }),
      ],
      selected: ["s"],
      drags: [
        { dragOffset: { x: -2, y: -380 } },
        { dragOffset: { x: 3, y: -377 } },
        { dragOffset: { x: 55, y: -370 } },
        { dragOffset: { x: 51, y: -370 } },
        { dragOffset: { x: -226, y: -380 } },
        { dragOffset: { x: -350, y: -375 } },
        { dragOffset: { x: 105, y: -365 } },
        // side_right, side_left and center_horizontal, 2 or 3 off
        { dragOffset: { x: -2, y: -380 } },
        { dragOffset: { x: -453, y: -380 } },
        { dragOffset: { x: 27, y: -380 } },
      ],
    }),
  },
  {
    id: "gaps-vertical",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "b", x: 20, y: 160, width: 60, height: 100 }),
        el(up, { id: "c", x: -30, y: 520, width: 100, height: 60 }),
        el(up, { id: "s", x: 400, y: 300, width: 50, height: 100 }),
      ],
      selected: ["s"],
      drags: [
        { dragOffset: { x: -380, y: 2 } },
        { dragOffset: { x: -377, y: 4 } },
        { dragOffset: { x: -370, y: 58 } },
        { dragOffset: { x: -370, y: -218 } },
        { dragOffset: { x: -375, y: -365 } },
        { dragOffset: { x: -375, y: 25 } },
        // side_bottom, side_top and center_vertical, 2 off
        { dragOffset: { x: -380, y: 22 } },
        { dragOffset: { x: -380, y: -458 } },
        { dragOffset: { x: -380, y: 42 } },
      ],
    }),
  },
  {
    id: "gaps-grid",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "b", x: 200, y: 0, width: 100, height: 100 }),
        el(up, { id: "c", x: 0, y: 200, width: 100, height: 100 }),
        el(up, { id: "d", x: 200, y: 200, width: 100, height: 100 }),
        el(up, { id: "e", x: 400, y: 0, width: 100, height: 100 }),
        el(up, { id: "s", x: 700, y: 700, width: 100, height: 100 }),
      ],
      selected: ["s"],
      drags: [
        { dragOffset: { x: -98, y: -503 } },
        { dragOffset: { x: -300, y: -500 } },
        { dragOffset: { x: -500, y: -702 } },
        { dragOffset: { x: -103, y: -697 } },
        { dragOffset: { x: -1, y: -699 } },
      ],
    }),
  },
  {
    id: "diamond-ellipse-rotated",
    build: (up) => ({
      elements: [
        el(up, { type: "diamond", id: "d", x: 0, y: 0, width: 120, height: 80 }),
        el(up, { type: "ellipse", id: "e", x: 200, y: 0, width: 80, height: 80, angle: 0.3 }),
        el(up, { id: "r", x: 0, y: 200, width: 100, height: 60, angle: Math.PI / 6 }),
        el(up, { type: "diamond", id: "s", x: 400, y: 300, width: 60, height: 60, angle: 1 }),
      ],
      selected: ["s"],
      corners: [
        { ids: ["d"] },
        { ids: ["d"], opts: { omitCenter: true } },
        { ids: ["d"], opts: { boundingBoxCorners: true } },
        { ids: ["e"] },
        { ids: ["e"], opts: { boundingBoxCorners: true, omitCenter: true } },
        { ids: ["r"] },
        { ids: ["r"], opts: { dragOffset: { x: 10, y: 20 } } },
        { ids: ["d", "e"] },
        { ids: ["d", "r"], opts: { omitCenter: true, dragOffset: { x: -5.5, y: 2 } } },
        { ids: [] },
      ],
      drags: [
        { dragOffset: { x: -335, y: -290 } },
        { dragOffset: { x: -165, y: -300 } },
        { dragOffset: { x: -385, y: -60 } },
      ],
      pointers: [{ pointer: [59, 3] }, { pointer: [238, 41] }, { pointer: [2, 239] }],
    }),
  },
  {
    id: "groups-and-bound-text",
    build: (up) => {
      const [box, text] = boxWithText(up, "box", 0, 0, 120, 80);
      const [gbox, gtext] = boxWithText(up, "gbox", 300, 0, 100, 100, { groupIds: ["g1"] });
      return {
        elements: [
          box,
          text,
          gbox,
          el(up, { id: "g2", x: 450, y: 150, width: 50, height: 50, groupIds: ["g1"] }),
          gtext,
          el(up, { id: "nested", x: 0, y: 300, width: 50, height: 50, groupIds: ["inner", "outer"] }),
          el(up, { id: "nested2", x: 100, y: 330, width: 50, height: 50, groupIds: ["inner2", "outer"] }),
          el(up, { id: "s", x: 600, y: 600, width: 40, height: 40 }),
          el(up, { id: "s2", x: 700, y: 650, width: 40, height: 60 }),
        ],
        selected: ["s", "s2"],
        corners: [{ ids: ["box", "box-text"] }, { ids: ["s", "s2"] }, { ids: ["s", "s2"], opts: { dragOffset: { x: 1, y: 2 } } }],
        drags: [
          { dragOffset: { x: -598, y: -599 } },
          { dragOffset: { x: -305, y: -452 } },
          { dragOffset: { x: -701, y: -305 } },
          { dragOffset: { x: -150, y: -547 } },
        ],
        pointers: [{ pointer: [59, 42] }, { pointer: [400, 153] }, { pointer: [148, 303] }],
      };
    },
  },
  {
    id: "linear-and-text",
    build: (up) => ({
      elements: [
        el(up, { type: "line", id: "l", x: 0, y: 0, width: 100, height: 50, points: [[0, 0], [100, 50]] }),
        el(up, { type: "arrow", id: "ar", x: 200, y: 0, width: 100, height: 80, points: [[0, 0], [50, 80], [100, 0]], roundness: { type: 2 } }),
        el(up, { type: "text", id: "t", x: 0, y: 200, width: 80, height: 25, text: "abcdefgh" }),
        el(up, { type: "freedraw", id: "f", x: 200, y: 200, width: 60, height: 40, points: [[0, 0], [30, 40], [60, 10]] }),
        el(up, { type: "arrow", id: "sa", x: 500, y: 500, width: 100, height: 100 }),
        el(up, { id: "s", x: 600, y: 300, width: 40, height: 40 }),
      ],
      selected: ["s"],
      corners: [{ ids: ["l"] }, { ids: ["ar"] }, { ids: ["f"] }, { ids: ["l", "ar", "f"] }],
      drags: [
        { dragOffset: { x: -597, y: -254 } },
        { dragOffset: { x: -497, y: -302 } },
        { dragOffset: { x: -383, y: -103 } },
      ],
      pointers: [{ pointer: [98, 52] }, { pointer: [248, 3] }, { pointer: [78, 227] }],
    }),
  },
  {
    id: "zoom-and-viewport",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "far", x: 3000, y: 0, width: 100, height: 100 }),
        el(up, { id: "far2", x: 0, y: 3000, width: 100, height: 100 }),
        el(up, { id: "edge", x: 1180, y: 880, width: 100, height: 100 }),
        el(up, { id: "s", x: 400, y: 400, width: 100, height: 100 }),
      ],
      selected: ["s"],
      state: { zoom: 2, scrollX: -10, scrollY: 20, width: 800, height: 600, offsetLeft: 30, offsetTop: 40 },
      drags: [
        { dragOffset: { x: -397, y: -396 } },
        { dragOffset: { x: -395, y: -395 } },
        { dragOffset: { x: 2598, y: -398 } },
      ],
      pointers: [{ pointer: [103, 3] }, { pointer: [105, 95] }, { pointer: [3002, 2] }],
    }),
  },
  {
    id: "zoom-out",
    build: (up) => ({
      elements: [
        el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
        el(up, { id: "far", x: 2000, y: 0, width: 100, height: 100 }),
        el(up, { id: "s", x: 400, y: 400, width: 100, height: 100 }),
      ],
      selected: ["s"],
      state: { zoom: 0.5 },
      drags: [
        { dragOffset: { x: -385, y: -415 } },
        { dragOffset: { x: -383, y: -417 } },
        { dragOffset: { x: 1585, y: -390 } },
      ],
      pointers: [{ pointer: [115, 90] }],
    }),
  },
  {
    id: "modes",
    build: (up) => ({
      elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), el(up, { id: "s", x: 300, y: 300, width: 100, height: 100 })],
      selected: ["s"],
      state: { objectsSnapModeEnabled: false },
      drags: [
        { dragOffset: { x: -198, y: -297 } },
        { dragOffset: { x: -198, y: -297 }, event: { ctrlOrCmd: true } },
      ],
      pointers: [{ pointer: [103, 57] }, { pointer: [103, 57], event: { ctrlOrCmd: true } }],
    }),
  },
  {
    id: "modes-grid",
    build: (up) => ({
      elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), el(up, { id: "s", x: 300, y: 300, width: 100, height: 100 })],
      selected: ["s"],
      state: { objectsSnapModeEnabled: false, gridModeEnabled: true },
      drags: [{ dragOffset: { x: -198, y: -297 }, event: { ctrlOrCmd: true } }],
    }),
  },
  {
    id: "modes-lasso",
    build: (up) => ({
      elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), el(up, { id: "s", x: 300, y: 300, width: 100, height: 100 })],
      selected: ["s"],
      state: { activeTool: "lasso" },
      drags: [{ dragOffset: { x: -198, y: -297 } }],
    }),
  },
  {
    id: "modes-lasso-dragging",
    build: (up) => ({
      elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), el(up, { id: "s", x: 300, y: 300, width: 100, height: 100 })],
      selected: ["s"],
      state: { activeTool: "lasso", selectedElementsAreBeingDragged: true },
      drags: [{ dragOffset: { x: -198, y: -297 } }],
    }),
  },
  {
    id: "no-selection",
    build: (up) => ({
      elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), el(up, { id: "b", x: 300, y: 300, width: 100, height: 100 })],
      selected: [],
      drags: [{ dragOffset: { x: -198, y: -297 } }],
    }),
  },
  {
    id: "resizes",
    build: (up) => {
      const s = el(up, { id: "s", x: 300, y: 300, width: 100, height: 100 });
      const resized = (dx, dy, handle) => {
        const e = clone(s);
        if (handle.includes("e")) e.width += dx;
        else if (handle.includes("w")) {
          e.x += dx;
          e.width -= dx;
        }
        if (handle.includes("n")) {
          e.y += dy;
          e.height -= dy;
        } else if (handle.includes("s")) e.height += dy;
        return [e];
      };
      const resizes = [];
      for (const handle of ["n", "s", "e", "w", "ne", "nw", "se", "sw", "rotation", false]) {
        for (const [dx, dy] of [
          [-197, -3],
          [3, 3],
          [-103, -298],
          [-296, -196],
          [-104.5, -202.25],
        ]) {
          resizes.push({ handle, dragOffset: [dx, dy], latest: resized(dx, dy, handle || "") });
        }
      }
      const rotated = { ...clone(s), angle: 0.5 };
      resizes.push({ handle: "e", dragOffset: [3, 3], latest: [rotated] });
      resizes.push({ handle: "e", dragOffset: [3, 3], latest: [{ ...clone(s), angle: 0.005 }] });
      resizes.push({ handle: "se", dragOffset: [3, 3], latest: resized(3, 3, "se"), event: { ctrlOrCmd: true } });
      return {
        elements: [
          el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }),
          el(up, { type: "ellipse", id: "b", x: 200, y: 100, width: 100, height: 100 }),
          el(up, { id: "c", x: 500, y: 500, width: 50, height: 50 }),
          s,
        ],
        selected: ["s"],
        resizes,
      };
    },
  },
  {
    id: "resizes-multi",
    build: (up) => {
      const s1 = el(up, { id: "s1", x: 300, y: 300, width: 50, height: 50 });
      const s2 = el(up, { id: "s2", x: 380, y: 320, width: 50, height: 80 });
      return {
        elements: [el(up, { id: "a", x: 0, y: 0, width: 100, height: 100 }), s1, s2],
        selected: ["s1", "s2"],
        resizes: [
          { handle: "se", dragOffset: [-327, -297], latest: [{ ...clone(s1), width: 20, height: 20 }, { ...clone(s2), x: 330, width: 20 }] },
          { handle: "nw", dragOffset: [-296, -304], latest: [{ ...clone(s1), angle: 1 }, clone(s2)] },
        ],
      };
    },
  },
];

// -- random scenes --------------------------------------------------------------------

const TYPES = ["rectangle", "rectangle", "rectangle", "diamond", "ellipse", "text", "line", "arrow", "frame"];

const randomCase = (id, seed) => ({
  id,
  build: (up) => {
    const r = rng(seed);
    const count = r.int(3, 9);
    const elements = [];
    const grid = r.pick([10, 20, 25]);
    for (let i = 0; i < count; i++) {
      const type = r.pick(TYPES);
      const x = r.int(-10, 40) * grid + (r.chance(0.25) ? q(r.next() * 8 - 4) : 0);
      const y = r.int(-10, 30) * grid + (r.chance(0.25) ? q(r.next() * 8 - 4) : 0);
      const width = r.int(2, 12) * grid;
      const height = r.int(2, 12) * grid;
      const angle = r.chance(0.15) ? q(r.next() * 6) : 0;
      const groupIds = r.chance(0.2) ? [r.pick(["g1", "g2"])] : [];
      const opts = { type, id: `e${i}`, x, y, width, height, angle, groupIds };
      if (type === "line" || type === "arrow") {
        opts.points = [[0, 0], [width, r.chance(0.5) ? height : -height]];
      }
      if (type === "text") {
        opts.text = "abcdef".slice(0, r.int(1, 6));
        opts.width = opts.text.length * 10;
        opts.height = 25;
      }
      if (r.chance(0.15) && (type === "rectangle" || type === "diamond" || type === "ellipse")) {
        elements.push(...boxWithText(up, `e${i}`, x, y, width, height, { type, angle, groupIds }));
      } else {
        elements.push(el(up, opts));
      }
    }
    const candidates = elements.filter((e) => !(e.type === "text" && e.containerId));
    const selCount = r.int(1, Math.min(3, candidates.length - 1));
    const selected = [];
    while (selected.length < selCount) {
      const e = r.pick(candidates);
      if (!selected.includes(e.id)) selected.push(e.id);
    }
    const others = candidates.filter((e) => !selected.includes(e.id));
    const selectedEl = elements.find((e) => e.id === selected[0]);
    const zoom = r.pick([1, 1, 1, 2, 0.5, 1.25]);
    const state = { zoom, width: r.pick([1200, 1600]), height: r.pick([900, 1200]) };
    // Aim a selected element's corner at another element's corner, plus a
    // little jitter, so most drags land inside the snap distance.
    const drags = [];
    for (let k = 0; k < 6; k++) {
      const target = r.pick(others);
      const tx = target.x + r.pick([0, target.width / 2, target.width]);
      const ty = target.y + r.pick([0, target.height / 2, target.height]);
      const sx = selectedEl.x + r.pick([0, selectedEl.width / 2, selectedEl.width]);
      const sy = selectedEl.y + r.pick([0, selectedEl.height / 2, selectedEl.height]);
      const gap = r.chance(0.3) ? r.pick([-1, 1]) * r.int(1, 8) * grid : 0;
      drags.push({
        dragOffset: {
          x: q(tx - sx + gap + (r.next() * 20 - 10)),
          y: q(ty - sy + (r.next() * 20 - 10)),
        },
        event: { ctrlOrCmd: r.chance(0.1) },
      });
    }
    const pointers = [];
    for (let k = 0; k < 4; k++) {
      const target = r.pick(candidates);
      pointers.push({ pointer: [q(target.x + r.next() * 20 - 10), q(target.y + target.height + r.next() * 20 - 10)] });
    }
    const news = [];
    for (let k = 0; k < 2; k++) {
      const target = r.pick(candidates);
      const origin = [q(target.x + r.next() * 16 - 8), q(target.y + r.next() * 16 - 8)];
      const dragOffset = [r.int(-100, 100), r.int(-100, 100)];
      news.push({
        element: el(up, { type: r.pick(["rectangle", "ellipse", "diamond"]), id: "new", x: origin[0], y: origin[1], width: dragOffset[0], height: dragOffset[1] }),
        origin,
        dragOffset,
      });
    }
    const resizes = [];
    const originals = elements.filter((e) => selected.includes(e.id));
    for (let k = 0; k < 3; k++) {
      const handle = r.pick(["n", "s", "e", "w", "ne", "nw", "se", "sw"]);
      const target = r.pick(others);
      const dragOffset = [q(target.x - selectedEl.x + r.next() * 20 - 10), q(target.y - selectedEl.y + r.next() * 20 - 10)];
      resizes.push({ handle, dragOffset, latest: originals.map((e) => ({ ...clone(e), width: e.width + 10 })) });
    }
    return { elements, selected, state, drags, pointers, news, resizes, corners: [{ ids: selected }, { ids: [others[0].id] }] };
  },
});

// -- isSnappingEnabled and friends ------------------------------------------------------

const enabledCases = (up) => {
  const out = [];
  const rect = el(up, { id: "r" });
  const arrow = el(up, { type: "arrow", id: "a" });
  const line = el(up, { type: "line", id: "l" });
  const sets = { none: [], rect: [rect], arrow: [arrow], line: [line], "arrow+rect": [arrow, rect] };
  for (const objectsSnapModeEnabled of [true, false]) {
    for (const gridModeEnabled of [true, false]) {
      for (const activeTool of ["selection", "lasso", "rectangle"]) {
        for (const selectedElementsAreBeingDragged of [true, false]) {
          for (const event of [null, { ctrlOrCmd: false }, { ctrlOrCmd: true }]) {
            for (const [name, selected] of Object.entries(sets)) {
              const state = { ...STATE, objectsSnapModeEnabled, gridModeEnabled, activeTool, selectedElementsAreBeingDragged };
              const result = up.isSnappingEnabled({ app: appOf(state), event: eventOf(up, event), selectedElements: selected });
              out.push({ state: { objectsSnapModeEnabled, gridModeEnabled, activeTool, selectedElementsAreBeingDragged }, event, selected: clone(selected), result });
            }
          }
        }
      }
    }
  }
  return out;
};

const TOOLS = [
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
  "stickynote",
  "autoshape",
  "bucketfill",
];

// -- renderSnaps ------------------------------------------------------------------------

/** A 2D context that records every call and assignment as it is made. */
class CallContext {
  constructor() {
    this.calls = [];
  }
  record(name, args) {
    this.calls.push([name, ...args]);
  }
  set lineWidth(v) {
    this.record("lineWidth", [v]);
  }
  set strokeStyle(v) {
    this.record("strokeStyle", [v]);
  }
}
for (const name of ["save", "restore", "translate", "beginPath", "moveTo", "lineTo", "stroke"]) {
  CallContext.prototype[name] = function call(...args) {
    this.record(name, args);
  };
}

const RENDER_STATES = [
  { theme: "light", zenModeEnabled: false, zoom: 1, scrollX: 0, scrollY: 0 },
  { theme: "dark", zenModeEnabled: false, zoom: 2, scrollX: 12.5, scrollY: -40 },
  { theme: "dark", zenModeEnabled: true, zoom: 0.5, scrollX: -3, scrollY: 7 },
  { theme: "light", zenModeEnabled: true, zoom: 1.25, scrollX: 100, scrollY: 50 },
];

const renderCases = (up, cases) => {
  const sets = [[]];
  for (const c of cases) {
    for (const item of [...c.drags, ...c.resizes, ...c.news, ...c.pointers]) {
      if (item.result.snapLines.length) sets.push(item.result.snapLines);
    }
  }
  const kinds = (lines) => new Set(lines.map((l) => `${l.type}/${l.direction ?? ""}`));
  // Every set with a kind of snap line not seen yet, then every eighth.
  const seen = new Set();
  const picked = sets.filter((lines, i) => {
    const fresh = [...kinds(lines)].some((k) => !seen.has(k));
    for (const k of kinds(lines)) seen.add(k);
    return fresh || i % 8 === 0;
  });
  const out = [];
  picked.forEach((snapLines, i) => {
    for (const state of i === 0 ? RENDER_STATES : [RENDER_STATES[i % RENDER_STATES.length], RENDER_STATES[(i + 1) % RENDER_STATES.length]]) {
      const context = new CallContext();
      up.renderSnaps(context, { ...state, zoom: { value: state.zoom }, snapLines });
      out.push({ state, snapLines, calls: clone(context.calls) });
    }
  });
  return out;
};

// -- the fixture -------------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating snapping fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  const cases = [];
  const specs = [...handCases()];
  for (let i = 0; i < 80; i++) specs.push(randomCase(`random-${i}`, 1000 + i * 7919));
  for (const { id, build } of specs) {
    up.reseed(1);
    cases.push(buildCase(up, { id, ...build(up) }));
  }
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  up.reseed(1);
  return format({
    description:
      "getSnapDistance, isSnappingEnabled, isActiveToolNonLinearSnappable, getElementsCorners, getReferenceSnapPoints, getVisibleGaps, snapDraggedElements, snapResizingElements, snapNewElement, getSnapLinesAtPointer (packages/excalidraw/snapping.ts) with the snap cache filled as App.tsx fills it, and renderSnaps (renderer/renderSnaps.ts) on a recording context, in upstream's test mode (ids id0.., timestamps 1, reseed(1) before each case). Generated by tools/goldens/snapping-fixtures.mjs.",
    upstream: commit,
    snapDistance: [1, 2, 0.5, 0.1, 1.25, 30].map((zoom) => ({ zoom, result: up.getSnapDistance(zoom) })),
    nonLinearSnappable: TOOLS.map((tool) => ({ tool, result: up.isActiveToolNonLinearSnappable(tool) })),
    enabled: enabledCases(up),
    cases,
    renders: renderCases(up, cases),
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`snapping-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (value) => value.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("snapping fixture is out of date: run node tools/goldens/snapping-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`snapping fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
