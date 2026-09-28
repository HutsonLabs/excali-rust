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
import { format } from "./lib/format.mjs";
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
  const result = generator[c.method](...c.args, { ...c.options });
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
  return {
    ...c,
    strokePoints: up.getStrokePoints(c.points, options),
    outline: up.getStroke(c.points, options),
  };
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
  const up = await loadUpstream(upstream);
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
