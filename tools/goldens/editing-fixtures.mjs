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
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

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

const buildCases = () => [...withinCases(), ...groupCases()];

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
      "getElementsWithinSelection and selectGroupsForSelectedElements " +
      "(packages/element/src/selection.ts, groups.ts) in upstream's test mode (ids id0.., " +
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
  globalThis.window ??= {};
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
