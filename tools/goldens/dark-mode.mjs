#!/usr/bin/env node
// Dark-mode colour filter goldens for excali-scene (ex-218): upstream's own
// applyDarkModeFilter, removeDarkModeFilter and rgbToHex
// (packages/common/src/colors.ts:86-160, 345-362) and COLOR_PALETTE
// (:193-212), run from the pinned checkout under plain Node.
//
//   node tools/goldens/dark-mode.mjs            write the fixture
//   node tools/goldens/dark-mode.mjs --check    exit 1 if it is stale
//   node tools/goldens/dark-mode.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/dark-mode.json:
//
// - filter: DARK_THEME_FILTER (common/src/constants.ts:204);
// - palette: COLOR_PALETTE as upstream exports it (key order kept);
// - paletteFilter: for every palette colour (name, shade index or null),
//   applyDarkModeFilter(color), removeDarkModeFilter(color),
//   removeDarkModeFilter of the filtered colour, and applyDarkModeFilter of
//   that again (the round trip upstream's own test checks for a few colours,
//   colors.test.ts:225-235);
// - apply: applyDarkModeFilter for the cases of upstream's colors.test.ts
//   and colours in every notation tinycolor reads, with enable true and
//   false;
// - remove: removeDarkModeFilter for the same inputs and every component
//   value on the grey axis (the reverse invert's clamping);
// - rgbToHex: upstream's rgbToHex for the cases of colors.test.ts:237-305
//   and alpha rounding around the half steps.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const FILE = "dark-mode.json";

const ENTRY = `
export {
  applyDarkModeFilter,
  removeDarkModeFilter,
  rgbToHex,
  COLOR_PALETTE,
  DARK_THEME_FILTER,
} from "@excalidraw/common";
`;

const usage = () => {
  process.stderr.write("usage: dark-mode.mjs [--check] [--out DIR]\n");
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

// -- palette --------------------------------------------------------------------

function* paletteColors(palette) {
  for (const [name, value] of Object.entries(palette)) {
    if (typeof value === "string") {
      yield { name, index: null, color: value };
    } else {
      for (const [index, color] of value.entries()) yield { name, index, color };
    }
  }
}

const paletteCases = (up) =>
  [...paletteColors(up.COLOR_PALETTE)].map(({ name, index, color }) => {
    const dark = up.applyDarkModeFilter(color);
    const restored = up.removeDarkModeFilter(dark);
    return {
      name,
      index,
      color,
      dark,
      remove: up.removeDarkModeFilter(color),
      restored,
      redark: up.applyDarkModeFilter(restored),
    };
  });

// -- notations ------------------------------------------------------------------

/** colors.test.ts:14-105, 220-223, then every notation and edge tinycolor reads. */
const COLORS = [
  // colors.test.ts
  "#000000",
  "#ffffff",
  "#ff0000",
  "#00ff00",
  "#0000ff",
  "red",
  "rgb(255, 0, 0)",
  "rgba(255, 0, 0, 0.5)",
  "transparent",
  "#f00",
  "#ff0000ff",
  "#ff000080",
  "#ff000000",
  "#ededed",
  "#121212",
  // hex forms
  "#FF8800",
  "ff8800",
  "f80",
  "#f808",
  "  #1e1e1e  ",
  "#12345678",
  "#00000001",
  "#000000fe",
  // functional forms
  "rgb(10%, 20%, 30%)",
  "rgba(10, 20, 30, 0.004)",
  "rgba(10, 20, 30, 0.002)",
  "rgba(10, 20, 30, 1.5)",
  "rgb 1 2 3",
  "rgb(300, -5, 128.6)",
  "hsl(0, 100%, 50%)",
  "hsla(210, 50%, 40%, 0.25)",
  "hsv(300, 100%, 100%)",
  "hsva(120, 40%, 60%, 0.75)",
  // names
  "cyan",
  "rebeccapurple",
  "WHITE",
  // not colours: tinycolor reads opaque black
  "",
  "nope",
  "#12",
  "rgb(1, 2)",
];

/** Every component on the grey axis: the reverse invert clamps both ends. */
const GREYS = Array.from({ length: 256 }, (_, v) => {
  const h = v.toString(16).padStart(2, "0");
  return `#${h}${h}${h}`;
});

const applyCases = (up) =>
  COLORS.map((color) => ({
    color,
    result: up.applyDarkModeFilter(color),
    disabled: up.applyDarkModeFilter(color, false),
  }));

const removeCases = (up) =>
  [...new Set([...COLORS, ...GREYS])].map((color) => ({
    color,
    result: up.removeDarkModeFilter(color),
  }));

// -- rgbToHex -------------------------------------------------------------------

const RGB_TO_HEX = [
  [0, 0, 0],
  [255, 255, 255],
  [255, 0, 0],
  [0, 255, 0],
  [0, 0, 255],
  [30, 30, 30],
  [0, 0, 1],
  [0, 1, 0],
  [1, 0, 0],
  [15, 15, 15],
  [255, 0, 0, null],
  [255, 0, 0, 1],
  [255, 0, 0, 0.5],
  [255, 0, 0, 0],
  [255, 0, 0, 0.99],
  [255, 0, 0, 0.05],
  [18, 52, 86, 0.001],
  [18, 52, 86, 0.002],
  [18, 52, 86, 0.998],
  [18, 52, 86, 1.5],
];

const rgbToHexCases = (up) =>
  RGB_TO_HEX.map(([r, g, b, a]) => ({
    args: a === undefined ? [r, g, b] : [r, g, b, a],
    result: up.rgbToHex(r, g, b, a ?? undefined),
  }));

// -- main -----------------------------------------------------------------------

const build = async (upstream) => {
  const up = await loadUpstream(upstream, { entry: ENTRY });
  return format({
    description:
      "Upstream applyDarkModeFilter, removeDarkModeFilter (colors.ts:86-160), rgbToHex (:345-362), COLOR_PALETTE (:193-212) and DARK_THEME_FILTER (constants.ts:204) at the pinned commit (tools/goldens/dark-mode.mjs). rgbToHex args: a null alpha is undefined.",
    upstream: upstream.commit,
    filter: up.DARK_THEME_FILTER,
    palette: up.COLOR_PALETTE,
    paletteFilter: paletteCases(up),
    apply: applyCases(up),
    remove: removeCases(up),
    rgbToHex: rgbToHexCases(up),
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating dark-mode goldens");
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
    process.stderr.write(`dark-mode: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(
        `stale: ${where}\ndark-mode goldens are out of date: run node tools/goldens/dark-mode.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`dark-mode goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
