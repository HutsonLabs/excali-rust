#!/usr/bin/env node
// Clipboard fixtures for excali-core's clipboard JSON codec (ex-113):
// upstream's own serializeAsClipboardJSON and parseClipboard
// (packages/excalidraw/clipboard.ts:143-193, 523-555), run from the pinned
// checkout under plain Node on a table of inputs.
//
//   node tools/goldens/clipboard-fixtures.mjs            write the fixture
//   node tools/goldens/clipboard-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/clipboard-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/clipboard.json:
//
//   { "description", "upstream", "serialize": [...], "parse": [...] }
//
// - serialize cases: { id, elements, files, seed, output }. `elements` is
//   the JSON text of the element array passed in (a string, so a lone
//   UTF-16 surrogate in it survives the fixture); `files` the BinaryFiles
//   object or null. `output` is serializeAsClipboardJSON({elements, files})
//   exactly as upstream returns it, after reseed(seed): every element whose
//   frameId is cleared goes through mutateElement, which draws its
//   versionNonce from randomInteger() and sets updated to
//   getUpdatedTimestamp(), 1 in test mode.
// - parse cases: { id, textPlain, isPlainPaste, result }. The call is
//   parseClipboard(await parseDataTransferEvent(event), isPlainPaste), where
//   event is a paste ClipboardEvent whose clipboardData holds one string
//   item, text/plain = textPlain (none when textPlain is null). `result`
//   lists the returned keys in order (`keys`, an undefined value included,
//   as the object has it) and their values: `elements` is upstream's array
//   with each lone surrogate as U+FFFD (excali-core's public form, see
//   crate::json; JSON cannot hold a lone surrogate), `files` is
//   { "value": files } when the key's value is defined.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"), reseed(seed) runs before each serialize case, and Math.random
// throws while generating.
//
// The elements are those of crates/excali-core/tests/fixtures/every-type
// .excalidraw (upstream's own constructors, scene-fixtures.mjs), with the
// frameId, id, fileId and extra keys each case sets.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "clipboard.json";
const SCENE = join(FIXTURES_DIR, "every-type.excalidraw");

const ENTRY = `
export {
  serializeAsClipboardJSON,
  parseClipboard,
  parseDataTransferEvent,
} from "./packages/excalidraw/clipboard";
export { reseed } from "./packages/common/src/random";
`;

// clipboard.ts imports file helpers from ./data/blob (image paste, file
// handles); the string paths serializeAsClipboardJSON and parseClipboard
// take never call them.
const STUBS = ["packages/excalidraw/data/blob"];

