#!/usr/bin/env node
// Rough option mapping goldens for excali-scene (ex-207): upstream's own
// generateRoughOptions (packages/element/src/shape.ts:195-260, with the
// private adjustRoughness at :172-193) and applyDarkModeFilter
// (packages/common/src/colors.ts:86-125), run from the pinned checkout under
// plain Node.
//
//   node tools/goldens/rough-options.mjs            write the fixture
//   node tools/goldens/rough-options.mjs --check    exit 1 if it is stale
//   node tools/goldens/rough-options.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/rough-options.json:
//
// - base, typeFields: every element is {...base, ...typeFields[type],
//   ...overrides}; the Rust test builds the same object and reads it with
//   Element::from_map, so both sides see the same element;
// - options: generateRoughOptions(element, continuousPath, isDarkMode) for
//   every row of the option table in site/content/research/rendering.md
//   section 1 (seed, dashes, multi-stroke, widths, fill weight, hachure gap,
//   roughness, stroke colour, preserveVertices, fill style and fill per
//   type, curveFitting, line/freedraw loops, arrows) and the types it
//   throws for. `options` is the returned object as JSON (undefined values
//   dropped) and `keys` its own keys in order, so a key set to undefined
//   (fill: undefined for a transparent background) is still recorded;
// - adjustRoughness: the roughness generateRoughOptions returns for a grid
//   of sizes around every threshold of adjustRoughness (20/50, 15, 50, 10)
//   for sharp, rounded and linear elements, and roughness values around the
//   2.5 cap;
// - darkMode: applyDarkModeFilter(color) for colours in every notation.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const FILE = "rough-options.json";

const ENTRY = `
export { generateRoughOptions } from "./packages/element/src/shape";
export { applyDarkModeFilter } from "./packages/common/src/colors";
`;

