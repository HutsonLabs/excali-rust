#!/usr/bin/env node
// Editing fixtures for excali-editor (ex-712): upstream's own selection,
// group, element creation and editing code, run from the pinned checkout
// under plain Node, on hand-written scenes of the gestures the
// <excali-editor> element wires (box selection, drawing, the edit actions).
//
//   node tools/goldens/editing-fixtures.mjs            write the fixture
//   node tools/goldens/editing-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/editing-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/editing.json:
//
//   { "description", "upstream", "cases": [ { id, kind, ... } ] }
//
// Every case holds its scene (`elements`, JSON as upstream holds them) and
// what upstream answered. Kinds:
//
// - "within": `selection` (the selection element's x, y, width, height)
//   and `results`, each { mode, excludeElementsInFrames, ids }:
//   getElementsWithinSelection(elements, selection, elementsMap,
//   excludeElementsInFrames, mode) (packages/element/src/selection.ts:70),
//   the ids in the order returned.
// - "groups": `appState` ({ selectedElementIds, editingGroupId }) and
//   `result`: selectGroupsForSelectedElements(appState, elements,
//   appState, null) (packages/element/src/groups.ts:68-196).
// - "new": `tool`, `appState` (the keys that differ from
//   getDefaultAppState() in test mode, where currentItemRoundness is
//   "sharp"), `origin` and `element`: the element App creates
//   when the tool's pointer goes down at `origin` (grid off), with the
//   attributes App passes (createGenericElementOnPointerDown
//   App.tsx:10534-10601, handleLinearElementOnPointerDown :10313-10408 with
//   its two points, handleFreeDrawElementOnPointerDown :9988-10035 for a
//   mouse, createFrameElementOnPointerDown :10603-10634) through upstream's
//   newElement, newArrowElement, newLinearElement, newFreeDrawElement and
//   newFrameElement, as constructed (App then gives a line or an arrow its
//   two points with mutateElement). id, seed, versionNonce and updated are
//   drawn and left out.
// - "drag": `element` (at `origin`) and `drags`, each { to,
//   shouldMaintainAspectRatio, shouldResizeFromCenter, result }:
//   dragNewElement({ newElement, elementType, originX, originY, x, y,
//   width: |x - originX|, height: |y - originY|, ... }) as
//   maybeDragNewGenericElement calls it (App.tsx:13550-13569), from the
//   element as created each time; result is its { x, y, width, height }.
// - "perfect": `items`, each { type, width, height, result }:
//   getPerfectElementSize (packages/element/src/sizeHelpers.ts:158-185).
// - "eraser": `paths`, each { points, restore, steps }: an EraserTrail
//   (packages/excalidraw/eraser/index.ts) on an app whose visible elements
//   are the scene's non-deleted ones, startPath at the first point then
//   addPointToPath(x, y, restore) for each further one; steps holds what
//   each call returned (the ids to erase).
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "editing.json";

