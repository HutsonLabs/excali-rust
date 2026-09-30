#!/usr/bin/env node
// Shape recognition fixtures for excali-editor (ex-706): upstream's own
// recognizeShape and convertToShape (packages/element/src/convertToShape.ts,
// built on packages/math/src/pca.ts), run from the pinned checkout under
// plain Node on the strokes of upstream's recognizeShape.test.ts and on
// seeded random strokes.
//
//   node tools/goldens/shape-recognition-fixtures.mjs            write the fixture
//   node tools/goldens/shape-recognition-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/shape-recognition-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/shape-recognition.json:
//
//   { "description", "upstream", "recognize": [ { id, points, previous,
//     zoom, features, boundingBox, type } ], "convert": [ { id, points,
//     previous, appState, frames, element } ] }
//
// - recognize: one call of recognizeShape(points, previous, zoom). previous
//   is the previous element's type (null for none; upstream reads only its
//   type), zoom null when the call left it at its default (1). features is
//   what the module-private extractFeatures returned for the stroke (null
//   when the size gate returned freedraw before extracting), recorded by
//   wrapping its one call site at build time; boundingBox and type are the
//   result's.
// - convert: one call of convertToShape(points, appState, elementsMap,
//   previous) with elementsMap the frames given; appState holds the keys
//   convertToShape reads (the rest are getDefaultAppState()'s), element is
//   the element it returned (null for undefined).
//
// Cases:
// - upstream-*: every recognizeShape call of every test in
//   packages/element/tests/recognizeShape.test.ts, the test file itself
//   bundled and run with a minimal describe / it / expect (its assertions
//   hold, or generation fails), numbered in call order.
// - random-*: seeded strokes (closed polygons and ellipses, open polylines,
//   arrows, scribbles), rotated, scaled and jittered, at several zooms,
//   some after an arrow.
// - convert-*: convertToShape on a selection of those strokes under app
//   states that vary every key it reads, with and without enclosing frames.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each convert case, and
// random strokes come from a Park-Miller generator seeded per case.
// Math.random throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "shape-recognition.json";

const ENTRY = `
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "./packages/common/src/index";
export { convertToShape, recognizeShape } from "./packages/element/src/convertToShape";
export { newFrameElement, newMagicFrameElement } from "./packages/element/src/newElement";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export const runShapeTests = async () => {
  await import("./packages/element/tests/recognizeShape.test.ts");
};
`;

// Every recognizeShape call, the test's and convertToShape's, goes through
// the recorder, and extractFeatures' result is handed to it on the way to
// classify; the functions themselves are upstream's, unchanged.
const PATCH = {
  "packages/element/src/convertToShape": (source) => {
    const from = "export const recognizeShape = ";
    const call = "classify(extractFeatures(points))";
    if (!source.includes(from)) throw new Error("convertToShape: recognizeShape not found");
    if (!source.includes(call)) throw new Error("convertToShape: classify(extractFeatures(points)) not found");
    return (
      source
        .replace(from, "const upstreamRecognizeShape = ")
        .replace(call, "classify(globalThis.__shape.features(extractFeatures(points)))") +
      "\nexport const recognizeShape = (points, previousElement, zoom) =>\n" +
      "  globalThis.__shape.recognize(points, previousElement, zoom, upstreamRecognizeShape);\n"
    );
  },
};

const usage = () => {
  process.stderr.write("usage: shape-recognition-fixtures.mjs [--check] [--out DIR]\n");
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
    range: (lo, hi) => lo + next() * (hi - lo),
  };
};

// -- the recorder ---------------------------------------------------------------------

const createRecorder = () => {
  const recorder = {
    calls: [],
    pendingFeatures: null,
    features: (features) => {
      recorder.pendingFeatures = features;
      return features;
    },
    recognize: (points, previousElement, zoom, upstream) => {
      recorder.pendingFeatures = null;
      const result = upstream(points, previousElement, zoom);
      recorder.calls.push({
        points: clone(points),
        previous: previousElement?.type ?? null,
        zoom: zoom === undefined ? null : zoom,
        features: clone(recorder.pendingFeatures),
        boundingBox: clone(result.boundingBox),
        type: result.type,
      });
      return result;
    },
  };
  return recorder;
};

