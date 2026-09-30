#!/usr/bin/env node
// Bucket fill fixtures for excali-editor (ex-704): upstream's own
// packages/element/src/bucketFill.ts, run from the pinned checkout under
// plain Node on the scenes of upstream's bucketFill tests and on seeded
// random scenes.
//
//   node tools/goldens/bucket-fill-fixtures.mjs            write the fixture
//   node tools/goldens/bucket-fill-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/bucket-fill-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/bucket-fill.json:
//
//   { "description", "upstream", "cases": [ { id, test?, calls } ] }
//
// Every call holds its inputs (the scene as `elements`, JSON as upstream
// holds it; the elements map is arrayToMap(elements)) and what upstream
// answered:
//
// - { fn: "computeBucketFillPolygon", point, options, elements, result }
// - { fn: "isRestylableFill", hitElement, scenePoints, elements, result }
//
// Cases:
// - upstream-*: packages/element/tests/bucketFill.test.ts itself, loaded
//   from the checkout with its two imports that need the app
//   (API.createElement, from tests/helpers/api.ts) or recording
//   (computeBucketFillPolygon, isRestylableFill) replaced, and run under a
//   minimal describe/it/expect/vi. Its assertions run as written, so the
//   generator fails if the harness does not reproduce the test; each `it`
//   is one case with every call it made.
// - edge-*: scenes for the failure reasons the others leave out
//   (open_region, invalid_polygon, too_small).
// - random-*: seeded scenes of two to seven closed and open outlines
//   (rectangles, diamonds, ellipses sharp and round, open, polygon and
//   curved lines, freedraw loops), some rotated, opaque, hachure,
//   translucent or strokeless, each clicked at twelve points
//   (every other one near an element's middle), and random-lines-*: grids of
//   four to six crossing open lines, for the owner-less fallback.
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
export const FIXTURE = "bucket-fill.json";

const TEST_MODULE = "packages/element/tests/bucketFill.test";

const ENTRY = `
export {
  computeBucketFillPolygon,
  isRestylableFill,
} from "./packages/element/src/bucketFill.ts";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_VERTICAL_ALIGN,
  ROUNDNESS,
  getSizeFromPoints,
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
import "./${TEST_MODULE}";
`;

// The test's imports the harness answers: API.createElement (the app's test
// helper) and the two functions under test, wrapped to record each call.
const IMPORT_API = `import { API } from "@excalidraw/excalidraw/tests/helpers/api";`;
const IMPORT_UNDER_TEST = `import { computeBucketFillPolygon, isRestylableFill } from "../src/bucketFill";`;

const patchTest = (source) => {
  for (const line of [IMPORT_API, IMPORT_UNDER_TEST]) {
    if (!source.includes(line)) throw new Error(`bucketFill.test.ts no longer has: ${line}`);
  }
  return source
    .replace(IMPORT_API, "const API = { createElement: (opts) => globalThis.__bucketFill.createElement(opts) };")
    .replace(
      IMPORT_UNDER_TEST,
      [
        `import { computeBucketFillPolygon as __compute, isRestylableFill as __restylable } from "../src/bucketFill";`,
        "const computeBucketFillPolygon = (args) => globalThis.__bucketFill.compute(__compute, args);",
        "const isRestylableFill = (args) => globalThis.__bucketFill.restylable(__restylable, args);",
      ].join("\n"),
    );
};

const usage = () => {
  process.stderr.write("usage: bucket-fill-fixtures.mjs [--check] [--out DIR]\n");
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

// -- a minimal vitest --------------------------------------------------------------

const tests = [];
const suites = [];

const describe = (name, fn) => {
  suites.push(name);
  try {
    fn();
  } finally {
    suites.pop();
  }
};

const it = (name, fn) => {
  tests.push({ name: [...suites, name].join(" > "), fn });
};

// it.each(rows)(name, fn): one test per row, `%s` in the name taking the
// row's first value (vitest's printf formatting, as the test uses it)
it.each = (rows) => (name, fn) => {
  for (const row of rows) {
    const values = Array.isArray(row) ? row : [row];
    let k = 0;
    it(name.replace(/%s/g, () => String(values[k++])), () => fn(...values));
  }
};

const deepEqual = (a, b) => {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (a instanceof Set || b instanceof Set) {
    return a instanceof Set && b instanceof Set && deepEqual([...a], [...b]);
  }
  // toEqual ignores properties whose value is undefined
  const keys = (o) => Object.keys(o).filter((k) => o[k] !== undefined);
  const ka = keys(a);
  const kb = keys(b);
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && deepEqual(a[k], b[k]));
};

const show = (v) => {
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
};

