#!/usr/bin/env node
// Frame clipping goldens for excali-scene (ex-403): upstream's own frame
// geometry (packages/element/src/frame.ts, and getElementLineSegments,
// packages/element/src/bounds.ts:299-420) run from the pinned checkout
// under Node on the frame scenes of static-scene.mjs.
//
//   node tools/goldens/frame-clip.mjs            write the fixture
//   node tools/goldens/frame-clip.mjs --check    exit 1 if it is stale
//   node tools/goldens/frame-clip.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/frame-clip.json: per scene,
// the elements and the app state, and per element:
//
// - `segments`: getElementLineSegments(element, elementsMap), the outline
//   the frame tests intersect;
// - `bounds`: getElementBounds(element, elementsMap);
// - `targetFrame`: the id of getTargetFrame(element, elementsMap, appState);
// - for each frame-like element of the scene (`frames`, by id):
//   isElementIntersectingFrame, isElementContainingFrame,
//   elementsAreInFrameBounds([element]), elementOverlapsWithFrame,
//   isElementInFrame (with that frame as the target, a fresh group cache)
//   and shouldApplyFrameClip (a fresh group cache).
//
// The static scene fixture (static-scene.mjs) holds what these add up to
// when upstream draws: which elements are clipped, in order, with the
// group cache shared across the scene.
//
// Deterministic: Math.random throws while generating, every element is
// built with a fixed id and seed after reseed(), and text measures 10 px
// per UTF-16 code unit (setCustomTextMetricsProvider).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { DRAG_SELECTION, frameClip, frameDrag, RANDOM_SEED } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const OUT_FILE = "frame-clip.json";

const ENTRY = `
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export {
  elementOverlapsWithFrame,
  elementsAreInFrameBounds,
  getTargetFrame,
  isElementContainingFrame,
  isElementInFrame,
  isElementIntersectingFrame,
  shouldApplyFrameClip,
} from "./packages/element/src/frame";
export { getElementBounds, getElementLineSegments } from "./packages/element/src/bounds";
export { isFrameLikeElement } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "@excalidraw/common";
`;

const STUBS = [
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

const usage = () => {
  process.stderr.write("usage: frame-clip.mjs [--check] [--out DIR]\n");
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

/** What the frame functions read of the app state (StaticCanvasAppState). */
const APP_STATE = {
  frameToHighlight: null,
  selectedElementIds: {},
  frameRendering: { enabled: true, clip: true, name: true, outline: true },
  selectedElementsAreBeingDragged: false,
  editingGroupId: null,
};

const scenes = (up) => {
  const clip = frameClip(up);
  const drag = frameDrag(up);
  const highlight = drag[0];
  return [
    { name: "frame-clip", elements: clip, appState: APP_STATE },
    { name: "frame-clip-off", elements: clip, appState: { ...APP_STATE, frameRendering: { ...APP_STATE.frameRendering, clip: false } } },
    { name: "frame-drag", elements: drag, appState: { ...APP_STATE, selectedElementIds: DRAG_SELECTION, selectedElementsAreBeingDragged: true, frameToHighlight: highlight } },
    {
      name: "frame-drag-editing-group",
      elements: drag,
      appState: { ...APP_STATE, selectedElementIds: DRAG_SELECTION, selectedElementsAreBeingDragged: true, frameToHighlight: highlight, editingGroupId: "fd-far" },
    },
    // editing a group while dragging with no frame highlighted: the
    // selection leaves the group before the overlap test
    {
      name: "frame-drag-editing-group-no-highlight",
      elements: drag,
      appState: { ...APP_STATE, selectedElementIds: DRAG_SELECTION, selectedElementsAreBeingDragged: true, editingGroupId: "fd-group" },
    },
    { name: "frame-selected", elements: drag, appState: { ...APP_STATE, selectedElementIds: DRAG_SELECTION, frameToHighlight: highlight } },
  ];
};

const plain = (value) => JSON.parse(JSON.stringify(value));

const points = (segments) => segments.map(([a, b]) => [[a[0], a[1]], [b[0], b[1]]]);

const run = (up, s) => {
  const elements = plain(s.elements);
  const appState = plain(s.appState);
  const map = up.arrayToMap(elements);
  const frames = elements.filter((e) => up.isFrameLikeElement(e));
  const results = elements.map((element) => {
    const target = up.getTargetFrame(element, map, appState);
    const perFrame = {};
    for (const frame of frames) {
      perFrame[frame.id] = {
        intersecting: up.isElementIntersectingFrame(element, frame, map),
        containing: up.isElementContainingFrame(element, frame, map),
        inBounds: up.elementsAreInFrameBounds([element], frame, map),
        overlaps: up.elementOverlapsWithFrame(element, frame, map),
        inFrame: up.isElementInFrame(element, map, appState, { targetFrame: frame, checkedGroups: new Map() }),
        shouldClip: up.shouldApplyFrameClip(element, frame, appState, map, new Map()),
      };
    }
    return {
      id: element.id,
      segments: points(up.getElementLineSegments(element, map)),
      bounds: up.getElementBounds(element, map),
      targetFrame: target ? target.id : null,
      frames: perFrame,
    };
  });
  return { name: s.name, elements, appState, results };
};

const build = async (upstream) => {
  const load = () =>
    loadUpstream(upstream, {
      entry: ENTRY,
      stubs: STUBS,
      define: {
        "import.meta.env.MODE": '"test"',
        "import.meta.env.PKG_NAME": "undefined",
        "import.meta.env.PKG_VERSION": "undefined",
      },
    });
  const fresh = async () => {
    const up = await load();
    up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
    up.reseed(RANDOM_SEED);
    return up;
  };
  const out = [];
  const names = scenes(await fresh()).map((s) => s.name);
  for (const name of names) {
    const up = await fresh();
    out.push(run(up, scenes(up).find((x) => x.name === name)));
  }
  return format({
    description:
      "Upstream frame geometry (packages/element/src/frame.ts, getElementLineSegments and getElementBounds in bounds.ts) at the pinned commit on the frame scenes of static-scene.mjs (tools/goldens/frame-clip.mjs): per element its outline segments, bounds and target frame, and per frame-like element of the scene the intersection, containment, in-bounds, overlap, in-frame and clip decisions.",
    upstream: upstream.commit,
    scenes: out,
  });
};

/** Runs fn with Math.random disabled. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating frame-clip goldens");
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
    process.stderr.write(`frame-clip: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("frame-clip goldens are out of date: run node tools/goldens/frame-clip.mjs\n");
      process.exit(1);
    }
    process.stdout.write("frame-clip goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
