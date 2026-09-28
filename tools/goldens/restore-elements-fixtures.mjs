#!/usr/bin/env node
// Restore fixtures for excali-core's scene-level passes (ex-105): upstream's
// own restoreElements and bumpElementVersions
// (packages/excalidraw/data/restore.ts:946-1138, 1150-1173), run from the
// pinned checkout under plain Node on tables of inputs.
//
//   node tools/goldens/restore-elements-fixtures.mjs            write the fixture
//   node tools/goldens/restore-elements-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/restore-elements-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/restore-elements.json (inputs in
// lib/restore-elements-cases.mjs):
//
//   { "description", "upstream", "cases": [ { id, call, elements, existing?,
//     opts?, result | error, hooks? } ] }
//
// - call "restoreElements": restoreElements(elements, existing ?? null, opts).
// - call "bumpElementVersions": bumpElementVersions(restoreElements(elements,
//   null), existing).
// - result: JSON.parse(JSON.stringify(restored)), the elements in order.
//   error: the message when the call throws.
// - hooks: in call order, what restore.ts passed to and got from the three
//   functions that need geometry or text measurement, which excali-core
//   takes from its RestoreEnv: updateElbowArrowPoints (the elbow arrow
//   fix-up, restore.ts:1076-1093), refreshTextDimensions (restore.ts:1032-1045)
//   and getStickyNoteLayout (restore.ts:931-941). Each is
//   { hook, element, other?, updates?, result }: element is the first
//   argument as passed (the arrow, text or note), other the second one's id
//   where it is an element (the text's container, the note's label; null
//   when none), updates the elbow update, result the return value (null for
//   undefined). The calls are recorded by wrapping the three call sites in
//   restore.ts at build time (probeSource below); nothing else changes.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"), where randomId() is `id${n}` and getUpdatedTimestamp() is 1
// (packages/common/src/random.ts:16, utils.ts:552); reseed(1) before each
// case restarts the ids at id0 and randomInteger() at roughjs' Random(1).
// Math.random throws while generating. Text is measured at 10 px per
// character, what upstream's test environment gives.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { buildScenesCases } from "./lib/restore-elements-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "restore-elements.json";

const ENTRY = `
export { restoreElements, bumpElementVersions } from "./packages/excalidraw/data/restore";
export { reseed } from "./packages/common/src/random";
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
export { newElementWith } from "./packages/element/src/mutateElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export {
  DEFAULT_VERTICAL_ALIGN,
  ROUNDNESS,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
} from "./packages/common/src/index";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const PROBED = ["updateElbowArrowPoints", "refreshTextDimensions", "getStickyNoteLayout"];

/**
 * restore.ts with each call of a PROBED function routed through
 * globalThis.__restoreProbe, which calls the function and records the call.
 * Each name must be called exactly once.
 */
const probeSource = (source) => {
  let patched = source;
  for (const name of PROBED) {
    const call = new RegExp(`\\b${name}\\(`, "g");
    const count = (patched.match(call) ?? []).length;
    if (count !== 1) throw new Error(`restore.ts calls ${name} ${count} times, expected 1`);
    patched = patched.replace(call, `globalThis.__restoreProbe("${name}", ${name})(`);
  }
  return patched;
};

const usage = () => {
  process.stderr.write("usage: restore-elements-fixtures.mjs [--check] [--out DIR]\n");
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

/** The recorded form of one probed call. */
const record = (name, args, result) => {
  const entry = { hook: name, element: clone(args[0]) };
  if (name === "updateElbowArrowPoints") entry.updates = clone(args[2]);
  else entry.other = args[1] ? clone(args[1].id) : null;
  entry.result = clone(result);
  return entry;
};

/** Resolves a case's builders into JSON; each builder runs after reseed(1). */
const resolveCase = (up, c) => {
  const out = { id: c.id, call: c.call };
  up.reseed(1);
  out.elements = clone(typeof c.elements === "function" ? c.elements(up) : c.elements);
  if (c.existing) {
    up.reseed(1);
    out.existing = clone(typeof c.existing === "function" ? c.existing(up) : c.existing);
  }
  if (c.opts) out.opts = c.opts;
  return out;
};

/** Runs fn with console.error silenced (upstream logs repair failures). */
const quietly = (fn) => {
  const log = console.error;
  console.error = () => {};
  try {
    return fn();
  } finally {
    console.error = log;
  }
};

const runCase = (up, c) => {
  up.reseed(1);
  const hooks = [];
  globalThis.__restoreProbe =
    (name, fn) =>
    (...args) => {
      const result = fn(...args);
      hooks.push(record(name, args, result));
      return result;
    };
  let recorded;
  try {
    const elements = clone(c.elements);
    const restored = quietly(() =>
      c.call === "bumpElementVersions"
        ? up.bumpElementVersions(up.restoreElements(elements, null), clone(c.existing))
        : up.restoreElements(elements, c.existing ? clone(c.existing) : null, c.opts),
    );
    recorded = { ...c, result: clone(restored) };
  } catch (error) {
    recorded = { ...c, error: String(error.message) };
  } finally {
    delete globalThis.__restoreProbe;
  }
  if (hooks.length) recorded.hooks = hooks;
  return recorded;
};

// Every non-ASCII code unit as a \u escape (see restore-fixtures.mjs).
const asciiJson = (fixture) => {
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

const buildFixture = (up, commit) => {
  const cases = buildScenesCases().map((c) => resolveCase(up, c));
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  const fixture = {
    description:
      "restoreElements scene-level passes and bumpElementVersions, packages/excalidraw/data/restore.ts, " +
      "in upstream's test mode (randomId id0.., getUpdatedTimestamp 1, reseed(1) before each case, text " +
      "10 px per character). Generated by tools/goldens/restore-elements-fixtures.mjs.",
    upstream: commit,
    cases: cases.map((c) => runCase(up, c)),
  };
  return asciiJson(fixture);
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
    process.stderr.write(`restore-elements-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  // appState.ts reads devicePixelRatio at load.
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    patch: { "packages/excalidraw/data/restore": probeSource },
    define: { "import.meta.env.MODE": '"test"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write(
        "restore-elements fixture is out of date: run node tools/goldens/restore-elements-fixtures.mjs\n",
      );
      process.exit(1);
    }
    process.stdout.write(`restore-elements fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