const expect = (actual) => {
  const matchers = (negated) => {
    const check = (pass, what, expected) => {
      if (pass === negated) {
        throw new Error(`expected ${show(actual)} ${negated ? "not " : ""}${what} ${show(expected)}`);
      }
    };
    return {
      toBe: (e) => check(Object.is(actual, e), "toBe", e),
      toEqual: (e) => check(deepEqual(actual, e), "toEqual", e),
      toBeNull: () => check(actual === null, "toBeNull", null),
      toBeGreaterThan: (e) => check(actual > e, "toBeGreaterThan", e),
      toBeLessThan: (e) => check(actual < e, "toBeLessThan", e),
      toBeLessThanOrEqual: (e) => check(actual <= e, "toBeLessThanOrEqual", e),
      toBeCloseTo: (e, digits = 2) =>
        check(Math.abs(actual - e) < 10 ** -digits / 2, `toBeCloseTo(${digits})`, e),
      toContain: (e) => check(actual.includes(e), "toContain", e),
    };
  };
  const positive = matchers(false);
  positive.not = matchers(true);
  return positive;
};

const vi = {
  spyOn: (object, name) => {
    const original = object[name];
    return {
      mockImplementation(fn) {
        object[name] = fn;
        return this;
      },
      mockRestore() {
        object[name] = original;
      },
    };
  },
};

// -- recording ----------------------------------------------------------------

let calls = null;

/** The elements of `elementsMap`, checked to be arrayToMap(elements). */
const sceneOf = (elements, elementsMap) => {
  if (elements) {
    const same = elementsMap.size === elements.length && elements.every((e) => elementsMap.get(e.id) === e);
    if (!same) throw new Error("elementsMap is not arrayToMap(elements)");
    return elements;
  }
  return [...elementsMap.values()];
};

const recorder = (up) => ({
  createElement: (opts) => apiCreateElement(up, opts),
  compute: (fn, args) => {
    const elements = sceneOf(args.elements, args.elementsMap);
    const result = fn(args);
    calls.push({
      fn: "computeBucketFillPolygon",
      point: clone(args.point),
      options: clone(args.options ?? null),
      elements: clone(elements),
      result: clone(result),
    });
    return result;
  },
  restylable: (fn, args) => {
    const result = fn(args);
    calls.push({
      fn: "isRestylableFill",
      hitElement: clone(args.hitElement),
      scenePoints: clone(args.scenePoints),
      elements: clone(sceneOf(null, args.elementsMap)),
      result,
    });
    return result;
  },
});

// -- seeded random scenes --------------------------------------------------------

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

const round = (v) => Math.round(v * 100) / 100;

const randomStyle = (r) => {
  const style = {};
  if (r.chance(0.35)) {
    style.backgroundColor = r.pick(["#ffc9c9", "#a5d8ff", "#b2f2bb80", "rgba(0, 0, 0, 0.5)", "#ffec99"]);
    style.fillStyle = r.pick(["solid", "solid", "solid", "hachure", "cross-hatch"]);
  }
  if (r.chance(0.1)) style.strokeColor = "transparent";
  if (r.chance(0.1)) style.opacity = r.pick([0, 50]);
  if (r.chance(0.3)) style.angle = round(r.range(0, 2 * Math.PI));
  style.strokeWidth = r.pick([1, 2, 4]);
  return style;
};

const randomElement = (up, r) => {
  const x = round(r.range(-20, 200));
  const y = round(r.range(-20, 200));
  const width = round(r.range(20, 240));
  const height = round(r.range(20, 240));
  const style = randomStyle(r);
  const kind = r.pick(["rectangle", "rectangle", "diamond", "ellipse", "line", "polygon", "curve", "freedraw"]);
  switch (kind) {
    case "rectangle":
    case "diamond":
    case "ellipse":
      return apiCreateElement(up, { type: kind, x, y, width, height, roundness: r.chance(0.5) ? null : undefined, ...style });
    case "line": {
      const n = r.int(2, 4);
      const points = [[0, 0]];
      for (let i = 1; i < n; i++) points.push([round(r.range(-150, 250)), round(r.range(-150, 250))]);
      return apiCreateElement(up, { type: "line", x, y, points, ...up.getSizeFromPoints(points), roundness: null, ...style });
    }
    case "polygon":
    case "curve": {
      const n = r.int(3, 6);
      const points = [[0, 0]];
      for (let i = 1; i < n; i++) points.push([round(r.range(-100, 200)), round(r.range(-100, 200))]);
      points.push([0, 0]);
      return apiCreateElement(up, {
        type: "line",
        x,
        y,
        points,
        ...up.getSizeFromPoints(points),
        polygon: kind === "polygon" || r.chance(0.5),
        roundness: kind === "curve" ? { type: up.ROUNDNESS.PROPORTIONAL_RADIUS } : null,
        ...style,
      });
    }
    case "freedraw": {
      const n = r.int(12, 40);
      const rx = width / 2;
      const ry = height / 2;
      const gap = r.pick([0, 0.02, 0.06]);
      const points = Array.from({ length: n + 1 }, (_, i) => {
        const t = (i / n) * (2 * Math.PI - gap);
        return [round(rx + rx * Math.cos(t) + r.range(-2, 2)), round(ry + ry * Math.sin(t) + r.range(-2, 2))];
      });
      const shifted = points.map(([px, py]) => [round(px - points[0][0]), round(py - points[0][1])]);
      return apiCreateElement(up, {
        type: "freedraw",
        x,
        y,
        points: shifted,
        ...up.getSizeFromPoints(shifted),
        strokeOptions: { variability: r.pick(["variable", "constant"]), streamline: r.pick([0.25, 0.5]) },
        ...style,
      });
    }
  }
  throw new Error(kind);
};