const ENTRY = `
export { getElementsWithinSelection } from "./packages/element/src/selection";
export { selectGroupsForSelectedElements } from "./packages/element/src/groups";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
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
  newFreeDrawElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { dragNewElement } from "./packages/element/src/dragElements";
export { getPerfectElementSize } from "./packages/element/src/sizeHelpers";
export { Scene } from "./packages/element/src/Scene";
export { EraserTrail } from "./packages/excalidraw/eraser/index";
export {
  ARROW_TYPE,
  DEFAULT_STROKE_STREAMLINE,
  FRAME_STYLE,
} from "./packages/common/src/constants";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const usage = () => {
  process.stderr.write("usage: editing-fixtures.mjs [--check] [--out DIR]\n");
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

// -- scenes -------------------------------------------------------------------

/** A filled element as API.createElement leaves it. */
const el = (up, type, id, x, y, width = 100, height = 100, rest = {}) =>
  apiCreateElement(up, { type, id, x, y, width, height, backgroundColor: "#a5d8ff", ...rest });

/** A container with a bound label ("label" text in it). */
const labelled = (up, id, x, y, width = 200, height = 100, rest = {}) => {
  const container = el(up, "rectangle", id, x, y, width, height, {
    boundElements: [{ type: "text", id: `${id}-label` }],
    ...rest,
  });
  const label = apiCreateElement(up, {
    type: "text",
    id: `${id}-label`,
    x: x + width / 2 - 25,
    y: y + height / 2 - 12.5,
    width: 50,
    height: 25,
    text: "label",
    containerId: id,
    groupIds: rest.groupIds ?? [],
  });
  return [container, label];
};

/** An arrow from (x, y) through `points` (relative). */
const arrow = (up, id, x, y, points, rest = {}) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return apiCreateElement(up, {
    type: "arrow",
    id,
    x,
    y,
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
    points,
    ...rest,
  });
};

const THREE = (up) => [
  el(up, "rectangle", "a", 100, 100),
  el(up, "rectangle", "b", 300, 100),
  el(up, "rectangle", "c", 600, 400),
];

const BOXES = [
  // the parity row: around a and b
  [50, 50, 400, 200],
  // a partly
  [50, 50, 100, 100],
  // just inside a (a's stroke pokes out)
  [99.5, 99.5, 101, 101],
  // exactly a's box grown by half the stroke
  [99, 99, 102, 102],
  // everything
  [0, 0, 1000, 1000],
  // nothing
  [800, 0, 100, 100],
];

const SCENES = (up) => [
  ["three", THREE(up), BOXES],
  [
    "shapes",
    [
      el(up, "ellipse", "e", 100, 100, 120, 80),
      el(up, "diamond", "d", 300, 100, 100, 140),
      el(up, "rectangle", "rot", 500, 100, 120, 60, { angle: Math.PI / 6 }),
      el(up, "rectangle", "locked", 100, 300, 50, 50, { locked: true }),
      arrow(up, "arr", 300, 300, [
        [0, 0],
        [150, 60],
      ]),
      apiCreateElement(up, {
        type: "line",
        id: "ln",
        x: 520,
        y: 300,
        width: 80,
        height: 80,
        points: [
          [0, 0],
          [80, 80],
        ],
      }),
    ],
    [
      [50, 50, 200, 200],
      [80, 110, 200, 60],
      [250, 50, 200, 200],
      [450, 50, 250, 200],
      [470, 70, 170, 120],
      [80, 280, 100, 100],
      [280, 280, 200, 100],
      [320, 310, 60, 30],
      [500, 290, 120, 120],
      [540, 320, 20, 20],
    ],
  ],
  [
    "labels",
    [
      ...labelled(up, "box", 100, 100),
      arrow(up, "lbl-arrow", 100, 400, [
        [0, 0],
        [300, 0],
      ], { boundElements: [{ type: "text", id: "arrow-label" }] }),
      apiCreateElement(up, {
        type: "text",
        id: "arrow-label",
        x: 225,
        y: 387.5,
        width: 50,
        height: 25,
        text: "label",
        containerId: "lbl-arrow",
      }),
    ],
    [
      [50, 50, 300, 200],
      [150, 120, 100, 60],
      [50, 350, 400, 100],
      [200, 380, 100, 40],
    ],
  ],
  [
    "groups",
    [
      el(up, "rectangle", "g1", 100, 100, 100, 100, { groupIds: ["inner", "outer"] }),
      el(up, "rectangle", "g2", 300, 100, 100, 100, { groupIds: ["inner", "outer"] }),
      el(up, "rectangle", "g3", 500, 100, 100, 100, { groupIds: ["outer"] }),
      el(up, "rectangle", "solo", 100, 400, 100, 100, { groupIds: ["lonely"] }),
    ],
    [
      [50, 50, 400, 200],
      [50, 50, 600, 200],
      [50, 50, 150, 200],
      [50, 350, 200, 200],
    ],
  ],
  [
    "frames",
    [
      apiCreateElement(up, { type: "frame", id: "f", x: 100, y: 100, width: 400, height: 300 }),
      el(up, "rectangle", "in", 150, 150, 100, 100, { frameId: "f" }),
      el(up, "rectangle", "out", 450, 350, 100, 100, { frameId: "f" }),
      el(up, "rectangle", "free", 600, 100),
    ],
    [
      [50, 50, 500, 400],
      [140, 140, 120, 120],
      [440, 340, 120, 120],
      [50, 50, 700, 450],
    ],
  ],
];

const selectionElement = (up, [x, y, width, height]) =>
  up.newElement({ type: "selection", x, y, width, height });

const withinCases = () =>
  ["three", "shapes", "labels", "groups", "frames"].map((name) => ({
    id: `within-${name}`,
    build: (up) => {
      const [, elements, boxes] = SCENES(up).find(([n]) => n === name);
      const elementsMap = up.arrayToMap(elements);
      const selections = boxes.map((box) => {
        const selection = selectionElement(up, box);
        const results = [];
        for (const mode of ["contain", "overlap"]) {
          for (const exclude of [false, true]) {
            const ids = up
              .getElementsWithinSelection(elements, selection, elementsMap, exclude, mode)
              .map((e) => e.id);
            results.push({ mode, excludeElementsInFrames: exclude, ids });
          }
        }
        return { box, results };
      });
      return { id: `within-${name}`, kind: "within", elements: clone(elements), selections };
    },
  }));

const GROUP_SELECTIONS = [
  [{ g1: true }, null],
  [{ g3: true }, null],
  [{ g1: true }, "outer"],
  [{ g1: true }, "inner"],
  [{ solo: true }, null],
  [{ solo: true, g3: true }, null],
  [{}, null],
  [{ g2: true }, "lonely"],
];

const groupCases = () =>
  GROUP_SELECTIONS.map(([selectedElementIds, editingGroupId], i) => ({
    id: `groups-${i}`,
    build: (up) => {
      const [, elements] = SCENES(up).find(([n]) => n === "groups");
      const appState = { ...up.getDefaultAppState(), selectedElementIds, editingGroupId };
      const result = up.selectGroupsForSelectedElements(appState, elements, appState, null);
      return {
        id: `groups-${i}`,
        kind: "groups",
        elements: clone(elements),
        appState: { selectedElementIds, editingGroupId },
        result: {
          selectedElementIds: result.selectedElementIds,
          selectedGroupIds: result.selectedGroupIds,
          editingGroupId: result.editingGroupId,
        },
      };
    },
  }));

// -- element creation -----------------------------------------------------------

/** App's attributes for a new element of `tool` (see the "new" kind). */
const createElement = (up, tool, appState, [x, y]) => {
  const strokeWidth = up.getStrokeWidthByKey(tool, appState.currentItemStrokeWidthKey);
  const base = {
    x,
    y,
    strokeColor: appState.currentItemStrokeColor,
    backgroundColor: appState.currentItemBackgroundColor,
    fillStyle: appState.currentItemFillStyle,
    strokeWidth,
    strokeStyle: appState.currentItemStrokeStyle,
    roughness: appState.currentItemRoughness,
    opacity: appState.currentItemOpacity,
    locked: false,
    frameId: null,
  };
  switch (tool) {
    case "rectangle":
    case "diamond":
    case "ellipse":
    case "selection":
      return up.newElement({
        type: tool,
        ...base,
        roundness:
          appState.currentItemRoundness === "round"
            ? {
                type: up.isUsingAdaptiveRadius(tool)
                  ? up.ROUNDNESS.ADAPTIVE_RADIUS
                  : up.ROUNDNESS.PROPORTIONAL_RADIUS,
              }
            : null,
      });
    case "arrow": {
      const element = up.newArrowElement({
        type: "arrow",
        ...base,
        roundness:
          appState.currentItemArrowType === up.ARROW_TYPE.round
            ? { type: up.ROUNDNESS.PROPORTIONAL_RADIUS }
            : null,
        startArrowhead: appState.currentItemStartArrowhead,
        endArrowhead: appState.currentItemEndArrowhead,
        elbowed: appState.currentItemArrowType === up.ARROW_TYPE.elbow,
        fixedSegments: appState.currentItemArrowType === up.ARROW_TYPE.elbow ? [] : null,
      });
      return element;
    }
    case "line": {
      const element = up.newLinearElement({
        type: "line",
        ...base,
        roundness:
          appState.currentItemRoundness === "round"
            ? { type: up.ROUNDNESS.PROPORTIONAL_RADIUS }
            : null,
      });
      return element;
    }
    case "freedraw":
      return up.newFreeDrawElement({
        type: "freedraw",
        ...base,
        strokeWidth: up.getStrokeWidthByKey("freedraw", appState.currentItemStrokeWidthKey),
        roundness: null,
        simulatePressure: true,
        strokeOptions: {
          variability: appState.currentItemStrokeVariability,
          streamline: up.DEFAULT_STROKE_STREAMLINE,
        },
        points: [[0, 0]],
        pressures: [],
      });
    case "frame":
      return up.newFrameElement({
        x,
        y,
        opacity: appState.currentItemOpacity,
        locked: false,
        ...up.FRAME_STYLE,
      });
    default:
      throw new Error(`createElement: ${tool}`);
  }
};

const withoutDrawn = (element) => {
  const copy = clone(element);
  for (const key of ["id", "seed", "versionNonce", "updated"]) delete copy[key];
  return copy;
};

const APP_STATES = [
  ["default", {}],
  ["round", { currentItemRoundness: "round" }],
  ["sharp", { currentItemRoundness: "sharp", currentItemArrowType: "sharp" }],
  [
    "styled",
    {
      currentItemStrokeColor: "#e03131",
      currentItemBackgroundColor: "#ffc9c9",
      currentItemFillStyle: "hachure",
      currentItemStrokeWidthKey: "bold",
      currentItemStrokeStyle: "dashed",
      currentItemRoughness: 2,
      currentItemOpacity: 60,
      currentItemStartArrowhead: "bar",
      currentItemEndArrowhead: "triangle",
      currentItemStrokeVariability: "variable",
    },
  ],
  ["elbow", { currentItemArrowType: "elbow" }],
];

const TOOLS = ["rectangle", "diamond", "ellipse", "arrow", "line", "freedraw", "frame"];

const newCases = () =>
  APP_STATES.flatMap(([name, overrides]) =>
    TOOLS.map((tool) => ({
      id: `new-${tool}-${name}`,
      build: (up) => {
        const appState = { ...up.getDefaultAppState(), ...overrides };
        const element = createElement(up, tool, appState, [100, 100]);
        return {
          id: `new-${tool}-${name}`,
          kind: "new",
          tool,
          appState: overrides,
          origin: [100, 100],
          element: withoutDrawn(element),
        };
      },
    })),
  );

const DRAG_TARGETS = [
  [250, 200],
  [100, 100],
  [100, 250],
  [250, 100],
  [40, 30],
  [40, 260],
  [300, 60],
  [110.5, 180.25],
];

const dragCases = () =>
  ["rectangle", "ellipse", "diamond", "frame", "selection"].map((tool) => ({
    id: `drag-${tool}`,
    build: (up) => {
      const appState = up.getDefaultAppState();
      const origin = [100, 100];
      const drags = [];
      let first = null;
      for (const to of DRAG_TARGETS) {
        for (const shouldMaintainAspectRatio of [false, true]) {
          for (const shouldResizeFromCenter of [false, true]) {
            const element = createElement(up, tool, appState, origin);
            first ??= withoutDrawn(element);
            const scene = new up.Scene([element], { skipValidation: true });
            up.dragNewElement({
              newElement: element,
              elementType: tool,
              originX: origin[0],
              originY: origin[1],
              x: to[0],
              y: to[1],
              width: Math.abs(to[0] - origin[0]),
              height: Math.abs(to[1] - origin[1]),
              shouldMaintainAspectRatio,
              shouldResizeFromCenter,
              zoom: 1,
              scene,
              informMutation: false,
            });
            const { x, y, width, height } = scene.getElement(element.id);
            drags.push({
              to,
              shouldMaintainAspectRatio,
              shouldResizeFromCenter,
              result: { x, y, width, height },
            });
          }
        }
      }
      return { id: `drag-${tool}`, kind: "drag", element: first, origin, drags };
    },
  }));

const perfectCases = () => [
  {
    id: "perfect",
    build: (up) => {
      const items = [];
      for (const type of ["line", "arrow", "freedraw", "rectangle", "ellipse", "selection"]) {
        for (const [width, height] of [
          [150, 100],
          [150, -100],
          [-150, 100],
          [100, 150],
          [100, 5],
          [5, 100],
          [100, 100],
          [100, 0],
          [0, 100],
          [100, 27],
          [100, 58],
        ]) {
          items.push({ type, width, height, result: up.getPerfectElementSize(type, width, height) });
        }
      }
      return { id: "perfect", kind: "perfect", items };
    },
  },
];

// -- the eraser -------------------------------------------------------------------

const ERASER_SCENES = (up) => {
  const transparent = (type, id, x, y, w = 100, h = 100, rest = {}) =>
    apiCreateElement(up, { type, id, x, y, width: w, height: h, ...rest });
  return [
    [
      "parity",
      [el(up, "rectangle", "r", 100, 100), el(up, "rectangle", "keep", 400, 100)],
      [
        { points: [[80, 150], [115, 150], [150, 150], [185, 150], [220, 150]] },
        { points: [[80, 150], [220, 150]] },
        { points: [[380, 50], [380, 90]] },
      ],
    ],
    [
      "outline",
      [
        transparent("rectangle", "hollow", 100, 100),
        transparent("ellipse", "ring", 300, 100),
        transparent("diamond", "kite", 500, 100),
      ],
      [
        { points: [[130, 130], [170, 170]] },
        { points: [[90, 150], [130, 150]] },
        { points: [[350, 130], [350, 170]] },
        { points: [[350, 90], [350, 110]] },
        { points: [[550, 150], [560, 150]] },
        { points: [[490, 150], [520, 150]] },
      ],
    ],
    [
      "labels",
      [
        ...labelled(up, "box", 100, 100),
        transparent("rectangle", "g1", 400, 100, 100, 100, { groupIds: ["grp"] }),
        transparent("rectangle", "g2", 600, 100, 100, 100, { groupIds: ["grp"] }),
        transparent("rectangle", "locked", 100, 400, 100, 100, { locked: true }),
      ],
      [
        { points: [[200, 90], [200, 130]] },
        { points: [[190, 145], [230, 155]] },
        { points: [[390, 150], [420, 150]] },
        { points: [[90, 450], [130, 450]] },
        { points: [[90, 150], [130, 150], [90, 150]], restore: [false, false, true] },
      ],
    ],
    [
      "linear",
      [
        arrow(up, "arr", 100, 100, [
          [0, 0],
          [200, 100],
        ]),
        apiCreateElement(up, {
          type: "line",
          id: "ln",
          x: 100,
          y: 300,
          width: 200,
          height: 0,
          points: [
            [0, 0],
            [200, 0],
          ],
        }),
        apiCreateElement(up, {
          type: "freedraw",
          id: "fd",
          x: 400,
          y: 100,
          width: 100,
          height: 100,
          points: [
            [0, 0],
            [50, 60],
            [100, 100],
          ],
        }),
      ],
      [
        { points: [[200, 140], [200, 160]] },
        { points: [[150, 200], [160, 210]] },
        { points: [[200, 290], [200, 310]] },
        { points: [[440, 150], [460, 150]] },
        { points: [[600, 150], [620, 150]] },
      ],
    ],
  ];
};

const eraserCases = () =>
  ["parity", "outline", "labels", "linear"].map((name) => ({
    id: `eraser-${name}`,
    build: (up) => {
      const [, elements, paths] = ERASER_SCENES(up).find(([n]) => n === name);
      const map = up.arrayToMap(elements);
      const app = {
        state: { theme: "light", zoom: { value: 1 }, scrollX: 0, scrollY: 0 },
        visibleElements: elements.filter((e) => !e.isDeleted),
        scene: { getNonDeletedElementsMap: () => map },
      };
      const recorded = paths.map(({ points, restore = [] }) => {
        const trail = new up.EraserTrail(app);
        trail.startPath(points[0][0], points[0][1]);
        const steps = points
          .slice(1)
          .map((p, i) => trail.addPointToPath(p[0], p[1], restore[i + 1] ?? false));
        trail.endPath();
        return { points, restore: points.map((_, i) => restore[i] ?? false), steps };
      });
      return { id: `eraser-${name}`, kind: "eraser", elements: clone(elements), paths: recorded };
    },
  }));

const buildCases = () => [
  ...withinCases(),
  ...groupCases(),
  ...newCases(),
  ...dragCases(),
  ...perfectCases(),
  ...eraserCases(),
];

// -- the fixture --------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating editing fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const asciiJson = (fixture) =>
  format(fixture).replace(
    /[\u0080-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );

const buildFixture = (up, commit) => {
  const ids = new Set();
  const cases = [];
  for (const c of buildCases()) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
    up.reseed(1);
    const recorded = c.build(up);
    if (recorded.id !== c.id) throw new Error(`case ${c.id} recorded as ${recorded.id}`);
    cases.push(recorded);
  }
  return asciiJson({
    description:
      "getElementsWithinSelection, selectGroupsForSelectedElements, the new elements App creates, " +
      "dragNewElement, getPerfectElementSize and EraserTrail (packages/element/src/selection.ts, " +
      "groups.ts, newElement.ts, dragElements.ts, sizeHelpers.ts, packages/excalidraw/eraser) " +
      "in upstream's test mode (ids id0.., " +
      "timestamps 1, reseed(1) before each case). Generated by tools/goldens/editing-fixtures.mjs.",
    upstream: commit,
    cases,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`editing-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  // the eraser trail creates its SVG path in the document; its animation
  // frames (which only draw the trail) never run
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>");
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.requestAnimationFrame = () => 0;
  globalThis.cancelAnimationFrame = () => {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("editing fixture is out of date: run node tools/goldens/editing-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`editing fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
