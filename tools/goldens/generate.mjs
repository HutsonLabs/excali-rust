#!/usr/bin/env node
// Golden generator for the Rust rendering crates (excali-rough,
// excali-freehand, excali-scene).
//
//   node tools/goldens/generate.mjs              write goldens/*.json
//   node tools/goldens/generate.mjs --check      exit 1 if goldens/ is stale
//   node tools/goldens/generate.mjs --out DIR    write (or --check) DIR instead
//
// Runs headless under plain Node. Upstream's own TypeScript (the pinned
// checkout from scripts/upstream/checkout.sh) is bundled with esbuild and
// executed with the rendering packages pinned in package-lock.json
// (roughjs 4.6.4, perfect-freehand 1.2.0), so element goldens are the ops
// upstream's ShapeCache.generateElementShape produces, not a
// re-implementation. Output is byte-stable: fixed inputs, explicit seeds,
// Math.random disabled while generating, and a deterministic formatter.
//
// Setup: (cd tools/goldens && npm ci) and scripts/upstream/checkout.sh.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import {
  EASINGS,
  elementGoldens,
  freehandCases,
  RANDOM_SEEDS,
  roughGoldens,
} from "./fixtures.mjs";
import { fractionalIndexCases, orderKeyCases } from "./fixtures-fractional.mjs";
import { format } from "./lib/format.mjs";
import { jsSortCases, jsSortResult } from "./jssort.mjs";
import { mathCases } from "./math.mjs";
import { loadUpstream, readJson, REPO_ROOT, TOOL_DIR, verifyUpstream } from "./lib/upstream.mjs";

const PACKAGES = ["perfect-freehand", "points-on-curve", "roughjs", "tinycolor2"];
const RANDOM_COUNT = 64;