const RANDOM_SCENES = 48;

const randomCases = () =>
  Array.from({ length: RANDOM_SCENES }, (_, i) => ({
    id: `random-${String(i).padStart(2, "0")}`,
    run: (up, rec) => {
      const r = rng(7919 * (i + 1));
      const elements = Array.from({ length: r.int(2, 7) }, () => randomElement(up, r));
      const elementsMap = up.arrayToMap(elements);
      for (let k = 0; k < 12; k++) {
        // even clicks near an element's middle, odd ones anywhere
        const target = r.pick(elements);
        const point =
          k % 2 === 0
            ? [
                round(target.x + target.width * r.range(0.2, 0.8)),
                round(target.y + target.height * r.range(0.2, 0.8)),
              ]
            : [round(r.range(-40, 420)), round(r.range(-40, 420))];
        rec.compute(up.computeBucketFillPolygon, { point, elements, elementsMap });
      }
    },
  }));

/**
 * Seeded grids of two or three roughly horizontal and two or three roughly
 * vertical open lines (some with a bend, some curved) crossing each other,
 * so the owner-less fallback has regions to find.
 */
const LINE_SCENES = 16;

const lineCases = () =>
  Array.from({ length: LINE_SCENES }, (_, i) => ({
    id: `random-lines-${String(i).padStart(2, "0")}`,
    run: (up, rec) => {
      const r = rng(104729 * (i + 1));
      const line = (vertical) => {
        const at = round(r.range(20, 240));
        const points = [[0, 0]];
        if (r.chance(0.4)) points.push([round(r.range(100, 180)), round(r.range(-30, 30))]);
        points.push([round(r.range(270, 290)), round(r.range(-30, 30))]);
        const pts = vertical ? points.map(([px, py]) => [py, px]) : points;
        return apiCreateElement(up, {
          type: "line",
          x: vertical ? at : round(r.range(-20, -5)),
          y: vertical ? round(r.range(-20, -5)) : at,
          points: pts,
          ...up.getSizeFromPoints(pts),
          roundness: r.chance(0.25) ? { type: up.ROUNDNESS.PROPORTIONAL_RADIUS } : null,
          strokeWidth: r.pick([1, 2, 4]),
        });
      };
      const elements = [
        ...Array.from({ length: r.int(2, 3) }, () => line(false)),
        ...Array.from({ length: r.int(2, 3) }, () => line(true)),
      ];
      const elementsMap = up.arrayToMap(elements);
      for (let k = 0; k < 12; k++) {
        const point = [round(r.range(0, 260)), round(r.range(0, 260))];
        rec.compute(up.computeBucketFillPolygon, { point, elements, elementsMap });
      }
    },
  }));

/**
 * Scenes for the failure reasons the test and the random scenes leave out:
 * an owner whose whole outline is buried under an opaque fill-compatible
 * polygon above it (nothing bounds the click: open_region), sliver
 * polygons whose face passes the area check but collapses when simplified
 * (invalid_polygon), and bulged triangles whose simplified ring falls under
 * the minimum area (too_small).
 */
