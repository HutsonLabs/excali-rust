#!/usr/bin/env node
// Restore fixtures for excali-core's base normalisation (ex-103): upstream's
// own restoreElementWithProperties and restoreElement
// (packages/excalidraw/data/restore.ts:430-515, 517-752), run from the
// pinned checkout under plain Node on a table of inputs.
//
//   node tools/goldens/restore-fixtures.mjs            write the fixtures
//   node tools/goldens/restore-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/restore-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/restore-base.json:
//
//   { "description", "upstream", "cases": [ { id, call, element, extra?, result | error } ] }
//
// - call "base": restoreElementWithProperties(element, extra). The function
//   is module-private upstream; the loader appends an export line to
//   restore.ts (lib/upstream.mjs, exposedModules) and changes nothing else.
//   `extra` is a list of [key, value] entries in order; an entry with no
//   value is a key set to undefined, which JSON cannot hold.
// - call "restoreElement": restoreElement(element, targetElementsMap, null)
//   for the generic types (rectangle, diamond, ellipse, iframe, embeddable),
//   whose restore is restoreElementWithProperties(element, {})
//   (restore.ts:726-731).
// - result: JSON.parse(JSON.stringify(restored)), so keys set to undefined
//   are gone and non-finite numbers are null, as a saved file has them.
//   error: the message when the call throws (restoreElements then drops the
//   element, restore.ts:977-979).
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"), where randomId() is `id${n}` and getUpdatedTimestamp() is 1
// (packages/common/src/random.ts:16, utils.ts:552); reseed() before each
// case restarts the ids at id0. Math.random throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "restore-base.json";

const ENTRY = `
export { restoreElement, restoreElementWithProperties } from "./packages/excalidraw/data/restore";
export { reseed } from "./packages/common/src/random";
`;
const EXPOSE = { "packages/excalidraw/data/restore": ["restoreElementWithProperties"] };