const usage = () => {
  process.stderr.write("usage: rough-options.mjs [--check] [--out DIR]\n");
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

/** What JSON.stringify writes, read back. */
const json = (value) => JSON.parse(JSON.stringify(value));

// -- elements -------------------------------------------------------------------

/** _ExcalidrawElementBase with DEFAULT_ELEMENT_PROPS (constants.ts:514-532). */
const BASE = {
  id: "el",
  type: "rectangle",
  x: 0,
  y: 0,
  width: 100,
  height: 100,
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
  index: null,
  roundness: null,
  seed: 1041657908,
  version: 1,
  versionNonce: 0,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
};

const LINEAR = {
  points: [
    [0, 0],
    [100, 100],
  ],
  startBinding: null,
  endBinding: null,
  startArrowhead: null,
  endArrowhead: null,
};

/** The per-type keys newElement.ts gives each type. */
const TYPE_FIELDS = {
  rectangle: {},
  diamond: {},
  ellipse: {},
  embeddable: {},
  iframe: {},
  selection: {},
  stickynote: { baseHeight: 100 },
  image: { fileId: null, status: "pending", scale: [1, 1], crop: null },
  frame: { name: null },
  magicframe: { name: null },
  text: {
    fontSize: 20,
    fontFamily: 5,
    text: "",
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    originalText: "",
    autoResize: true,
    lineHeight: 1.25,
  },
  line: { ...LINEAR, polygon: false },
  arrow: { ...LINEAR, endArrowhead: "arrow", elbowed: false },
  freedraw: {
    points: LINEAR.points,
    pressures: [],
    simulatePressure: true,
    strokeOptions: { variability: "variable", streamline: 0.5 },
  },
};

const element = (overrides) => {
  const type = overrides.type ?? BASE.type;
  return structuredClone({ ...BASE, ...TYPE_FIELDS[type], ...overrides });
};

// -- options --------------------------------------------------------------------

const SHAPES = ["rectangle", "iframe", "embeddable", "diamond", "ellipse"];
const STROKE_COLORS = [
  "#1e1e1e",
  "#e03131",
  "#ffffff",
  "#000000",
  "#abc",
  "#11223344",
  "1971c2",
  "transparent",
  "rgba(255, 0, 0, 0.5)",
  "rgb(12, 34, 56)",
  "hsl(120, 50%, 50%)",
  "hsva(200, 40%, 60%, 0.25)",
  "red",
  "RebeccaPurple",
  "not a colour",
  "",
];
const BACKGROUNDS = ["transparent", "#a5d8ff", "rgba(0,0,0,0)", "#ffc9c980", "hsla(0, 0%, 0%, 0)"];
const FILL_STYLES = ["hachure", "cross-hatch", "solid", "zigzag"];
const LOOPS = {
  open: [[0, 0], [50, 50], [100, 0]],
  closed: [[0, 0], [100, 0], [100, 100], [0, 0]],
  "gap-8": [[0, 0], [100, 0], [50, 50], [8, 0]],
  "gap-8.001": [[0, 0], [100, 0], [50, 50], [8.001, 0]],
  "gap-diagonal-7.98": [[0, 0], [100, 0], [50, 50], [5.6, 5.7]],
  "gap-diagonal-8.49": [[0, 0], [100, 0], [50, 50], [6, 6]],
  "two-points": [[0, 0], [0, 0]],
  "three-same": [[0, 0], [0, 0], [0, 0]],
  "one-point": [[0, 0]],
};

function* optionInputs() {
  // seed (:201)
  for (const seed of [0, 1, 7, 1041657908, 2147483647, 2147483648, 4294967296, -5, 12.5]) {
    yield [`seed/${seed}`, { seed }];
  }
  // strokeLineDash, disableMultiStroke, strokeWidth, fillWeight, hachureGap
  // (:168-170, :202-221)
  for (const strokeStyle of ["solid", "dashed", "dotted"]) {
    for (const strokeWidth of [0, 0.5, 1, 2, 3.3, 4, 16]) {
      yield [`stroke/${strokeStyle}/sw${strokeWidth}`, { strokeStyle, strokeWidth }];
    }
  }
  // roughness and preserveVertices (:222, :224-225)
  for (const roughness of [0, 1, 1.5, 1.99, 2, 2.5, 3, 10]) {
    for (const continuousPath of [false, true]) {
      yield [`roughness/${roughness}/continuous-${continuousPath}`, { roughness }, continuousPath];
    }
  }
  // stroke through the dark filter (:223)
  for (const strokeColor of STROKE_COLORS) {
    for (const dark of [false, true]) {
      yield [`stroke-color/${JSON.stringify(strokeColor)}/dark-${dark}`, { strokeColor }, false, dark];
    }
  }
  // fillStyle / fill for the closed shapes, curveFitting for the ellipse
  // (:229-240)
  for (const type of SHAPES) {
    for (const backgroundColor of BACKGROUNDS) {
      for (const dark of [false, true]) {
        yield [
          `fill/${type}/${JSON.stringify(backgroundColor)}/dark-${dark}`,
          { type, backgroundColor, fillStyle: "hachure" },
          false,
          dark,
        ];
      }
    }
    for (const fillStyle of FILL_STYLES) {
      yield [`fill-style/${type}/${fillStyle}`, { type, fillStyle, backgroundColor: "#ffec99" }];
    }
    yield [`continuous/${type}`, { type, roughness: 2 }, true];
  }
  // line / freedraw fill only for loops (:243-252); arrow never (:254-255)
  for (const type of ["line", "freedraw", "arrow"]) {
    for (const [name, points] of Object.entries(LOOPS)) {
      for (const backgroundColor of ["transparent", "#b2f2bb", "rgba(0,0,0,0)"]) {
        for (const dark of [false, true]) {
          yield [
            `linear-fill/${type}/${name}/${JSON.stringify(backgroundColor)}/dark-${dark}`,
            { type, points, backgroundColor, fillStyle: "cross-hatch" },
            false,
            dark,
          ];
        }
      }
    }
  }
  yield ["linear-fill/line/polygon", { type: "line", points: LOOPS.closed, polygon: true, backgroundColor: "#b2f2bb" }];
  yield ["linear-fill/line/open-polygon", { type: "line", points: LOOPS.open, polygon: true, backgroundColor: "#b2f2bb" }];
  // the types it throws for (:256-258)
  for (const type of ["selection", "stickynote", "image", "frame", "magicframe", "text"]) {
    yield [`unimplemented/${type}`, { type }];
  }
}

const optionCases = (up) =>
  [...optionInputs()].map(([id, overrides, continuousPath = false, isDarkMode = false]) => {
    const out = { id, element: overrides, continuousPath, isDarkMode };
    try {
      const options = up.generateRoughOptions(element(overrides), continuousPath, isDarkMode);
      out.keys = Object.keys(options);
      out.options = json(options);
    } catch (error) {
      out.error = `${error.constructor.name}: ${error.message}`;
    }
    return out;
  });

// -- adjustRoughness ------------------------------------------------------------

/** Around every threshold: 10 (the divisor), 15, 20 and 50; negative sizes. */
const SIZES = [-60, 0, 9.99, 10, 14.99, 15, 19.99, 20, 49.99, 50, 120];

const ROUGHNESS_VARIANTS = [
  { type: "rectangle", roundness: null },
  { type: "rectangle", roundness: { type: 3 } },
  { type: "rectangle", roundness: { type: 1 } },
  { type: "diamond", roundness: { type: 2 } },
  { type: "ellipse", roundness: { type: 2 } },
  { type: "iframe", roundness: { type: 3 } },
  { type: "embeddable", roundness: null },
  { type: "line", roundness: null },
  { type: "line", roundness: { type: 2 } },
  { type: "arrow", roundness: null },
  { type: "arrow", roundness: { type: 2 } },
  { type: "freedraw", roundness: null },
];

function* roughnessInputs() {
  for (const v of ROUGHNESS_VARIANTS) {
    for (const width of SIZES) {
      for (const height of SIZES) {
        yield { ...v, width, height, roughness: 2 };
      }
    }
  }
  // the 2.5 cap, for both divisors
  for (const roughness of [0, 1, 4.5, 5, 5.01, 7.5, 7.51, 9, 100]) {
    for (const [width, height] of [
      [5, 5],
      [30, 30],
      [60, 60],
    ]) {
      yield { type: "rectangle", roundness: null, width, height, roughness };
    }
  }
}

const roughnessCases = (up) =>
  [...roughnessInputs()].map((c) => ({
    ...c,
    result: up.generateRoughOptions(element(c), false, false).roughness,
  }));

// -- dark mode ------------------------------------------------------------------

const DARK_COLORS = [
  ...STROKE_COLORS,
  "#fff",
  "#f8f9fa",
  "#e9ecef",
  "#868e96",
  "#343a40",
  "#ffc9c9",
  "#b2f2bb",
  "#a5d8ff",
  "#ffec99",
  "#2f9e44",
  "#1971c2",
  "#f08c00",
  "#00000000",
  "#12345678",
  "rgba(10, 20, 30, 0.004)",
  "hsl(0, 100%, 50%)",
  "hsv(300, 100%, 100%)",
  "cyan",
];

const darkModeCases = (up) =>
  [...new Set(DARK_COLORS)].map((color) => ({ color, result: up.applyDarkModeFilter(color) }));

// -- main -----------------------------------------------------------------------

const build = async (upstream) => {
  const up = await loadUpstream(upstream, { entry: ENTRY });
  const options = optionCases(up);
  const ids = new Set();
  for (const c of options) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  return format({
    description:
      "Upstream generateRoughOptions (shape.ts:195-260, adjustRoughness :172-193) and applyDarkModeFilter (colors.ts:86-125) at the pinned commit (tools/goldens/rough-options.mjs). An element is {...base, ...typeFields[type], ...overrides}.",
    upstream: upstream.commit,
    base: BASE,
    typeFields: TYPE_FIELDS,
    options,
    adjustRoughness: roughnessCases(up),
    darkMode: darkModeCases(up),
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating rough option goldens");
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
    process.stderr.write(`rough-options: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(
        `stale: ${where}\nrough option goldens are out of date: run node tools/goldens/rough-options.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`rough option goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