const edgeCases = () => {
  const paint = (up, x, y, points) =>
    apiCreateElement(up, {
      type: "line",
      x,
      y,
      points,
      ...up.getSizeFromPoints(points),
      polygon: true,
      roundness: null,
      backgroundColor: "#ffc9c9",
      fillStyle: "solid",
      strokeColor: "transparent",
    });
  const polygon = (up, points, x = 0, y = 0) =>
    apiCreateElement(up, {
      type: "line",
      x,
      y,
      points,
      ...up.getSizeFromPoints(points),
      polygon: true,
      roundness: null,
    });
  const clicks = (up, rec, elements, points) => {
    const elementsMap = up.arrayToMap(elements);
    for (const point of points) rec.compute(up.computeBucketFillPolygon, { point, elements, elementsMap });
  };
  return [
    {
      id: "edge-owner-buried-under-paint",
      run: (up, rec) => {
        const rect = apiCreateElement(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100, roundness: null });
        const cover = paint(up, -20, -20, [
          [0, 0],
          [140, 0],
          [140, 140],
          [0, 140],
          [0, 0],
        ]);
        clicks(up, rec, [rect, cover], [
          [50, 50],
          [5, 5],
        ]);
      },
    },
    {
      id: "edge-sliver-polygons",
      run: (up, rec) => {
        // a 24 px² sliver whose apex sits 0.6 px off its base, once per
        // starting vertex: simplifying drops the apex when the face ring
        // does not start on it
        const ring = [
          [0, 0],
          [40, 0.6],
          [80, 0],
        ];
        const elements = ring.map((_, start) => {
          const pts = [...ring.slice(start), ...ring.slice(0, start)];
          const points = [...pts, pts[0]].map(([x, y]) => [x - pts[0][0], y - pts[0][1]]);
          return polygon(up, points, pts[0][0], start * 20 + pts[0][1]);
        });
        clicks(
          up,
          rec,
          elements,
          elements.map((_, i) => [40, i * 20 + 0.2]),
        );
      },
    },
    {
      id: "edge-bulged-triangles",
      run: (up, rec) => {
        // a 3.6 px² triangle with a 0.7 px bulge on its long side (6.4 px²
        // in all), once per starting vertex: simplifying drops the bulge
        // when the face ring does not start on it
        const ring = [
          [0, 0],
          [8, 0],
          [4.09, 1.15],
          [0, 0.9],
        ];
        const elements = ring.map((_, start) => {
          const pts = [...ring.slice(start), ...ring.slice(0, start)];
          const points = [...pts, pts[0]].map(([x, y]) => [x - pts[0][0], y - pts[0][1]]);
          return apiCreateElement(up, {
            type: "line",
            x: pts[0][0],
            y: start * 20 + pts[0][1],
            points,
            ...up.getSizeFromPoints(points),
            polygon: true,
            roundness: null,
            strokeWidth: 1,
          });
        });
        clicks(
          up,
          rec,
          elements,
          elements.map((_, i) => [2, i * 20 + 0.3]),
        );
      },
    },
  ];
};

// -- output -------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  // getFreedrawFillPolygon (shape.ts:592-619), which isPointInElement
  // reaches for a freedraw loop, draws its curve with an unseeded
  // RoughGenerator at roughness 0: rough.js multiplies every draw by the
  // roughness, so the draws cannot reach the output. Anywhere else
  // Math.random throws.
  Math.random = () => {
    if (new Error().stack.includes("getFreedrawFillPolygon")) return 0.5;
    throw new Error("Math.random called while generating bucket fill fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

// Every non-ASCII code unit as a \u escape (see restore-fixtures.mjs).
const asciiJson = (fixture) =>
  format(fixture).replace(/[\u0080-￿]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`);

const slug = (name) =>
  name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

const buildFixture = (up, commit) => {
  const rec = recorder(up);
  globalThis.__bucketFill = rec;
  const cases = [];
  const ids = new Set();
  const run = (id, test, body) => {
    if (ids.has(id)) throw new Error(`duplicate case id ${id}`);
    ids.add(id);
    up.reseed(1);
    calls = [];
    body();
    if (!calls.length) throw new Error(`case ${id} made no calls`);
    cases.push({ id, ...(test ? { test } : {}), calls });
    calls = null;
  };
  for (const t of tests) {
    run(`upstream-${slug(t.name)}`, t.name, () => {
      try {
        t.fn();
      } catch (error) {
        throw new Error(`bucketFill.test.ts "${t.name}" failed under the harness: ${error.message}`);
      }
    });
  }
  for (const c of [...randomCases(), ...lineCases(), ...edgeCases()]) run(c.id, null, () => c.run(up, rec));
  return asciiJson({
    description:
      "computeBucketFillPolygon and isRestylableFill (packages/element/src/bucketFill.ts) on the scenes of " +
      "packages/element/tests/bucketFill.test.ts (run as written) and seeded random scenes, in upstream's test " +
      "mode (ids id0.., timestamps 1, reseed(1) before each case). Generated by tools/goldens/bucket-fill-fixtures.mjs.",
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
    process.stderr.write(`bucket-fill-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  Object.assign(globalThis, { describe, it, expect, vi });
  globalThis.__bucketFill = {
    createElement: () => {
      throw new Error("API.createElement called while loading");
    },
  };
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
    patch: { [TEST_MODULE]: patchTest },
  });
  // newTextElement measures its text; API.createElement then sets the
  // width and height it was given, so the metric never reaches a fixture
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  if (!tests.length) throw new Error("bucketFill.test.ts registered no tests");
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("bucket fill fixture is out of date: run node tools/goldens/bucket-fill-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`bucket fill fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
