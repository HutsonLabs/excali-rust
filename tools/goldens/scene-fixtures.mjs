#!/usr/bin/env node
// Scene fixtures for the typed `.excalidraw` codec (excali-core ex-102):
// upstream's own element constructors, mutateElement and serializeAsJSON,
// run from the pinned checkout under plain Node.
//
//   node tools/goldens/scene-fixtures.mjs            write the fixtures
//   node tools/goldens/scene-fixtures.mjs --check    exit 1 if they are stale
//   node tools/goldens/scene-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/:
//
// - every-type.excalidraw: one element of every persisted type built by
//   upstream's constructors (packages/element/src/newElement.ts) with
//   explicit ids and seeds, saved by serializeAsJSON(elements,
//   getDefaultAppState(), files, "local") (packages/excalidraw/data/json.ts);
// - unknown-keys.excalidraw: JSON.stringify(data, null, 2) of a scene with
//   unknown keys at every level, known keys out of canonical order and
//   values the typed model normalises;
// - unknown-keys-edited.excalidraw: that file parsed, edited with
//   mutateElement (packages/element/src/mutateElement.ts) and plain property
//   assignment, and stringified again.
//
// Deterministic: Date.now is fixed at TIMESTAMP before upstream's modules
// load (so `updated`, `created` and the module-level Random seeded from
// Date.now in packages/common/src/random.ts are fixed), upstream's own
// reseed(RANDOM_SEED) runs before each fixture (versionNonce draws), and
// Math.random throws while generating. Text is measured by a metrics provider
// (setCustomTextMetricsProvider, textMeasurements.ts) that gives 10 px per
// character, since Node has no canvas; that only affects text `width`.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const TIMESTAMP = 1700000000000;
export const RANDOM_SEED = TIMESTAMP;
export const EXPORT_SOURCE = "https://excalidraw.com";
export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");