// -- a minimal describe / it / expect -----------------------------------------------

const deepEqual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const expectFn = (actual) => {
  const check = (pass, what) => {
    if (!pass) throw new Error(`expect(${JSON.stringify(actual)}).${what} failed`);
  };
  return {
    toBe: (v) => check(Object.is(actual, v), `toBe(${JSON.stringify(v)})`),
    toEqual: (v) => check(deepEqual(actual, v), `toEqual(${JSON.stringify(v)})`),
  };
};

const collectTests = async (up) => {
  const root = { name: "", children: [] };
  let current = root;
  globalThis.describe = (name, fn) => {
    const node = { name, children: [] };
    current.children.push(node);
    const outer = current;
    current = node;
    fn();
    current = outer;
  };
  const it = (name, fn) => current.children.push({ name, fn });
  it.each = (table) => (name, fn) => {
    for (const row of table) it(name.replace("%s", row[0]), () => fn(...row));
  };
  globalThis.it = it;
  globalThis.expect = expectFn;
  await up.runShapeTests();
  const tests = [];
  const walk = (node, path) => {
    for (const child of node.children) {
      if (child.fn) tests.push({ path: [...path, child.name], fn: child.fn });
      else walk(child, [...path, child.name]);
    }
  };
  walk(root, []);
  return tests;
};

const slug = (text) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

// -- random strokes -------------------------------------------------------------------

const RANDOM_CASES = 240;

const edge = (from, to, steps) =>
  Array.from({ length: steps }, (_, i) => [
    from[0] + ((to[0] - from[0]) * i) / steps,
    from[1] + ((to[1] - from[1]) * i) / steps,
  ]);

const closedPolygon = (r, corners) => {
  const pts = corners.flatMap((c, i) => edge(c, corners[(i + 1) % corners.length], r.int(4, 24)));
  // closing: exact, short of the start, or past it
  const close = r.pick(["exact", "short", "over"]);
  if (close === "exact") return [...pts, corners[0]];
  if (close === "short") return pts.slice(0, pts.length - r.int(1, 6));
  return [...pts, ...edge(corners[0], corners[1], r.int(2, 8))];
};

const STROKES = {
  rectangle: (r) => {
    const w = r.range(20, 400);
    const h = r.range(20, 400);
    const taper = r.chance(0.3) ? r.range(0.5, 1) : 1;
    return closedPolygon(r, [
      [-w / 2, -h / 2],
      [w / 2, -h / 2],
      [(w / 2) * taper, h / 2],
      [(-w / 2) * taper, h / 2],
    ]);
  },
  diamond: (r) => {
    const w = r.range(20, 400);
    const h = r.range(20, 400);
    return closedPolygon(r, [
      [0, -h / 2],
      [w / 2, 0],
      [0, h / 2],
      [-w / 2, 0],
    ]);
  },
  triangle: (r) => {
    const s = r.range(40, 300);
    return closedPolygon(r, [
      [0, -s / 2],
      [s / 2, s / 2],
      [-s / 2, s / 2],
    ]);
  },
  ellipse: (r) => {
    const rx = r.range(10, 250);
    const ry = r.range(10, 250);
    const step = r.range(0.05, 0.3);
    const end = Math.PI * 2 + r.range(-0.6, 0.6);
    const pts = [];
    for (let a = 0; a <= end; a += step) pts.push([Math.cos(a) * rx, Math.sin(a) * ry]);
    return pts;
  },
  line: (r) => {
    const len = r.range(5, 600);
    const bow = r.chance(0.5) ? r.range(0, 60) : 0;
    const n = r.int(2, 60);
    return Array.from({ length: n }, (_, i) => {
      const t = n === 1 ? 0 : i / (n - 1);
      return [t * len, Math.sin(t * Math.PI) * bow];
    });
  },
  arrow: (r) => {
    const len = r.range(30, 500);
    const head = len * r.range(0.05, 0.5);
    const spread = r.range(Math.PI / 10, Math.PI / 4);
    const tip = [len, 0];
    const arm = (side) => [len - head * Math.cos(spread), -side * head * Math.sin(spread)];
    return [
      ...edge([0, 0], tip, r.int(4, 40)),
      ...edge(tip, arm(1), r.int(2, 12)),
      ...edge(arm(1), tip, r.int(2, 12)),
      ...edge(tip, arm(-1), r.int(2, 12)),
    ];
  },
  elbow: (r) => {
    const a = r.range(40, 300);
    const b = r.range(40, 300);
    return [...edge([0, 0], [a, 0], r.int(5, 30)), ...edge([a, 0], [a, b], r.int(5, 30)), [a, b]];
  },
  scribble: (r) => {
    const n = r.int(3, 120);
    const pts = [[0, 0]];
    for (let i = 1; i < n; i++) {
      const [x, y] = pts[i - 1];
      pts.push([x + r.range(-40, 40), y + r.range(-40, 40)]);
    }
    return pts;
  },
  tiny: (r) => Array.from({ length: r.int(1, 8) }, () => [r.range(-15, 15), r.range(-15, 15)]),
};