const usage = () => {
  process.stderr.write("usage: restore-fixtures.mjs [--check] [--out DIR]\n");
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

// -- inputs ---------------------------------------------------------------------

/** A complete current element as a file written today holds it. */
const full = (rest = {}) => ({
  id: "el",
  type: "rectangle",
  x: 10,
  y: 20,
  width: 100,
  height: 50,
  angle: 0.5,
  strokeColor: "#e03131",
  backgroundColor: "#ffec99",
  fillStyle: "hachure",
  strokeWidth: 4,
  strokeStyle: "dashed",
  roughness: 2,
  opacity: 60,
  groupIds: ["g1"],
  frameId: "f1",
  index: "a1",
  roundness: { type: 3 },
  seed: 42,
  version: 7,
  versionNonce: 99,
  isDeleted: false,
  boundElements: [{ id: "a1", type: "arrow" }],
  updated: 1700000000000,
  created: 1690000000000,
  link: "https://example.com",
  locked: true,
  ...rest,
});

/**
 * The values each base field is tried with: absent, every JSON falsy value
 * and a truthy one, so the table shows which fields use `||` and which `??`.
 */
const FALSY = [["absent"], ["null", null], ["zero", 0], ["empty", ""], ["false", false]];
const BASE_FIELDS = {
  version: 3,
  versionNonce: 5,
  index: "a5",
  isDeleted: true,
  id: "given",
  fillStyle: "zigzag",
  strokeWidth: 8,
  strokeStyle: "dotted",
  roughness: 0.5,
  opacity: 30,
  angle: 1.25,
  x: 12.5,
  y: -7,
  strokeColor: "#1971c2",
  backgroundColor: "#b2f2bb",
  width: 64,
  height: 32,
  seed: 1234,
  groupIds: ["a", "b"],
  frameId: "frame",
  roundness: { type: 2, value: 12 },
  boundElements: [{ type: "text", id: "t" }],
  updated: 1600000000000,
  created: 1500000000000,
  link: "https://excalidraw.com",
  locked: true,
  customData: { k: "v" },
};

const without = (object, key) => {
  const copy = { ...object };
  delete copy[key];
  return copy;
};

const fieldTable = () => {
  const cases = [];
  for (const [field, truthy] of Object.entries(BASE_FIELDS)) {
    for (const [name, ...value] of [...FALSY, ["set", truthy]]) {
      const element = without(full(), field);
      if (value.length) element[field] = value[0];
      cases.push({ id: `field-${field}-${name}`, call: "base", element });
    }
  }
  return cases;
};

const minimal = () => [
  { id: "minimal-type-only", call: "base", element: { type: "rectangle" } },
  { id: "minimal-empty-object", call: "base", element: {} },
  {
    // base.type is undefined (`"" || undefined`) but holds its place, so
    // extra's "" lands there, not after the other base keys.
    id: "minimal-no-type-extra-type-empty",
    call: "base",
    element: { id: "n", future: 1 },
    extra: [["type", ""]],
  },
  {
    id: "minimal-type-and-id",
    call: "base",
    element: { id: "r", type: "ellipse" },
  },
];

const TYPES = [
  "rectangle",
  "embeddable",
  "iframe",
  "image",
  "diamond",
  "ellipse",
  "stickynote",
  "text",
  "line",
  "arrow",
  "freedraw",
  "frame",
  "magicframe",
  "draw",
  "selection",
  "future-type",
];

const legacy = (type, rest = {}) => {
  const element = without(without(full({ type }), "roundness"), "boundElements");
  return { ...element, ...rest };
};

const roundnessCases = () => [
  ...TYPES.map((type) => ({
    id: `strokeSharpness-round-${type}`,
    call: "base",
    element: legacy(type, { strokeSharpness: "round" }),
  })),
  ...TYPES.map((type) => ({
    id: `strokeSharpness-sharp-${type}`,
    call: "base",
    element: legacy(type, { strokeSharpness: "sharp" }),
  })),
  {
    id: "strokeSharpness-round-roundness-null",
    call: "base",
    element: legacy("rectangle", { roundness: null, strokeSharpness: "round" }),
  },
  {
    id: "strokeSharpness-round-roundness-zero",
    call: "base",
    element: legacy("diamond", { roundness: 0, strokeSharpness: "round" }),
  },
  {
    id: "strokeSharpness-round-roundness-set-wins",
    call: "base",
    element: legacy("rectangle", { roundness: { type: 3 }, strokeSharpness: "round" }),
  },
  {
    id: "strokeSharpness-round-roundness-truthy-kept-as-is",
    call: "base",
    element: legacy("rectangle", { roundness: "yes", strokeSharpness: "round" }),
  },
  {
    id: "strokeSharpness-uppercase-is-not-round",
    call: "base",
    element: legacy("rectangle", { strokeSharpness: "ROUND" }),
  },
  {
    id: "strokeSharpness-non-string",
    call: "base",
    element: legacy("rectangle", { strokeSharpness: true }),
  },
  {
    id: "strokeSharpness-null-removed",
    call: "base",
    element: legacy("rectangle", { strokeSharpness: null }),
  },
  {
    // The radius type follows element.type, not the type restore writes.
    id: "strokeSharpness-round-draw-restored-as-line",
    call: "base",
    element: legacy("draw", { strokeSharpness: "round" }),
    extra: [["type", "line"]],
  },
  {
    id: "strokeSharpness-round-line-restored-as-rectangle-type",
    call: "base",
    element: legacy("line", { strokeSharpness: "round" }),
    extra: [["type", "rectangle"]],
  },
];

const boundElementsCases = () => [
  {
    id: "boundElementIds-to-boundElements",
    call: "base",
    element: legacy("rectangle", { boundElementIds: ["a1", "a2"] }),
  },
  {
    id: "boundElementIds-win-over-boundElements",
    call: "base",
    element: legacy("rectangle", {
      boundElements: [{ type: "text", id: "t1" }],
      boundElementIds: ["a1"],
    }),
  },
  {
    id: "boundElementIds-empty-array",
    call: "base",
    element: legacy("rectangle", {
      boundElements: [{ type: "text", id: "t1" }],
      boundElementIds: [],
    }),
  },
  {
    id: "boundElementIds-null-falls-back",
    call: "base",
    element: legacy("rectangle", {
      boundElements: [{ type: "text", id: "t1" }],
      boundElementIds: null,
    }),
  },
  {
    id: "boundElementIds-falsy-empty-string",
    call: "base",
    element: legacy("rectangle", { boundElementIds: "" }),
  },
  {
    id: "boundElementIds-any-values-mapped",
    call: "base",
    element: legacy("rectangle", { boundElementIds: [1, null, { x: 1 }, ["n"]] }),
  },
  {
    id: "boundElementIds-string-throws",
    call: "base",
    element: legacy("rectangle", { boundElementIds: "a1" }),
  },
  {
    id: "boundElementIds-object-throws",
    call: "base",
    element: legacy("rectangle", { boundElementIds: { 0: "a1" } }),
  },
  {
    id: "boundElementIds-number-throws",
    call: "base",
    element: legacy("rectangle", { boundElementIds: 3 }),
  },
  {
    id: "boundElements-null-becomes-empty",
    call: "base",
    element: legacy("rectangle", { boundElements: null }),
  },
  {
    id: "boundElements-kept-as-is",
    call: "base",
    element: legacy("rectangle", { boundElements: [{ type: "arrow", id: "x", extra: 1 }] }),
  },
];

const LINKS = [
  ["https", "https://excalidraw.com/?a=1#b"],
  ["padded", "  https://excalidraw.com \n"],
  ["whitespace-only", " \t\n "],
  ["nbsp-and-bom-trimmed", "\u00a0\ufeffhttps://x.test\u3000"],
  ["nel-not-trimmed", "\u0085https://x.test"],
  ["javascript", "javascript:alert(1)"],
  ["javascript-mixed-case", "JaVaScRiPt:alert(1)"],
  ["javascript-leading-symbols", "  %20javascript:alert(1)"],
  ["javascript-leading-word", "xjavascript:alert(1)"],
  ["data", "data:text/html;base64,PHNjcmlwdD4="],
  ["vbscript", "vbscript:msgbox"],
  ["javascript-entity-encoded", "&#106;&#97;&#118;&#97;&#115;&#99;&#114;&#105;&#112;&#116;&#58;alert(1)"],
  ["javascript-entity-no-semicolon", "&#106avascript:alert(1)"],
  ["entity-hex-prefix", "&#0x6A;avascript:x"],
  ["entity-x-is-nan", "&#x6A;avascript:x"],
  ["entity-exponent", "&#1e2;ata"],
  ["entity-overflow-wraps", "&#65601;bc"],
  ["entity-infinity", "&#Infinity;ok"],
  ["entity-surrogate-pair", "https://x.test/&#55357;&#56832;"],
  ["entity-lone-surrogate", "https://x.test/&#55357;z"],
  ["entity-newline-removed", "java&NewLine;script:alert(1)"],
  ["entity-tab-removed", "java&tab;script:alert(1)"],
  ["control-characters-removed", "java\u0001script\u200b:alert(1)"],
  ["c1-control-removed", "\u0080\u009fhttps://x.test"],
  ["colon-entity", "javascript&colon;alert(1)"],
  ["colon-entity-uppercase", "javascript&COLON;alert(1)"],
  ["double-quotes-escaped", 'https://x.test/"q"'],
  ["double-quote-entity-decoded-no", "https://x.test/&quot;"],
  ["relative-slash", "/path?q=1"],
  ["relative-dot", "./rel:javascript:x"],
  ["mailto", "mailto:someone@example.com"],
  ["no-scheme", "example.com/path"],
  ["colon-first", ":javascript:x"],
  ["line-separator-scheme", "safe\u2028javascript:alert(1)"],
  ["line-separator-first-line-wins", "a:b\u2028javascript:alert(1)"],
  ["only-entity-to-empty", "&#1;"],
  ["element-link", "https://excalidraw.com/?element=abc"],
  ["unicode", "https://\u4f8b\u3048.\u30c6\u30b9\u30c8/\u30d1\u30b9"],
];

const linkCases = () => [
  ...LINKS.map(([name, link]) => ({
    id: `link-${name}`,
    call: "base",
    element: full({ link }),
  })),
  { id: "link-number-throws", call: "base", element: full({ link: 5 }) },
  { id: "link-true-throws", call: "base", element: full({ link: true }) },
  { id: "link-array-throws", call: "base", element: full({ link: [] }) },
  { id: "link-object-throws", call: "base", element: full({ link: {} }) },
];

const customDataCases = () => [
  {
    id: "customData-extra-wins",
    call: "base",
    element: full({ customData: { a: 1 } }),
    extra: [["customData", { b: 2 }]],
  },
  {
    id: "customData-extra-only",
    call: "base",
    element: full(),
    extra: [["customData", { b: 2 }]],
  },
  {
    id: "customData-extra-undefined-removes",
    call: "base",
    element: full({ customData: { a: 1 } }),
    extra: [["customData"]],
  },
  {
    id: "customData-first-key-keeps-position",
    call: "base",
    element: { customData: { a: 1 }, ...full() },
  },
];

const dimensionCases = () => [
  { id: "negative-width", call: "base", element: full({ width: -40 }) },
  { id: "negative-height", call: "base", element: full({ height: -30 }) },
  { id: "negative-both", call: "base", element: full({ width: -40, height: -30 }) },
  { id: "negative-fractional", call: "base", element: full({ x: 0.1, width: -0.2 }) },
  { id: "negative-width-no-x", call: "base", element: without(full({ width: -40 }), "x") },
  { id: "negative-width-null-x", call: "base", element: full({ x: null, width: -40 }) },
  { id: "negative-width-string", call: "base", element: full({ width: "-40" }) },
  { id: "negative-width-string-x", call: "base", element: full({ x: "10", width: -40 }) },
  { id: "negative-width-string-x-not-numeric", call: "base", element: full({ x: "ten", width: -40 }) },
  { id: "negative-width-array", call: "base", element: full({ width: [-40] }) },
  { id: "negative-width-array-x", call: "base", element: full({ x: [5], width: -40 }) },
  { id: "negative-width-object-x", call: "base", element: full({ x: { v: 1 }, width: -40 }) },
  { id: "negative-width-true-x", call: "base", element: full({ x: true, width: -40 }) },
  { id: "negative-width-empty-string-x", call: "base", element: full({ x: "", width: -40 }) },
  { id: "negative-width-hex-string", call: "base", element: full({ width: "-0x10" }) },
  { id: "negative-width-string-infinity", call: "base", element: full({ width: "-Infinity" }) },
  { id: "negative-width-string-padded", call: "base", element: full({ width: " \n-8e1 " }) },
  { id: "negative-width-nested-array", call: "base", element: full({ width: [["-3"]] }) },
  { id: "negative-width-two-element-array", call: "base", element: full({ width: [-3, 1] }) },
  { id: "string-width-not-negative", call: "base", element: full({ width: "abc" }) },
  { id: "object-width", call: "base", element: full({ width: { w: -1 } }) },
  // ToPrimitive on a parsed object: an own `toString` key is not callable,
  // so no method gives a primitive and the comparison throws.
  { id: "object-width-own-toString-throws", call: "base", element: full({ width: { toString: 1 } }) },
  { id: "object-width-own-valueOf", call: "base", element: full({ width: { valueOf: -1 } }) },
  {
    id: "negative-width-array-own-toString-throws",
    call: "base",
    element: full({ width: [{ toString: 1 }] }),
  },
  {
    id: "negative-width-object-x-own-toString-throws",
    call: "base",
    element: full({ x: { toString: 1 }, width: -40 }),
  },
  {
    id: "object-x-own-toString-width-not-negative",
    call: "base",
    element: full({ x: { toString: 1 }, width: 40 }),
  },
  { id: "negative-zero-width", call: "base", element: full({ width: -0 }) },
  { id: "tiny-negative", call: "base", element: full({ x: 1e21, width: -5e-7 }) },
  {
    id: "negative-width-extra-x-overrides",
    call: "base",
    element: full({ width: -40 }),
    extra: [["x", 3]],
  },
  {
    id: "negative-width-extra-x-null-falls-back",
    call: "base",
    element: full({ width: -40 }),
    extra: [["x", null]],
  },
  {
    id: "extra-y-used-for-normalisation-then-wins",
    call: "base",
    element: full({ height: -10 }),
    extra: [["y", 100]],
  },
];

const orderCases = () => [
  {
    id: "unknown-keys-kept-in-place",
    call: "base",
    element: {
      future: { nested: [1, { b: 1, a: 2 }] },
      ...full(),
      futureLast: [true, null],
    },
  },
  {
    id: "missing-base-keys-appended-in-base-order",
    call: "base",
    element: { z: 1, type: "diamond", width: 5, a: 2 },
  },
  {
    id: "array-index-keys-first",
    call: "base",
    element: { b: 1, 10: "ten", 2: "two", ...full(), "01": "not an index" },
  },
  {
    id: "legacy-keys-stripped",
    call: "base",
    element: {
      strokeSharpness: "sharp",
      ...legacy("ellipse"),
      boundElementIds: ["x"],
      after: true,
    },
  },
  {
    id: "extra-keys-appended-and-override-in-place",
    call: "base",
    element: full({ name: "old" }),
    extra: [
      ["newKey", 1],
      ["name", "new"],
      ["x", 5],
    ],
  },
  {
    id: "extra-undefined-removes-element-key",
    call: "base",
    element: full({ simulatePressure: true }),
    extra: [["simulatePressure"], ["pressures", []]],
  },
  {
    id: "extra-type-overrides-type",
    call: "base",
    element: full({ type: "draw" }),
    extra: [["type", "line"]],
  },
  {
    id: "extra-type-falsy-keeps-element-type",
    call: "base",
    element: full({ type: "ellipse" }),
    extra: [["type", ""]],
  },
  {
    id: "extra-y-undefined-falls-back-then-removed",
    call: "base",
    element: full(),
    extra: [["y"]],
  },
  {
    id: "values-kept-verbatim",
    call: "base",
    element: full({
      version: "2",
      strokeWidth: "4",
      opacity: "50",
      groupIds: "not-an-array",
      locked: "yes",
      frameId: 0,
    }),
  },
];

/** A version-1 era element as old files hold it. */
const oldFile = (type, id) => ({
  id,
  type,
  x: 100,
  y: 200,
  width: -50,
  height: 80,
  angle: 0,
  strokeColor: "#000000",
  backgroundColor: "transparent",
  fillStyle: "hachure",
  strokeWidth: 1,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  strokeSharpness: "round",
  seed: 1968410350,
  version: 141,
  versionNonce: 361174001,
  isDeleted: false,
  boundElementIds: ["arrow-1"],
});

const restoreElementCases = () => [
  ...["rectangle", "diamond", "ellipse", "iframe", "embeddable"].map((type) => ({
    id: `restoreElement-legacy-${type}`,
    call: "restoreElement",
    element: oldFile(type, `old-${type}`),
  })),
  ...["rectangle", "diamond", "ellipse", "iframe", "embeddable"].map((type) => ({
    id: `restoreElement-current-${type}`,
    call: "restoreElement",
    element: full({ type, id: `cur-${type}`, customData: { keep: [1, 2] } }),
  })),
  {
    id: "restoreElement-missing-id",
    call: "restoreElement",
    element: without(oldFile("rectangle", ""), "id"),
  },
];

const buildCases = () => [
  ...minimal(),
  ...fieldTable(),
  ...roundnessCases(),
  ...boundElementsCases(),
  ...linkCases(),
  ...customDataCases(),
  ...dimensionCases(),
  ...orderCases(),
  ...restoreElementCases(),
];

// -- running upstream -----------------------------------------------------------

const toExtra = (entries = []) => {
  const extra = {};
  for (const [key, ...value] of entries) extra[key] = value.length ? value[0] : undefined;
  return extra;
};

const runCase = (up, c) => {
  up.reseed(1);
  // Each call gets its own copy: upstream may not mutate its input, but the
  // recorded input must be what was passed in.
  const element = JSON.parse(JSON.stringify(c.element));
  try {
    const restored =
      c.call === "base"
        ? up.restoreElementWithProperties(element, toExtra(c.extra))
        : up.restoreElement(element, new Map([[element.id, element]]), null);
    return { ...c, result: JSON.parse(JSON.stringify(restored)) };
  } catch (error) {
    return { ...c, error: String(error.message) };
  }
};

const buildFixture = (up, commit) => {
  const cases = buildCases();
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  const fixture = {
    description:
      "restoreElementWithProperties (call base) and restoreElement for the generic types, " +
      "packages/excalidraw/data/restore.ts, in upstream's test mode (randomId id0.., " +
      "getUpdatedTimestamp 1). Generated by tools/goldens/restore-fixtures.mjs.",
    upstream: commit,
    cases: cases.map((c) => runCase(up, c)),
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of the invisible code points the attribution gate
  // rejects (U+FEFF, U+200B, U+2028 are inputs here).
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-\uffff]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

/** Runs fn with Math.random disabled (see header). */
const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating restore fixtures");
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
    process.stderr.write(`restore-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  // appState.ts reads devicePixelRatio at load.
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    expose: EXPOSE,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("restore fixtures are out of date: run node tools/goldens/restore-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`restore fixtures up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
