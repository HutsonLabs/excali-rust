#!/usr/bin/env node
// Library fixtures for excali-core's `.excalidrawlib` port (ex-108):
// upstream's own parseLibraryJSON (packages/excalidraw/data/blob.ts:218-228,
// which checks isValidLibrary, json.ts:128-135, and calls
// restoreLibraryItems, restore.ts:1374-1415), serializeLibraryAsJSON
// (json.ts:137-145), mergeLibraryItems (library.ts:122-157) and
// getLibraryItemsHash (library.ts:594-605), run from the pinned checkout
// under plain Node.
//
//   node tools/goldens/library-fixtures.mjs            write the fixture
//   node tools/goldens/library-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/library-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/library.json:
//
//   { "description", "upstream", "source", "parse": [...], "catalogue": [...],
//     "merge": [...], "hash": [...] }
//
// - parse cases: { id, input, defaultStatus, output | error }. `input` is the
//   file text; the call is parseLibraryJSON(input, defaultStatus) after
//   reseed(1). `output` is serializeLibraryAsJSON(items) exactly as upstream
//   returns it (the items restoreLibraryItems gave, written with `source`);
//   `error` the message of what the call threw. `file` names the input's
//   path from the repository root when it is a fixture file. When arrow
//   bindings reached the legacy migration, which needs element geometry
//   (restore.ts:362-418, excali-core's RestoreEnv::migrate_legacy_binding,
//   task ex-116), `geometry` counts the binding ends that did and
//   `outputWithoutGeometry` is the output with that computation failing
//   (upstream then drops the binding, restore.ts:423-427): the case run
//   again with LinearElementEditor.getPointAtIndexGlobalCoordinates, the
//   branch's first call, throwing.
// - catalogue cases: { id, file, version, items, output_sha256, geometry?,
//   output_sha256_without_geometry? } for every
//   library of fixtures/libraries (fixtures/manifest.json, ex-003): the
//   gzipped file is read, parsed as above with defaultStatus "published" (an
//   import from libraries.excalidraw.com, library.ts:754-760), and the sha256
//   of the serialized output recorded with the item count, and
//   as for parse cases the output without geometry.
// - merge cases: { id, local, other, output }: parseLibraryJSON of the two
//   inputs (each after reseed(1)), mergeLibraryItems(local, other), written
//   with serializeLibraryAsJSON.
// - hash cases: { id, input, hash }: getLibraryItemsHash of the parsed items.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test": randomId() gives id0, id1, ..., restarted by reseed; the
// timestamp getUpdatedTimestamp() gives is 1). restoreLibraryItems reads
// Date.now() for `created`, which returns 1 here too, so excali-core's
// restore::TestEnv answers both. window.EXCALIDRAW_EXPORT_SOURCE is the
// `source` below. Math.random throws while generating.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { gunzipSync } from "node:zlib";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "library.json";
const SOURCE = "https://excalidraw.com";
const UPSTREAM_FIXTURE = "fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib";
const MANIFEST = join(REPO_ROOT, "fixtures", "manifest.json");

const ENTRY = `
export { parseLibraryJSON } from "./packages/excalidraw/data/blob";
export { serializeLibraryAsJSON } from "./packages/excalidraw/data/json";
export { mergeLibraryItems, getLibraryItemsHash } from "./packages/excalidraw/data/library";
export { reseed } from "./packages/common/src/random";
export { LinearElementEditor } from "./packages/element/src/linearElementEditor";
`;

// blob.ts and library.ts import browser and React code (file dialogs, image
// codecs, the scene exporter behind library item previews, jotai atoms)
// that parseLibraryJSON, mergeLibraryItems and getLibraryItemsHash never
// call. library.ts creates a jotai atom while it loads, so editor-jotai is a
// shim whose `atom` returns a plain object.
const STUBS = [
  "react",
  "jotai",
  "jotai-scope",
  "pica",
  "image-blob-reduce",
  "pako",
  "browser-fs-access",
  "png-chunk-text",
  "png-chunks-extract",
  "png-chunks-encode",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/data/image",
  "packages/excalidraw/data/encode",
  "packages/excalidraw/hooks/useLibraryItemSvg",
  "packages/excalidraw/scene",
  "packages/excalidraw/scene/export",
];
const SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
};