const randomStroke = (seed) => {
  // a Park-Miller stream starts near 0 for a small seed: spread the seeds
  const r = rng((seed * 2654435761) % 2147483647);
  const kind = r.pick(Object.keys(STROKES));
  const raw = STROKES[kind](r);
  const angle = r.chance(0.3) ? r.pick([0, Math.PI / 2, Math.PI]) : r.range(0, Math.PI * 2);
  const scale = r.pick([0.25, 0.5, 1, 1, 2, 5]);
  const noise = r.pick([0, 0, 1, 3, 8]);
  const [ox, oy] = [r.range(-2000, 2000), r.range(-2000, 2000)];
  const points = raw.map(([x, y]) => {
    const nx = x * scale + r.range(-0.5, 0.5) * noise;
    const ny = y * scale + r.range(-0.5, 0.5) * noise;
    return [nx * Math.cos(angle) - ny * Math.sin(angle) + ox, nx * Math.sin(angle) + ny * Math.cos(angle) + oy];
  });
  const zoom = r.pick([undefined, undefined, 0.1, 0.5, 1, 2, 10]);
  const previous = r.chance(0.2) ? { type: r.pick(["arrow", "rectangle", "line"]) } : null;
  return { kind, points, zoom, previous };
};

// -- convertToShape -------------------------------------------------------------------

const APP_STATES = [
  {},
  {
    currentItemRoundness: "sharp",
    currentItemStrokeWidthKey: "bold",
    currentItemStrokeColor: "#e03131",
    currentItemBackgroundColor: "#ffc9c9",
    currentItemFillStyle: "hachure",
    currentItemStrokeStyle: "dashed",
    currentItemRoughness: 2,
    currentItemOpacity: 60,
    currentItemStartArrowhead: "bar",
    currentItemEndArrowhead: null,
  },
  {
    currentItemRoundness: "round",
    currentItemStrokeWidthKey: "thin",
    currentItemFillStyle: "cross-hatch",
    currentItemStrokeStyle: "dotted",
    currentItemRoughness: 0,
    currentItemStartArrowhead: null,
    currentItemEndArrowhead: "triangle",
    zoom: { value: 0.1 },
  },
];

const APP_STATE_KEYS = [
  "zoom",
  "currentItemRoundness",
  "currentItemStrokeWidthKey",
  "currentItemStrokeColor",
  "currentItemBackgroundColor",
  "currentItemFillStyle",
  "currentItemStrokeStyle",
  "currentItemRoughness",
  "currentItemOpacity",
  "currentItemStartArrowhead",
  "currentItemEndArrowhead",
];

const boundsOf = (points) => {
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
};

