// Shared helpers for the golden generator tests. Everything here is an
// independent re-statement of upstream behaviour (from the cited sources),
// used to check the generated goldens; none of it feeds the generator.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const TOOL_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const REPO_ROOT = resolve(TOOL_DIR, "..", "..");
export const GOLDENS_DIR = join(REPO_ROOT, "goldens");
export const GENERATE = join(TOOL_DIR, "generate.mjs");

const CHECKOUT = join(REPO_ROOT, "scripts", "upstream", "checkout.sh");
export const upstreamDir = () =>
  execFileSync("bash", [CHECKOUT, "--print-dir"], { encoding: "utf8" }).trim();
export const pinnedCommit = () =>
  execFileSync("bash", [CHECKOUT, "--print-pin"], { encoding: "utf8" }).trim();

export const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

export const golden = (name) => {
  const path = join(GOLDENS_DIR, name);
  if (!existsSync(path)) {
    throw new Error(`missing golden ${path} (run node tools/goldens/generate.mjs)`);
  }
  return readJson(path);
};

/** Every numeric leaf of a JSON value. */
export function* numbers(value) {
  if (typeof value === "number") {
    yield value;
  } else if (Array.isArray(value)) {
    for (const item of value) yield* numbers(item);
  } else if (value && typeof value === "object") {
    for (const item of Object.values(value)) yield* numbers(item);
  }
}

/**
 * roughjs 4.6.4 Random.next (src/math.ts), the Park-Miller "minimal
 * standard" LCG with multiplier 48271:
 *   ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31
 */
export const parkMiller = (seed, count) => {
  const values = [];
  let state = seed;
  for (let i = 0; i < count; i++) {
    state = Math.imul(48271, state);
    values.push(((2 ** 31 - 1) & state) / 2 ** 31);
  }
  return values;
};

/**
 * roughjs 4.6.4 RoughGenerator.opsToPath(drawing, fixedDecimals): the
 * function upstream's SVG export uses (via RoughSVG.draw with
 * fixedDecimalPlaceDigits = MAX_DECIMALS_FOR_SVG_EXPORT = 2).
 */
export const opsToPath = (set, fixedDecimals) => {
  let path = "";
  for (const item of set.ops) {
    const data =
      typeof fixedDecimals === "number" && fixedDecimals >= 0
        ? item.data.map((d) => +d.toFixed(fixedDecimals))
        : item.data;
    switch (item.op) {
      case "move":
        path += `M${data[0]} ${data[1]} `;
        break;
      case "bcurveTo":
        path += `C${data[0]} ${data[1]}, ${data[2]} ${data[3]}, ${data[4]} ${data[5]} `;
        break;
      case "lineTo":
        path += `L${data[0]} ${data[1]} `;
        break;
    }
  }
  return path.trim();
};

// -- packages/element/src/shape.ts generateRoughOptions (:195-260) ----------

const LINEAR = new Set(["line", "arrow"]);
const ROUNDABLE = new Set([
  "rectangle",
  "iframe",
  "embeddable",
  "line",
  "diamond",
  "stickynote",
  "image",
]); // element/src/comparisons.ts:57-64 canChangeRoundness

/** shape.ts:172-193 */
export const adjustRoughness = (element) => {
  const { roughness } = element;
  const maxSize = Math.max(element.width, element.height);
  const minSize = Math.min(element.width, element.height);
  if (
    (minSize >= 20 && maxSize >= 50) ||
    (minSize >= 15 && !!element.roundness && ROUNDABLE.has(element.type)) ||
    (LINEAR.has(element.type) && maxSize >= 50)
  ) {
    return roughness;
  }
  return Math.min(roughness / (maxSize < 10 ? 3 : 2), 2.5);
};

/** shape.ts:168-170 */
export const strokeLineDash = (element) =>
  element.strokeStyle === "dashed"
    ? [8, 8 + element.strokeWidth]
    : element.strokeStyle === "dotted"
    ? [1.5, 6 + element.strokeWidth]
    : undefined;

/** The research table's option values that do not depend on the theme. */
export const expectedRoughOptions = (element, continuousPath) => ({
  seed: element.seed,
  disableMultiStroke: element.strokeStyle !== "solid",
  strokeWidth:
    element.strokeStyle !== "solid" ? element.strokeWidth + 0.5 : element.strokeWidth,
  fillWeight: element.strokeWidth / 2,
  hachureGap: element.strokeWidth * 4,
  roughness: adjustRoughness(element),
  preserveVertices: continuousPath || element.roughness < 2,
  curveFitting: element.type === "ellipse" ? 1 : 0.95,
});

/** roughjs 4.6.4 defaultOptions (src/generator.ts). */
export const ROUGH_DEFAULTS = {
  maxRandomnessOffset: 2,
  roughness: 1,
  bowing: 1,
  stroke: "#000",
  strokeWidth: 1,
  curveTightness: 0,
  curveFitting: 0.95,
  curveStepCount: 9,
  fillStyle: "hachure",
  fillWeight: -1,
  hachureAngle: -41,
  hachureGap: -1,
  dashOffset: -1,
  dashGap: -1,
  zigzagOffset: -1,
  seed: 0,
  disableMultiStroke: false,
  disableMultiStrokeFill: false,
  preserveVertices: false,
  fillShapeRoughnessGain: 0.8,
};

/** shape.ts:1314-1344 getSvgPathFromStroke, restated. */
export const svgPathFromStroke = (points) => {
  if (!points.length) {
    return "";
  }
  const med = (A, B) => [(A[0] + B[0]) / 2, (A[1] + B[1]) / 2];
  const max = points.length - 1;
  const TO_FIXED_PRECISION = /(\s?[A-Z]?,?-?[0-9]*\.[0-9]{0,2})(([0-9]|e|-)*)/g;
  return points
    .reduce(
      (acc, point, i, arr) => {
        if (i === max) {
          acc.push(point, med(point, arr[0]), "L", arr[0], "Z");
        } else {
          acc.push(point, med(point, arr[i + 1]));
        }
        return acc;
      },
      ["M", points[0], "Q"],
    )
    .join(" ")
    .replace(TO_FIXED_PRECISION, "$1");
};

/** The fourteen arrowheads of element/src/types.ts:342-365. */
export const ARROWHEADS = [
  "arrow",
  "bar",
  "circle",
  "circle_outline",
  "triangle",
  "triangle_outline",
  "diamond",
  "diamond_outline",
  "cardinality_one",
  "cardinality_many",
  "cardinality_one_or_many",
  "cardinality_exactly_one",
  "cardinality_zero_or_one",
  "cardinality_zero_or_many",
];

export const ELEMENT_FILES = [
  "elements-upstream-fixtures.json",
  "elements-rectangle.json",
  "elements-diamond.json",
  "elements-ellipse.json",
  "elements-line.json",
  "elements-arrow.json",
  "elements-arrowheads.json",
  "elements-elbow-arrow.json",
  "elements-freedraw.json",
  "elements-iframe-like.json",
];

export const ROUGH_FILES = [
  "rough-primitives.json",
  "rough-generator.json",
  "rough-fills.json",
  "rough-options.json",
  "rough-strokes.json",
];

export const ALL_FILES = [
  "random.json",
  ...ROUGH_FILES,
  ...ELEMENT_FILES,
  "freehand.json",
  "laser-pointer.json",
  "math.json",
  "js-sort.json",
  "fractional-indexing.json",
  "fractional-index.json",
];