const usage = () => {
  process.stderr.write("usage: library-fixtures.mjs [--check] [--out DIR]\n");
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

/** A library file's text: the envelope with `fields` after it. */
const lib = (fields, envelope = { type: "excalidrawlib", version: 2 }) =>
  JSON.stringify({ ...envelope, ...fields }, null, 2);
const v1 = (library) => lib({ library }, { type: "excalidrawlib", version: 1 });
const v2 = (libraryItems) => lib({ libraryItems });

/** A complete element as upstream writes one, with `fields` over it. */
const el = (id, fields = {}) => ({
  id,
  type: "rectangle",
  x: 10,
  y: 20,
  width: 100,
  height: 50,
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
  index: "a0",
  roundness: null,
  seed: 7,
  version: 3,
  versionNonce: 1234,
  isDeleted: false,
  boundElements: null,
  updated: 1700000000000,
  link: null,
  locked: false,
  ...fields,
});

const item = (id, elements, fields = {}) => ({
  id,
  status: "published",
  elements,
  created: 1600000000000,
  ...fields,
});

const parseCases = () => [
  { id: "upstream-fixture-library-v1", file: UPSTREAM_FIXTURE },
  { id: "upstream-fixture-library-v1-published", file: UPSTREAM_FIXTURE, defaultStatus: "published" },
  // restore.test.ts "restoreLibraryItems creation timestamps": `created`
  // missing, 123 and null on the elements; the item's own created kept.
  {
    id: "upstream-creation-timestamps-v1",
    input: v1([[el("a", { created: undefined }), el("b", { created: 123, index: "a1" }), el("c", { created: null, index: "a2" })]]),
  },
  {
    id: "upstream-creation-timestamps-v2",
    input: v2([
      {
        id: "library-item",
        status: "unpublished",
        created: 456,
        elements: [el("a"), el("b", { created: 123, index: "a1" }), el("c", { created: null, index: "a2" })],
      },
    ]),
  },

  // envelope: isValidLibrary and `libraryItems || library`
  { id: "v2-empty", input: v2([]) },
  { id: "v1-empty", input: v1([]) },
  { id: "version-3-invalid", input: lib({ libraryItems: [] }, { type: "excalidrawlib", version: 3 }) },
  { id: "version-string-invalid", input: lib({ libraryItems: [] }, { type: "excalidrawlib", version: "2" }) },
  { id: "version-missing-invalid", input: lib({ libraryItems: [] }, { type: "excalidrawlib" }) },
  { id: "version-1.0-valid", input: `{"type":"excalidrawlib","version":1.0,"library":[]}` },
  { id: "type-excalidraw-invalid", input: lib({ libraryItems: [] }, { type: "excalidraw", version: 2 }) },
  { id: "type-missing-invalid", input: JSON.stringify({ version: 2, libraryItems: [] }) },
  { id: "json-null-invalid", input: "null" },
  { id: "json-array-invalid", input: "[]" },
  { id: "json-string-invalid", input: '"excalidrawlib"' },
  { id: "json-syntax-error", input: '{"type":"excalidrawlib",' },
  { id: "json-empty", input: "" },
  { id: "no-items-key", input: lib({}) },
  { id: "v2-file-with-library-key", input: lib({ library: [[el("a")]] }) },
  { id: "v1-file-with-libraryItems-key", input: lib({ libraryItems: [item("i", [el("a")])] }, { type: "excalidrawlib", version: 1 }) },
  { id: "both-keys-libraryItems-wins", input: lib({ libraryItems: [item("i", [el("a")])], library: [[el("b")]] }) },
  { id: "both-keys-empty-libraryItems-wins", input: lib({ libraryItems: [], library: [[el("b")]] }) },
  { id: "libraryItems-null-falls-back", input: lib({ libraryItems: null, library: [[el("b")]] }) },
  { id: "libraryItems-null-library-missing", input: lib({ libraryItems: null }) },
  { id: "library-null-throws", input: lib({ libraryItems: 0, library: null }) },
  { id: "library-false-throws", input: lib({ library: false }) },
  { id: "library-empty-string", input: lib({ library: "" }) },
  { id: "libraryItems-object-throws", input: lib({ libraryItems: { a: [] } }) },
  { id: "libraryItems-number-throws", input: lib({ libraryItems: 5 }) },
  { id: "libraryItems-true-throws", input: lib({ libraryItems: true }) },
  { id: "libraryItems-string-iterates", input: lib({ libraryItems: "abé" }) },
  {
    id: "envelope-extra-keys-ignored",
    input: lib({ source: "https://example.com", extra: 1, libraryItems: [item("i", [el("a")])] }),
  },

  // items
  { id: "item-null-throws", input: v2([null]) },
  { id: "item-null-after-good-throws", input: v2([item("i", [el("a")]), null]) },
  { id: "items-scalar-dropped", input: v2([1, "s", true, false, 0, item("i", [el("a")])]) },
  { id: "item-empty-object-dropped", input: v2([{}]) },
  { id: "item-elements-empty-dropped", input: v2([item("i", [])]) },
  { id: "item-elements-null-dropped", input: v2([item("i", null)]) },
  { id: "item-elements-zero-dropped", input: v2([item("i", 0)]) },
  { id: "item-elements-string-throws", input: v2([item("i", "ab")]) },
  { id: "item-elements-object-throws", input: v2([item("i", { 0: el("a") })]) },
  { id: "item-elements-number-throws", input: v2([item("i", 5)]) },
  { id: "item-element-null-throws", input: v2([item("i", [el("a"), null])]) },
  { id: "item-elements-scalars-dropped", input: v2([item("i", [1, "s", true, [], [el("x")]])]) },
  { id: "item-elements-scalars-and-element", input: v2([item("i", [1, "s", el("a"), true])]) },
  { id: "item-all-deleted-dropped", input: v2([item("i", [el("a", { isDeleted: true })])]) },
  {
    id: "item-some-deleted-filtered",
    input: v2([item("i", [el("a"), el("b", { isDeleted: true, index: "a1" }), el("c", { index: "a2" })])]),
  },
  { id: "item-selection-dropped", input: v2([item("i", [el("s", { type: "selection" }), el("a", { index: "a1" })])]) },
  { id: "item-unknown-type-dropped", input: v2([item("i", [el("u", { type: "hexagon" })])]) },
  {
    // restoreElement throws on a non-string link; restoreElements drops the
    // element (restore.ts:977-979).
    id: "item-element-restore-throws-dropped",
    input: v2([item("i", [el("a", { link: 5 }), el("b", { index: "a1" })])]),
  },
  { id: "item-missing-id-status-created", input: v2([{ elements: [el("a")] }]) },
  { id: "item-falsy-id-status-created", input: v2([{ id: "", status: "", created: 0, elements: [el("a")] }]) },
  { id: "item-null-id-status-created", input: v2([{ id: null, status: null, created: null, elements: [el("a")] }]) },
  { id: "item-missing-fields-published", input: v2([{ elements: [el("a")] }]), defaultStatus: "published" },
  {
    id: "item-odd-values-kept",
    input: v2([{ id: 5, status: "draft", created: "yesterday", name: 7, error: false, elements: [el("a")] }]),
  },
  {
    id: "item-name-error-kept",
    input: v2([item("i", [el("a")], { name: "Stick figure é😀", error: "failed to load" })]),
  },
  {
    id: "item-unknown-keys-and-order-kept",
    input: `{"type":"excalidrawlib","version":2,"libraryItems":[{"zeta":1,"elements":${JSON.stringify([el("a")])},"2":"two","created":5,"alpha":{"nested":[1,2.5]},"1":"one","status":"unpublished"}]}`,
  },
  {
    id: "item-key-order-id-last",
    input: `{"type":"excalidrawlib","version":2,"libraryItems":[{"elements":${JSON.stringify([el("a")])},"created":5,"status":"published","id":"last"}]}`,
  },
  {
    id: "item-lone-surrogate-name",
    input: `{"type":"excalidrawlib","version":2,"libraryItems":[{"id":"i\\ud83d","name":"half \\udc00","status":"published","created":1,"elements":${JSON.stringify([el("a")])}}]}`,
  },
  {
    id: "v1-arrays-and-objects-mixed",
    input: v1([[el("a")], item("obj", [el("b")]), [], [el("c", { isDeleted: true })], [el("d")]]),
  },
  { id: "v1-published-default", input: v1([[el("a")], [el("b")]]), defaultStatus: "published" },

  // restoreElements(elements, null) per item
  {
    id: "elements-restored-legacy",
    input: v1([[{ type: "rectangle", id: "A", x: 1, y: 2, width: 30, height: 40, strokeSharpness: "round", boundElementIds: ["x"] }]]),
  },
  { id: "elements-duplicate-ids-renamed", input: v2([item("i", [el("x"), el("x", { index: "a1" }), el("x", { index: "a2" })])]) },
  {
    id: "elements-duplicate-ids-across-items-kept",
    input: v2([item("i", [el("x")]), item("j", [el("x")])]),
  },
  { id: "elements-missing-ids", input: v2([item("i", [el(undefined), el("", { index: "a1" })])]) },
  { id: "elements-missing-indices-synced", input: v2([item("i", [el("a", { index: null }), el("b", { index: null }), el("c", { index: undefined })])]) },
  { id: "elements-invalid-indices-synced", input: v2([item("i", [el("a", { index: "a2" }), el("b", { index: "a1" }), el("c", { index: "zz" })])]) },
  { id: "elements-indices-kept", input: v2([item("i", [el("a", { index: "a0" }), el("b", { index: "a5" }), el("c", { index: "b0" })])]) },
  {
    id: "elements-deleted-take-indices",
    input: v2([item("i", [el("a", { index: null }), el("b", { index: null, isDeleted: true }), el("c", { index: null })])]),
  },
  {
    id: "elements-arrow-binding-to-sibling",
    input: v2([
      item("i", [
        el("box"),
        el("arr", {
          type: "arrow",
          index: "a1",
          points: [
            [0, 0],
            [50, 0],
          ],
          startBinding: { elementId: "box", mode: "orbit", fixedPoint: [0.5, 0.5] },
          endBinding: { elementId: "gone", mode: "orbit", fixedPoint: [0.5, 0.5] },
          startArrowhead: null,
          endArrowhead: "arrow",
        }),
      ]),
    ]),
  },
  {
    // A binding saved before bindings had a mode, to an element of the
    // item: migrated with geometry (restore.ts:362-418).
    id: "elements-legacy-binding-migrated",
    input: v2([
      item("i", [
        el("box"),
        el("arr", {
          type: "arrow",
          index: "a1",
          x: 150,
          y: 45,
          width: 80,
          height: 0,
          points: [
            [0, 0],
            [80, 0],
          ],
          startBinding: { elementId: "box", focus: 0, gap: 5 },
          endBinding: null,
          startArrowhead: null,
          endArrowhead: "arrow",
        }),
      ]),
    ]),
  },
  {
    id: "elements-text-and-freedraw",
    input: v2([
      item("i", [
        el("t", {
          type: "text",
          text: "hi\nthere",
          originalText: "hi\nthere",
          fontSize: 20,
          fontFamily: 1,
          textAlign: "left",
          verticalAlign: "top",
          containerId: null,
          lineHeight: 1.25,
        }),
        el("f", {
          type: "freedraw",
          index: "a1",
          points: [
            [0, 0],
            [3, 4],
            [6, 1],
          ],
          pressures: [],
          simulatePressure: true,
        }),
        el("l", { type: "draw", index: "a2", points: [[1, 1], [11, 6]] }),
      ]),
    ]),
  },
  {
    id: "elements-unknown-keys-and-surrogates",
    input: `{"type":"excalidrawlib","version":2,"libraryItems":[{"id":"i","status":"published","created":1,"elements":[${JSON.stringify(
      el("a", { customData: { k: [1, { n: null }] }, zUnknown: "é" }),
    ).replace(/}$/, ',"loneText":"x\\ud800y"}')}]}]}`,
  },
];

const catalogueCases = () => {
  const manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
  return manifest.files
    .filter((f) => f.source === "libraries" && f.path.endsWith(".excalidrawlib.gz"))
    .map((f) => `fixtures/${f.path}`)
    .sort();
};

const mergeCases = () => {
  const a = [el("a1"), el("a2", { index: "a1", versionNonce: 11 })];
  const b = [el("b1", { versionNonce: 21 })];
  const c = [el("c1", { versionNonce: 31 }), el("c2", { index: "a1", versionNonce: 32 }), el("c3", { index: "a2", versionNonce: 33 })];
  const local = v2([item("A", a), item("B", b)]);
  return [
    { id: "identical-not-added", local, other: v2([item("A", a)]) },
    { id: "same-elements-other-item-id-not-added", local, other: v2([item("other", a, { name: "renamed" })]) },
    { id: "nonce-differs-added", local, other: v2([item("A2", [a[0], { ...a[1], versionNonce: 12 }])]) },
    { id: "id-differs-added", local, other: v2([item("A3", [a[0], { ...a[1], id: "a9" }])]) },
    { id: "order-differs-added", local, other: v2([item("A4", [{ ...a[1], index: "a0" }, { ...a[0], index: "a1" }])]) },
    { id: "prefix-added", local, other: v2([item("A5", [a[0]])]) },
    { id: "longer-added", local, other: v2([item("A6", [...a, el("a3", { index: "a2" })])]) },
    { id: "version-differs-not-added", local, other: v2([item("A7", [{ ...a[0], version: 99 }, a[1]])]) },
    { id: "deleted-element-filtered-then-equal", local, other: v2([item("A8", [...a, el("dead", { index: "a2", isDeleted: true })])]) },
    {
      id: "several-new-prepended-in-order",
      local,
      other: v2([item("C", c), item("A", a), item("D", [el("d1", { versionNonce: 41 })]), item("B", b)]),
    },
    { id: "duplicates-within-other-both-added", local, other: v2([item("C", c), item("C-again", c)]) },
    { id: "empty-local", local: v2([]), other: v2([item("C", c), item("A", a)]) },
    { id: "empty-other", local, other: v2([]) },
    { id: "v1-into-v2", local, other: v1([a, c]) },
  ];
};

const hashCases = () => [
  { id: "empty", input: v2([]) },
  { id: "one", input: v2([item("A", [el("a")])]) },
  { id: "two-order-independent-1", input: v2([item("A", [el("a")]), item("B", [el("b", { versionNonce: 99 })])]) },
  { id: "two-order-independent-2", input: v2([item("B", [el("b", { versionNonce: 99 })]), item("A", [el("a")])]) },
  { id: "named", input: v2([item("A", [el("a")], { name: "Arrow é😀" })]) },
  { id: "name-falsy", input: v2([item("A", [el("a")], { name: "" }), item("B", [el("b")], { name: 0 })]) },
  { id: "name-number", input: v2([item("A", [el("a")], { name: 42 })]) },
  { id: "id-number", input: v2([item(7, [el("a")])]) },
  {
    id: "large-nonces-wrap",
    input: v2([
      item("A", [
        el("a", { versionNonce: 2147483647 }),
        el("b", { versionNonce: 2000000000, index: "a1" }),
        el("c", { versionNonce: 1999999999, index: "a2" }),
        el("d", { versionNonce: 0, index: "a3" }),
        el("e", { versionNonce: 1, index: "a4" }),
        el("f", { versionNonce: 123456789, index: "a5" }),
      ]),
    ]),
  },
  { id: "fractional-nonce", input: v2([item("A", [el("a", { versionNonce: 0.5 })])]) },
  {
    id: "lone-surrogate-name",
    input: `{"type":"excalidrawlib","version":2,"libraryItems":[{"id":"i","name":"x\\ud83d","status":"published","created":1,"elements":${JSON.stringify([el("a")])}}]}`,
  },
  { id: "sort-by-code-units", input: v2([item("é", [el("a")]), item("😀", [el("b")]), item("｡", [el("c")]), item("Z", [el("d")])]) },
  { id: "upstream-fixture-library", file: UPSTREAM_FIXTURE },
];

// -- running upstream -----------------------------------------------------------

const readInput = (c) => {
  if (c.input !== undefined) return c.input;
  const path = join(REPO_ROOT, c.file);
  const data = readFileSync(path);
  return (c.file.endsWith(".gz") ? gunzipSync(data) : data).toString("utf8");
};

const parse = (up, text, defaultStatus) => {
  up.reseed(1);
  return defaultStatus === undefined ? up.parseLibraryJSON(text) : up.parseLibraryJSON(text, defaultStatus);
};

const runParse = (up, c) => {
  const input = readInput(c);
  const out = { id: c.id };
  if (c.file !== undefined) out.file = c.file;
  out.input = input;
  out.defaultStatus = c.defaultStatus ?? "unpublished";
  try {
    out.output = up.serializeLibraryAsJSON(parse(up, input, c.defaultStatus));
  } catch (error) {
    out.error = error.message;
    return out;
  }
  const probe = withoutGeometry(up, () => up.serializeLibraryAsJSON(parse(up, input, c.defaultStatus)));
  if (probe.ends) {
    out.geometry = probe.ends;
    out.outputWithoutGeometry = probe.result;
  }
  return out;
};

/**
 * Runs fn with the legacy binding migration failing at its first geometry
 * call (see header): { ends, result }, ends the number of binding ends that
 * reached it.
 */
const withoutGeometry = (up, fn) => {
  const editor = up.LinearElementEditor;
  const original = editor.getPointAtIndexGlobalCoordinates;
  let ends = 0;
  editor.getPointAtIndexGlobalCoordinates = () => {
    ends++;
    throw new Error("geometry probe");
  };
  try {
    const result = fn();
    return { ends, result };
  } finally {
    editor.getPointAtIndexGlobalCoordinates = original;
  }
};

const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");

const runCatalogue = (up, file) => {
  const input = readInput({ file });
  const items = parse(up, input, "published");
  const output = up.serializeLibraryAsJSON(items);
  const out = {
    id: file.replace(/^fixtures\/libraries\//, "").replace(/\.excalidrawlib\.gz$/, ""),
    file,
    version: JSON.parse(input).version,
    items: items.length,
    output_sha256: sha256(output),
  };
  const probe = withoutGeometry(up, () => up.serializeLibraryAsJSON(parse(up, input, "published")));
  if (probe.ends) {
    out.geometry = probe.ends;
    out.output_sha256_without_geometry = sha256(probe.result);
  }
  return out;
};

const runMerge = (up, c) => {
  const local = parse(up, c.local);
  const other = parse(up, c.other);
  return { id: c.id, local: c.local, other: c.other, output: up.serializeLibraryAsJSON(up.mergeLibraryItems(local, other)) };
};

const runHash = (up, c) => {
  const input = readInput(c);
  const out = { id: c.id };
  if (c.file !== undefined) out.file = c.file;
  out.input = input;
  out.hash = up.getLibraryItemsHash(parse(up, input));
  return out;
};

const unique = (cases) => {
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  return cases;
};

const buildFixture = (up, commit) => {
  const fixture = {
    description:
      "parseLibraryJSON, serializeLibraryAsJSON, mergeLibraryItems and getLibraryItemsHash " +
      "(packages/excalidraw/data/blob.ts, json.ts, restore.ts, library.ts) in upstream's test mode " +
      "(randomId id0, id1, ...; getUpdatedTimestamp and Date.now 1; reseed(1) before each parse). " +
      "Generated by tools/goldens/library-fixtures.mjs.",
    upstream: commit,
    source: SOURCE,
    parse: unique(parseCases()).map((c) => runParse(up, c)),
    catalogue: unique(catalogueCases().map((file) => runCatalogue(up, file))),
    merge: unique(mergeCases()).map((c) => runMerge(up, c)),
    hash: unique(hashCases()).map((c) => runHash(up, c)),
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of invisible code points the attribution gate rejects.
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-￿]/g,
    (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

/**
 * Runs fn with Math.random disabled and Date.now fixed at 1 (see header).
 * console.error is silenced: restoreElements logs each element it drops
 * and each binding it cannot repair, which the inputs do on purpose.
 */
const deterministic = (fn) => {
  const random = Math.random;
  const now = Date.now;
  const error = console.error;
  Math.random = () => {
    throw new Error("Math.random called while generating library fixtures");
  };
  Date.now = () => 1;
  console.error = () => {};
  try {
    return fn();
  } finally {
    Math.random = random;
    Date.now = now;
    console.error = error;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`library-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  globalThis.window.EXCALIDRAW_EXPORT_SOURCE = SOURCE;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("library fixtures are out of date: run node tools/goldens/library-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`library fixtures up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