/** Frames around (or beside) the stroke: none, an enclosing frame, or a
 * miss and an enclosing magic frame. */
const framesFor = (up, points, variant) => {
  const [x1, y1, x2, y2] = boundsOf(points);
  if (variant === 0) return [];
  if (variant === 1) {
    return [up.newFrameElement({ x: x1 - 10, y: y1 - 10, width: x2 - x1 + 20, height: y2 - y1 + 20 })];
  }
  return [
    up.newFrameElement({ x: x1 + 1, y: y1 - 10, width: x2 - x1 + 20, height: y2 - y1 + 20 }),
    up.newMagicFrameElement({ x: x1, y: y1, width: x2 - x1, height: y2 - y1 }),
  ];
};

const runConvertCase = (up, id, points, previous, variant) => {
  up.reseed(1);
  const overrides = APP_STATES[variant % APP_STATES.length];
  const appState = { ...up.getDefaultAppState(), ...overrides };
  const frames = framesFor(up, points, variant % 3);
  const element = up.convertToShape(points, appState, up.arrayToMap(frames), previous);
  return {
    id,
    points: clone(points),
    previous: previous?.type ?? null,
    appState: Object.fromEntries(APP_STATE_KEYS.map((k) => [k, clone(appState[k])])),
    frames: clone(frames),
    element: clone(element),
  };
};

// -- the fixture ----------------------------------------------------------------------

const buildFixture = async (up, commit) => {
  const recorder = createRecorder();
  globalThis.__shape = recorder;

  const recognize = [];
  const ids = new Set();
  const push = (list, c) => {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
    list.push(c);
  };

  for (const { path, fn } of await collectTests(up)) {
    recorder.calls = [];
    await fn();
    if (!recorder.calls.length) throw new Error(`upstream test ${path.join(" > ")} recognized nothing`);
    const name = slug(path.join(" "));
    recorder.calls.forEach((call, i) => push(recognize, { id: `upstream-${name}-${i}`, ...call }));
  }

  const strokes = [];
  for (let seed = 1; seed <= RANDOM_CASES; seed++) {
    const stroke = randomStroke(seed);
    recorder.calls = [];
    up.recognizeShape(stroke.points, stroke.previous, stroke.zoom);
    push(recognize, { id: `random-${seed}-${stroke.kind}`, ...recorder.calls[0] });
    strokes.push({ ...stroke, type: recorder.calls[0].type });
  }

  // convertToShape on the first 6 random strokes of each recognized type
  // (the arrows include short ones, convertToShape's < 60 line branch)
  const convert = [];
  const byType = new Map();
  strokes.forEach((s, i) => {
    const list = byType.get(s.type) ?? [];
    list.push(i);
    byType.set(s.type, list);
  });
  const chosen = [...byType.values()].flatMap((list) => list.slice(0, 6)).sort((a, b) => a - b);
  for (const i of chosen) {
    for (let variant = 0; variant < 3; variant++) {
      const s = strokes[i];
      push(convert, runConvertCase(up, `convert-${i + 1}-${s.type}-${variant}`, s.points, s.previous, variant));
    }
  }

  return format({
    description:
      "recognizeShape and convertToShape (packages/element/src/convertToShape.ts, on packages/math/src/pca.ts) on " +
      "recognizeShape.test.ts's strokes and seeded random strokes, in upstream's test mode (ids id0.., timestamps " +
      "1, reseed(1) before each convert case). Generated by tools/goldens/shape-recognition-fixtures.mjs.",
    upstream: commit,
    recognize,
    convert,
  });
};

/** Runs fn with Math.random disabled (see header). */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating shape recognition fixtures");
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
    process.stderr.write(`shape-recognition-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  // appState.ts reads devicePixelRatio at load.
  globalThis.devicePixelRatio = 1;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    patch: PATCH,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = await deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write(
        "shape recognition fixture is out of date: run node tools/goldens/shape-recognition-fixtures.mjs\n",
      );
      process.exit(1);
    }
    process.stdout.write(`shape recognition fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