const usage = () => {
  process.stderr.write("usage: generate.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: join(REPO_ROOT, "goldens") };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// -- serialisation --------------------------------------------------------------

/** JSON round trip: drops undefined, normalises -0, rejects non-finite. */
const plain = (value, where) =>
  JSON.parse(
    JSON.stringify(value, (key, v) => {
      if (typeof v === "number" && !Number.isFinite(v)) {
        throw new Error(`${where}: non-finite number at ${key}`);
      }
      return v;
    }),
  );

/** A rough.js Drawable without its RNG state. */
const drawable = (d) => {
  const { randomizer, ...options } = d.options;
  return {
    shape: d.shape,
    options,
    sets: d.sets.map((set) => ({ ...set, ops: set.ops.map((o) => ({ op: o.op, data: o.data })) })),
  };
};

// -- goldens --------------------------------------------------------------------

const randomGolden = (up) => ({
  name: "random.json",
  description:
    "rough.js 4.6.4 Random.next() (bin/math.js): ((2**31 - 1) & (seed = Math.imul(48271, seed))) / 2**31; the first values per seed.",
  cases: RANDOM_SEEDS.map((seed) => {
    const rng = new up.Random(seed);
    return { id: `seed${seed}`, seed, values: Array.from({ length: RANDOM_COUNT }, () => rng.next()) };
  }),
});

const roughCase = (up) => (c) => {
  const generator = new up.RoughGenerator();
  // The pattern fillers rotate the polygon points in place (hachure-fill
  // rotatePoints), so every call gets its own copy of the arguments: shared
  // fixture arrays would otherwise drift from case to case, and the recorded
  // args would be the drifted values rather than the input.
  const result = generator[c.method](...structuredClone(c.args), { ...c.options });
  return { ...c, drawable: drawable(result) };
};

const toRenderConfig = (rc) => ({
  isExporting: rc.isExporting,
  canvasBackgroundColor: rc.canvasBackgroundColor,
  embedsValidationStatus: new Map(rc.validatedEmbeds.map((id) => [id, true])),
  theme: rc.theme,
});

const elementCase = (up) => (c) => {
  const element = structuredClone(c.element);
  const shape = up.ShapeCache.generateElementShape(element, toRenderConfig(c.renderConfig));
  const list = shape === null ? [] : Array.isArray(shape) ? shape : [shape];
  const shapes = list.map((s) =>
    typeof s === "string" ? { type: "svgPath", d: s } : { type: "rough", drawable: drawable(s) },
  );
  const out = { id: c.id, element: c.element, renderConfig: c.renderConfig };
  if (c.freehandCase) out.freehandCase = c.freehandCase;
  if (element.type === "freedraw") {
    out.outline = up.getFreedrawOutlinePoints(element);
  }
  out.shapes = shapes;
  return out;
};

const freehandCase = (up) => (c) => {
  const options = { ...c.options };
  if (typeof options.easing === "string") options.easing = EASINGS[options.easing];
  for (const end of ["start", "end"]) {
    if (typeof options[end]?.easing === "string") {
      options[end] = { ...options[end], easing: EASINGS[options[end].easing] };
    }
  }
  return {
    ...c,
    strokePoints: up.getStrokePoints(c.points, options),
    outline: up.getStroke(c.points, options),
  };
};

const mathCase = (up) => (c) => {
  const fn = up.math[c.fn];
  if (typeof fn !== "function") throw new Error(`math.json: ${c.fn} is not exported by packages/math`);
  const result = fn(...structuredClone(c.args));
  return { ...c, result: result === undefined ? null : result };
};

// -- fractional indexing (ex-107) -----------------------------------------------

/** { result } or { error } for a call that may throw. */
const outcome = (fn) => {
  try {
    return { result: fn() };
  } catch (error) {
    return { error: error.message };
  }
};

const orderKeyCase = (up) => (c) => {
  const fi = up.fractionalIndexing;
  const digits = c.digits; // undefined selects upstream's default, BASE_62_DIGITS
  switch (c.fn) {
    case "validateOrderKey": {
      const { error } = outcome(() => fi.validateOrderKey(c.key));
      return error === undefined ? { ...c, valid: true } : { ...c, valid: false, error };
    }
    case "generateKeyBetween":
      return { ...c, ...outcome(() => fi.generateKeyBetween(c.a, c.b, digits)) };
    case "generateNKeysBetween":
      return { ...c, ...outcome(() => fi.generateNKeysBetween(c.a, c.b, c.n, digits)) };
    case "walk": {
      // insert at each picked slot of a growing sorted list; keys in insertion order
      const sorted = [];
      const keys = c.picks.map((p) => {
        const key = fi.generateKeyBetween(sorted[p - 1] ?? null, sorted[p] ?? null);
        sorted.splice(p, 0, key);
        return key;
      });
      return { ...c, keys };
    }
    default:
      throw new Error(`unknown fractional-indexing case ${c.fn}`);
  }
};

/** The fields fractionalIndex.ts and mutateElement read, as API.createElement sets them. */
const toElement = (e) => ({
  type: "rectangle",
  version: 1,
  versionNonce: 0,
  isDeleted: false,
  boundElements: null,
  locked: false,
  updated: 1,
  ...e,
});

/** Resolves the chained indices of the upstream test's large arrays. */
const resolveChain = (up, c) => {
  if (!c.chain) return c.elements;
  const { generateKeyBetween } = up.fractionalIndexing;
  let last = null;
  return c.elements.map((e, i) => {
    last = c.chain === "up" ? generateKeyBetween(last, null) : generateKeyBetween(null, last);
    const keep = c.chain === "down" || i === c.elements.length - 1;
    return { ...e, index: keep ? last : e.index };
  });
};

const isValidScene = (up, elements) =>
  outcome(() =>
    up.validateFractionalIndices(elements, {
      shouldThrow: true,
      includeBoundTextValidation: true,
      ignoreLogs: true,
    }),
  ).error === undefined;

const fractionalIndexCase = (up) => (c) => {
  const { chain, ...rest } = c;
  const input = resolveChain(up, c);
  const elements = () => input.map(toElement);
  const synced = (list) => ({ indices: list.map((e) => e.index), versions: list.map((e) => e.version) });
  switch (c.fn) {
    case "syncInvalidIndices":
    case "syncMovedIndices": {
      const out = { ...rest, elements: input, validInput: isValidScene(up, elements()) };
      const list = elements();
      const moved = new Map(list.filter((e) => c.moved?.includes(e.id)).map((e) => [e.id, e]));
      const r = outcome(() =>
        c.fn === "syncInvalidIndices" ? up.syncInvalidIndices(list) : up.syncMovedIndices(list, moved),
      );
      if (r.error !== undefined) return { ...out, error: r.error };
      return { ...out, ...synced(r.result), validOutput: isValidScene(up, r.result) };
    }
    case "syncInvalidIndicesImmutable": {
      // each element carries its input position (fractionalIndex.ts never
      // reads it; newElementWith copies it), so equal ids stay distinguishable
      const list = elements().map((e, i) => ({ ...e, from: i }));
      const r = outcome(() => up.syncInvalidIndicesImmutable(list));
      if (r.error !== undefined) return { ...rest, elements: input, error: r.error };
      const entries = [...r.result].map(([id, e]) => [id, e.from, e.index, e.version]);
      return { ...rest, elements: input, entries };
    }
    case "validateFractionalIndices": {
      const logged = [];
      const error = console.error;
      console.error = (first) => logged.push(first);
      let threw;
      try {
        up.validateFractionalIndices(elements(), {
          shouldThrow: true,
          includeBoundTextValidation: c.includeBoundTextValidation,
        });
      } catch (e) {
        threw = e;
      } finally {
        console.error = error;
      }
      if (threw && threw.code !== "ELEMENT_HAS_INVALID_INDEX") throw threw;
      if (!!threw !== logged.length > 0) throw new Error(`${c.id}: log and throw disagree`);
      return { ...rest, messages: logged.length ? logged[0].split("\n\n") : [] };
    }
    case "orderByFractionalIndex": {
      // the order as positions in the input, so equal ids stay distinguishable
      const list = elements();
      const position = new Map(list.map((e, i) => [e, i]));
      return { ...rest, order: up.orderByFractionalIndex(list).map((e) => position.get(e)) };
    }
    default:
      throw new Error(`unknown fractional-index case ${c.fn}`);
  }
};

const buildGoldens = (up) => {
  const files = [randomGolden(up)];
  for (const g of roughGoldens()) files.push({ ...g, cases: g.cases.map(roughCase(up)) });
  for (const g of elementGoldens(up)) files.push({ ...g, cases: g.cases.map(elementCase(up)) });
  files.push({
    name: "freehand.json",
    description:
      "perfect-freehand 1.2.0 getStrokePoints and getStroke; easing is named (easeOutSine = sin(t*pi/2), shape.ts:1241; linear = t).",
    cases: freehandCases().map(freehandCase(up)),
  });
  files.push({
    name: "math.json",
    description:
      "packages/math/src (all but curve.ts and pca.ts): math[fn](...args) = result. Points, vectors, segments, lines, triangles, rectangles and ranges are arrays; an ellipse is { center, halfWidth, halfHeight }.",
    cases: mathCases().map(mathCase(up)),
  });
  files.push({
    name: "js-sort.json",
    description:
      "Array.prototype.sort (V8 TimSort) with a comparator that can answer NaN: kind sort is [0..n).sort((i, j) => values[i] - values[j]); kind convexHull is packages/math/src/polygon.ts convexHull(points) as indices into points. Non-finite inputs are the strings NaN, Infinity, -Infinity.",
    cases: jsSortCases().map((c) => ({ ...c, result: jsSortResult(up, c) })),
  });
  files.push({
    name: "fractional-indexing.json",
    description:
      "Vendored fractional-indexing (packages/fractional-indexing/src/index.ts): validateOrderKey, generateKeyBetween and generateNKeysBetween (base 62 unless digits is given; result or thrown message), and random insertion walks.",
    cases: orderKeyCases().map(orderKeyCase(up)),
  });
  files.push({
    name: "fractional-index.json",
    description:
      "packages/element/src/fractionalIndex.ts: syncInvalidIndices and syncMovedIndices (indices and versions after the sync, or the thrown message), syncInvalidIndicesImmutable (the returned map as [id, input position, index, version] entries in map order), validateFractionalIndices log messages, orderByFractionalIndex id order.",
    cases: fractionalIndexCases().map(fractionalIndexCase(up)),
  });
  return files;
};

/** Runs fn with Math.random and console.error disabled (see header). */
const deterministic = (fn) => {
  const random = Math.random;
  const error = console.error;
  Math.random = () => {
    throw new Error("Math.random called while generating goldens (a seed of 0 reached rough.js?)");
  };
  // upstream logs the elbow-arrow extreme-coordinate guard (shape.ts:913)
  console.error = () => {};
  try {
    return fn();
  } finally {
    Math.random = random;
    console.error = error;
  }
};

const render = (files, upstream) => {
  const out = new Map();
  for (const f of files) {
    const text = format(plain({ description: f.description, cases: f.cases }, f.name));
    out.set(f.name, text);
  }
  const versions = Object.fromEntries(
    PACKAGES.map((p) => [p, readJson(join(TOOL_DIR, "node_modules", p, "package.json")).version]),
  );
  const manifest = {
    generator: "tools/goldens/generate.mjs",
    upstream: { repo: "https://github.com/excalidraw/excalidraw", commit: upstream.commit },
    packages: versions,
    files: files.map((f) => ({
      name: f.name,
      cases: f.cases.length,
      sha256: createHash("sha256").update(out.get(f.name)).digest("hex"),
    })),
  };
  out.set("manifest.json", format(manifest));
  return out;
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`generate: ${error.message}\n`);
    process.exit(1);
  }
  // A production build: mutateElement's getUpdatedTimestamp reads MODE
  // through isTestEnv (common/src/utils.ts:552, 641).
  const up = await loadUpstream(upstream, { define: { "import.meta.env.MODE": '"production"' } });
  const files = deterministic(() => buildGoldens(up));
  const out = render(files, upstream);
  const where = relative(process.cwd(), args.out) || ".";

  if (args.check) {
    const stale = [];
    for (const [name, text] of out) {
      const path = join(args.out, name);
      if (!existsSync(path) || readFileSync(path, "utf8") !== text) stale.push(name);
    }
    const extra = existsSync(args.out)
      ? readdirSync(args.out).filter((f) => f.endsWith(".json") && !out.has(f))
      : [];
    if (stale.length || extra.length) {
      for (const name of stale) process.stderr.write(`stale: ${join(where, name)}\n`);
      for (const name of extra) process.stderr.write(`unexpected: ${join(where, name)}\n`);
      process.stderr.write("goldens are out of date: run node tools/goldens/generate.mjs\n");
      if (process.arch !== "arm64") {
        process.stderr.write(
          `note: goldens are generated on arm64; on ${process.arch} V8's float results differ in the last bits, so run the generator on arm64 (see tools/goldens/README.md)\n`,
        );
      }
      process.exit(1);
    }
    process.stdout.write(`goldens up to date: ${out.size} files in ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  for (const f of readdirSync(args.out)) {
    if (f.endsWith(".json") && !out.has(f)) rmSync(join(args.out, f));
  }
  let cases = 0;
  for (const [name, text] of out) writeFileSync(join(args.out, name), text);
  for (const f of files) cases += f.cases.length;
  process.stdout.write(
    `wrote ${out.size} files (${cases} cases) to ${where} from upstream ${upstream.commit.slice(0, 7)}\n`,
  );
};

await main();