const ENTRY = `
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { mutateElement } from "./packages/element/src/mutateElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { serializeAsJSON } from "./packages/excalidraw/data/json";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

// data/json.ts imports the browser file dialogs (./filesystem) and blob
// loading (./blob) for loadFromJSON/saveAsJSON; serializeAsJSON uses neither.
const STUBS = ["packages/excalidraw/data/blob", "packages/excalidraw/data/filesystem"];

const usage = () => {
  process.stderr.write("usage: scene-fixtures.mjs [--check] [--out DIR]\n");
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

// -- fixtures -------------------------------------------------------------------

const everyType = (up) => {
  up.reseed(RANDOM_SEED);
  const elements = [
    up.newElement({
      type: "rectangle",
      id: "rect",
      x: 0,
      y: 0,
      width: 100,
      height: 50,
      seed: 1,
      customData: { note: "kept" },
    }),
    up.newElement({ type: "diamond", id: "diamond", x: 10, y: 10, width: 40, height: 40, seed: 2 }),
    up.newElement({
      type: "ellipse",
      id: "ellipse",
      x: 20,
      y: 20,
      width: 30,
      height: 20,
      angle: 0.5,
      seed: 3,
    }),
    up.newEmbeddableElement({
      type: "embeddable",
      id: "embed",
      x: 0,
      y: 100,
      seed: 4,
      link: "https://example.com",
    }),
    up.newIframeElement({ type: "iframe", id: "iframe", x: 0, y: 200, seed: 5 }),
    // normalizeStickyNoteStyle sets the default sticky background through
    // newElementWith, which bumps `version` and draws a `versionNonce`.
    up.newStickyNoteElement({
      type: "stickynote",
      id: "sticky",
      x: 300,
      y: 0,
      width: 200,
      height: 200,
      seed: 6,
    }),
    up.newFrameElement({ id: "frame", x: -100, y: -100, seed: 7, name: "Frame A" }),
    up.newMagicFrameElement({ id: "magic", x: -200, y: -200, seed: 8 }),
    up.newTextElement({ id: "text", x: 5, y: 5, seed: 9, text: "Hello" }),
    up.newFreeDrawElement({
      type: "freedraw",
      id: "draw",
      x: 1,
      y: 2,
      seed: 10,
      points: [
        [0, 0],
        [1.5, 2.25],
      ],
      simulatePressure: true,
    }),
    up.newLinearElement({
      type: "line",
      id: "line",
      x: 3,
      y: 4,
      seed: 11,
      points: [
        [0, 0],
        [10, 0],
      ],
    }),
    up.newArrowElement({
      type: "arrow",
      id: "arrow",
      x: 5,
      y: 6,
      seed: 12,
      points: [
        [0, 0],
        [20, 5],
      ],
      endArrowhead: "arrow",
      elbowed: false,
    }),
    up.newArrowElement({
      type: "arrow",
      id: "elbow",
      x: 7,
      y: 8,
      seed: 13,
      points: [
        [0, 0],
        [0, 30],
        [40, 30],
      ],
      elbowed: true,
    }),
    up.newImageElement({
      type: "image",
      id: "image",
      x: 9,
      y: 10,
      width: 64,
      height: 64,
      seed: 14,
      fileId: "file1",
    }),
  ];
  const files = {
    file1: {
      mimeType: "image/png",
      id: "file1",
      dataURL: "data:image/png;base64,iVBORw0KGgo=",
      created: TIMESTAMP,
      lastRetrieved: TIMESTAMP,
    },
  };
  return up.serializeAsJSON(elements, up.getDefaultAppState(), files, "local");
};

/** The base keys of an element in _newElementBase's order, with `rest` last. */
const element = (id, type, seed, rest = {}) => ({
  id,
  type,
  x: 10,
  y: 20,
  width: 100,
  height: 50,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  index: "a0",
  roundness: null,
  seed,
  version: 3,
  versionNonce: 42,
  isDeleted: false,
  boundElements: null,
  updated: TIMESTAMP,
  created: TIMESTAMP,
  link: null,
  locked: false,
  ...rest,
});

/** Inserts `entries` into `object` right before `key`, keeping the order. */
const insertBefore = (object, key, entries) =>
  Object.fromEntries(
    Object.entries(object).flatMap((e) => (e[0] === key ? [...Object.entries(entries), e] : [e])),
  );

const unknownKeysScene = () => {
  const rect = {
    futureFirst: { nested: [1, 2, { z: 1, a: 2 }] },
    ...insertBefore(element("r1", "rectangle", 1), "strokeColor", {
      futureMiddle: "between strokeColor and backgroundColor",
    }),
    futureLast: [true, null, 1.5e-7],
  };
  const text = element("t1", "text", 2, {
    text: "a\nb",
    fontSize: 20,
    baseFontSize: null,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    originalText: "a b",
    autoResize: true,
    lineHeight: 1.25,
    labelPosition: null,
    customData: null,
  });
  // Every key in reverse order.
  const ellipse = Object.fromEntries(
    Object.entries(element("e1", "ellipse", 3, {})).reverse(),
  );
  ellipse.roundness = { type: 2, value: null };
  const diamond = element("d1", "diamond", 4, {
    boundElements: [{ type: "arrow", id: "a1", futureBinding: 1 }],
    labelPosition: 0.5,
  });
  const arrow = element("a1", "arrow", 5, {
    points: [
      [0, 0],
      [100, 50],
    ],
    startBinding: null,
    endBinding: { elementId: "d1", fixedPoint: [0.5, 0.5], mode: "orbit", focus: 0, gap: 1 },
    startArrowhead: null,
    endArrowhead: "arrow",
    elbowed: false,
  });
  return {
    type: "excalidraw",
    version: 2,
    source: "https://future.example",
    futureTopLevel: { k: "v", 2: "b", 1: "a" },
    elements: [rect, text, ellipse, diamond, arrow],
    appState: { gridSize: 20, futureAppStateFlag: true, viewBackgroundColor: "#ffffff" },
    files: {
      f1: {
        mimeType: "image/png",
        id: "f1",
        dataURL: "data:image/png;base64,AA==",
        created: 1,
        futureFileKey: "x",
      },
    },
    pluginData: { b: 1, a: 2 },
  };
};

const unknownKeysEdited = (up, text) => {
  up.reseed(RANDOM_SEED);
  const data = JSON.parse(text);
  const elementsMap = new Map(data.elements.map((e) => [e.id, e]));
  const [rect, textElement] = data.elements;
  up.mutateElement(rect, elementsMap, { x: 99, customData: { added: true } });
  up.mutateElement(textElement, elementsMap, { text: "edited" });
  delete data.futureTopLevel;
  data.appState.futureAppStateFlag = false;
  return JSON.stringify(data, null, 2);
};

const buildFixtures = (up) => {
  const unknownKeys = JSON.stringify(unknownKeysScene(), null, 2);
  return new Map([
    ["every-type.excalidraw", everyType(up)],
    ["unknown-keys.excalidraw", unknownKeys],
    ["unknown-keys-edited.excalidraw", unknownKeysEdited(up, unknownKeys)],
  ]);
};

/** Runs fn with Math.random disabled (see header). */
const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating scene fixtures");
  };
  try {
    return fn();
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
    process.stderr.write(`scene-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  Date.now = () => TIMESTAMP;
  // appState.ts reads devicePixelRatio at load; getExportSource reads
  // window.EXCALIDRAW_EXPORT_SOURCE (packages/common/src/constants.ts:351).
  globalThis.devicePixelRatio = 1;
  globalThis.window = { EXCALIDRAW_EXPORT_SOURCE: EXPORT_SOURCE };
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    define: { "import.meta.env.MODE": '"production"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const out = deterministic(() => buildFixtures(up));
  const where = relative(process.cwd(), args.out) || ".";

  if (args.check) {
    const stale = [...out].filter(([name, text]) => {
      const path = join(args.out, name);
      return !existsSync(path) || readFileSync(path, "utf8") !== text;
    });
    if (stale.length) {
      for (const [name] of stale) process.stderr.write(`stale: ${join(where, name)}\n`);
      process.stderr.write("scene fixtures are out of date: run node tools/goldens/scene-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`scene fixtures up to date: ${out.size} files in ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  for (const [name, text] of out) writeFileSync(join(args.out, name), text);
  process.stdout.write(
    `wrote ${out.size} scene fixtures to ${where} from upstream ${upstream.commit.slice(0, 7)}\n`,
  );
};

await main();