const usage = () => {
  process.stderr.write("usage: clipboard-fixtures.mjs [--check] [--out DIR]\n");
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

const scene = JSON.parse(readFileSync(SCENE, "utf8"));
const byId = new Map(scene.elements.map((e) => [e.id, e]));

/** A copy of the fixture element `id`, with `changes` assigned in order. */
const el = (id, changes = {}) => {
  const base = byId.get(id);
  if (!base) throw new Error(`no fixture element ${id}`);
  return Object.assign(JSON.parse(JSON.stringify(base)), changes);
};

const FILE = scene.files.file1;
const file = (id, extra = {}) => ({ ...FILE, id, ...extra });

const serializeCases = () => [
  { id: "single-rectangle-no-files", elements: [el("rect")], files: null },
  { id: "every-type-with-files", elements: scene.elements.map((e) => el(e.id)), files: scene.files },
  { id: "every-type-no-files", elements: scene.elements.map((e) => el(e.id)), files: null },
  { id: "empty", elements: [], files: null },
  { id: "empty-with-files", elements: [], files: scene.files },
  {
    id: "child-of-copied-frame-kept",
    elements: [el("frame"), el("rect", { frameId: "frame" }), el("text", { frameId: "frame" })],
    files: null,
  },
  {
    id: "child-of-copied-magicframe-kept",
    elements: [el("magic"), el("ellipse", { frameId: "magic" })],
    files: null,
  },
  {
    // getContainingFrame looks the frame up among the copied elements only
    // (clipboard.ts:150, frame.ts:436-445), so a child whose frame is not
    // copied is not found and keeps its frameId.
    id: "child-of-frame-not-copied-kept",
    elements: [el("rect", { frameId: "frame" }), el("diamond", { frameId: "missing" })],
    files: null,
  },
  {
    // The frameId resolves among the copied elements to an element that
    // is not a frame: the child is orphaned and its frameId cleared.
    id: "frameId-to-copied-non-frame-cleared",
    elements: [el("rect"), el("ellipse", { frameId: "rect" })],
    files: null,
  },
  {
    id: "several-cleared-draw-nonces-in-order",
    elements: [
      el("rect"),
      el("diamond", { frameId: "rect" }),
      el("frame"),
      el("ellipse", { frameId: "frame" }),
      el("text", { frameId: "rect", version: 7 }),
      el("arrow", { frameId: "diamond" }),
      el("elbow", { frameId: "rect" }),
    ],
    files: null,
  },
  {
    id: "frameId-to-itself-non-frame-cleared",
    elements: [el("rect", { frameId: "rect" })],
    files: null,
  },
  {
    id: "frame-frameId-to-itself-kept",
    elements: [el("frame", { frameId: "frame" })],
    files: null,
  },
  {
    id: "frameId-empty-string-kept",
    elements: [el("rect", { id: "", frameId: "" }), el("ellipse", { frameId: "" })],
    files: null,
  },
  {
    // arrayToMap keeps the last element of an id.
    id: "duplicate-id-last-is-frame-kept",
    elements: [el("rect", { id: "dup" }), el("frame", { id: "dup" }), el("ellipse", { frameId: "dup" })],
    files: null,
  },
  {
    id: "duplicate-id-last-is-not-frame-cleared",
    elements: [el("frame", { id: "dup" }), el("rect", { id: "dup" }), el("ellipse", { frameId: "dup" })],
    files: null,
  },
  {
    id: "deleted-elements-kept",
    elements: [el("rect", { isDeleted: true }), el("ellipse", { frameId: "rect", isDeleted: true })],
    files: null,
  },
  {
    id: "unknown-keys-and-order-kept-when-cleared",
    elements: [
      el("rect"),
      (() => {
        const e = el("diamond", { zExtra: { nested: [1, 2.5, "x"] } });
        const out = { frameId: "rect", "7": "index-key", ...e };
        out.frameId = "rect";
        return out;
      })(),
    ],
    files: null,
  },
  {
    id: "lone-surrogate-text-kept-when-cleared",
    elements: [el("rect"), el("text", { frameId: "rect", text: "a\ud83d", originalText: "\udc00b" })],
    files: null,
  },
  {
    id: "image-file-missing-from-files",
    elements: [el("image", { fileId: "absent" })],
    files: scene.files,
  },
  {
    id: "image-fileId-null-not-collected",
    elements: [el("image", { fileId: null })],
    files: scene.files,
  },
  {
    id: "image-fileId-empty-not-collected",
    elements: [el("image", { fileId: "" })],
    files: { "": file("") },
  },
  {
    id: "images-files-in-element-order-js-key-order",
    elements: [
      el("image", { id: "i1", fileId: "zeta" }),
      el("image", { id: "i2", fileId: "42" }),
      el("image", { id: "i3", fileId: "zeta" }),
      el("image", { id: "i4", fileId: "alpha" }),
      el("rect"),
    ],
    files: { alpha: file("alpha"), 42: file("42"), zeta: file("zeta"), unused: file("unused") },
  },
  {
    id: "files-falsy-values-skipped",
    elements: [
      el("image", { id: "i1", fileId: "zero" }),
      el("image", { id: "i2", fileId: "nul" }),
      el("image", { id: "i3", fileId: "empty" }),
      el("image", { id: "i4", fileId: "no" }),
      el("image", { id: "i5", fileId: "yes" }),
    ],
    files: { zero: 0, nul: null, empty: "", no: false, yes: "truthy" },
  },
  {
    // An inherited property (Object.prototype.toString) is truthy and is
    // assigned to the result, but a function is not JSON: the key is
    // dropped by JSON.stringify.
    id: "files-inherited-key-dropped",
    elements: [el("image", { fileId: "toString" }), el("image", { id: "i2", fileId: "constructor" })],
    files: {},
  },
  {
    // A JSON-parsed own "__proto__" key is found, but assigning it to the
    // result sets the prototype: no own key, nothing written.
    id: "files-proto-key-dropped",
    elements: [el("image", { fileId: "__proto__" }), el("image", { id: "i2", fileId: "file1" })],
    files: JSON.parse(`{"__proto__":${JSON.stringify(FILE)},"file1":${JSON.stringify(FILE)}}`),
  },
  {
    // The detached copy comes from deepCopyElement, which skips own keys
    // "shape" and "canvas" at depth 0 (packages/element/src/duplicate.ts:
    // 640-646); an element written as given keeps them, and so does a
    // nested object.
    id: "shape-and-canvas-dropped-only-when-cleared",
    elements: [
      el("rect", { shape: 1, canvas: { a: 1 } }),
      el("diamond", { frameId: "rect", canvas: 1, shape: 2, customData: { shape: 3, canvas: 4 } }),
      el("frame", { shape: null }),
      el("ellipse", { frameId: "frame", canvas: "c" }),
      el("text", { frameId: "ellipse", shape: [1], zAfter: true }),
    ],
    files: null,
  },
];

const parseCases = () => {
  const rect = el("rect");
  const clip = (type, extra = {}) => JSON.stringify({ type, elements: [rect], ...extra });
  const cases = [
    // clipboard.test.ts "should parse JSON as plaintext if not
    // excalidraw-api/clipboard data"
    { id: "upstream-plain-number", textPlain: "123" },
    { id: "upstream-plain-array", textPlain: "[123]" },
    { id: "upstream-plain-object", textPlain: JSON.stringify({ val: 42 }) },
    // clipboard.test.ts "should parse valid excalidraw JSON if inside text/plain"
    { id: "upstream-clipboard-json", textPlain: clip("excalidraw/clipboard", { files: undefined }) },
    { id: "type-excalidraw", textPlain: clip("excalidraw") },
    { id: "type-excalidraw-api-clipboard", textPlain: clip("excalidraw-api/clipboard") },
    { id: "type-excalidrawlib-is-text", textPlain: clip("excalidrawlib") },
    { id: "type-case-sensitive", textPlain: clip("Excalidraw") },
    { id: "type-missing", textPlain: JSON.stringify({ elements: [rect] }) },
    { id: "type-not-string", textPlain: JSON.stringify({ type: ["excalidraw"], elements: [] }) },
    { id: "elements-missing", textPlain: JSON.stringify({ type: "excalidraw/clipboard" }) },
    { id: "elements-object", textPlain: JSON.stringify({ type: "excalidraw/clipboard", elements: {} }) },
    { id: "elements-null", textPlain: JSON.stringify({ type: "excalidraw", elements: null }) },
    { id: "elements-string", textPlain: JSON.stringify({ type: "excalidraw", elements: "[]" }) },
    { id: "api-elements-not-array", textPlain: JSON.stringify({ type: "excalidraw-api/clipboard", elements: 1 }) },
    { id: "elements-empty", textPlain: JSON.stringify({ type: "excalidraw/clipboard", elements: [] }) },
    {
      id: "elements-any-items",
      textPlain: JSON.stringify({ type: "excalidraw", elements: [1, "a", null, [], { type: "nope" }] }),
    },
    { id: "files-object", textPlain: clip("excalidraw/clipboard", { files: scene.files }) },
    { id: "files-null", textPlain: clip("excalidraw/clipboard", { files: null }) },
    { id: "files-zero", textPlain: clip("excalidraw", { files: 0 }) },
    { id: "files-before-elements", textPlain: `{"files":{},"type":"excalidraw","elements":[]}` },
    { id: "duplicate-type-last-wins", textPlain: `{"type":"excalidrawlib","elements":[],"type":"excalidraw"}` },
    { id: "duplicate-type-last-loses", textPlain: `{"type":"excalidraw","elements":[],"type":"excalidrawlib"}` },
    { id: "whole-scene-file", textPlain: readFileSync(SCENE, "utf8") },
    { id: "json-null", textPlain: "null" },
    { id: "json-true", textPlain: "true" },
    { id: "json-string", textPlain: JSON.stringify("excalidraw") },
    { id: "invalid-json", textPlain: '{"type":"excalidraw","elements":[' },
    { id: "trailing-garbage", textPlain: `${clip("excalidraw")}x` },
    { id: "plain-text", textPlain: "  hello\n world \t" },
    { id: "empty-string", textPlain: "" },
    { id: "whitespace-only", textPlain: " \n\t\u00a0\ufeff\u2028\u3000 " },
    { id: "no-text-plain-item", textPlain: null },
    { id: "trimmed-json", textPlain: `\n\t ${clip("excalidraw/clipboard")} \r\n` },
    { id: "trimmed-js-whitespace-json", textPlain: `\ufeff\u00a0${clip("excalidraw/clipboard")}\u2029\u3000` },
    {
      id: "lone-surrogate-elements",
      textPlain: `{"type":"excalidraw/clipboard","elements":[{"id":"t","text":"a\\ud83d","n":1.50,"big":1e21,"small":1e-7,"neg":-0}]}`,
    },
    { id: "nested-keys-js-order", textPlain: `{"type":"excalidraw","elements":[{"b":1,"2":2,"a":3,"1":4}]}` },
    {
      // JSON.parse reads a literal beyond the f64 range as +-Infinity, which
      // JSON.stringify writes as null (and so does this fixture).
      id: "number-overflow-elements",
      textPlain:
        `{"type":"excalidraw","elements":[{"x":1e400,"y":-1E+400,"n":[1${"0".repeat(400)},-0.5e309],` +
        `"small":1e-400,"s":"1e400"}]}`,
    },
    { id: "number-overflow-invalid-json-is-text", textPlain: `{"type":"excalidraw","elements":[01e400]}` },
  ];
  // The same inputs as a plain paste (Ctrl+Shift+V): elements also come
  // back as JSON.stringify(elements, null, 2) text.
  const plain = cases.map((c) => ({ ...c, id: `${c.id}-plain-paste`, isPlainPaste: true }));
  return [...cases.map((c) => ({ ...c, isPlainPaste: false })), ...plain];
};

// -- running upstream -----------------------------------------------------------

const TEST_MODE_TIMESTAMP = 1;

const runSerialize = (up, c, index) => {
  const seed = 1000 + index;
  up.reseed(seed);
  const text = JSON.stringify(c.elements);
  const elements = JSON.parse(text);
  const files = c.files === null ? null : JSON.parse(JSON.stringify(c.files));
  const output = up.serializeAsClipboardJSON({ elements, files });
  return { id: c.id, elements: text, files: c.files, seed, output };
};

const LONE_SURROGATE_ESCAPE = /\\ud[89a-f][0-9a-f]{2}/g;

const pasteEvent = (textPlain) => {
  const items = textPlain === null ? [] : [{ kind: "string", type: "text/plain" }];
  return {
    type: "paste",
    clipboardData: {
      items,
      getData: (type) => (type === "text/plain" && textPlain !== null ? textPlain : ""),
    },
  };
};

const runParse = async (up, c) => {
  const list = await up.parseDataTransferEvent(pasteEvent(c.textPlain));
  const data = await up.parseClipboard(list, c.isPlainPaste);
  const result = { keys: Object.keys(data) };
  for (const key of result.keys) {
    const value = data[key];
    if (key === "elements") {
      result.elements = JSON.parse(JSON.stringify(value).replace(LONE_SURROGATE_ESCAPE, "\\ufffd"));
    } else if (key === "files") {
      if (value !== undefined) result.files = { value };
    } else if (value !== undefined) {
      result[key] = value;
    }
  }
  return { id: c.id, textPlain: c.textPlain, isPlainPaste: c.isPlainPaste, result };
};

const unique = (cases) => {
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  return cases;
};

const buildFixture = async (up, commit) => {
  const serialize = unique(serializeCases()).map((c, i) => runSerialize(up, c, i));
  const parse = [];
  for (const c of unique(parseCases())) parse.push(await runParse(up, c));
  const fixture = {
    description:
      "serializeAsClipboardJSON and parseClipboard (text/plain), packages/excalidraw/clipboard.ts, " +
      `in upstream's test mode (getUpdatedTimestamp ${TEST_MODE_TIMESTAMP}, reseed(seed) per ` +
      "serialize case). Generated by tools/goldens/clipboard-fixtures.mjs.",
    upstream: commit,
    serialize,
    parse,
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of the invisible code points the attribution gate
  // rejects (U+FEFF, U+2028 are inputs here).
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-\uffff]/g,
    (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

/** Runs fn with Math.random disabled (see header). */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating clipboard fixtures");
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
    process.stderr.write(`clipboard-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = await deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("clipboard fixtures are out of date: run node tools/goldens/clipboard-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`clipboard fixtures up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
