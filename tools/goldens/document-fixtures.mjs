#!/usr/bin/env node
// Round-trip goldens for every scene-bearing file of upstream's test
// fixtures (packages/excalidraw/tests/fixtures), for excali-core's D1
// conformance test (ex-g101): upstream's own file loading and saving, run
// from the pinned checkout under plain Node.
//
//   node tools/goldens/document-fixtures.mjs            write the fixture
//   node tools/goldens/document-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/document-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/document-round-trip.json:
//
//   { "description", "upstream", "source", "cases": [...] }
//
// Each case is { id, files, kind, input, output, reload }:
//
// - `files`: the fixture files (paths from the repository root, under
//   fixtures/upstream/) the case covers;
// - `kind` "scene": `input` is the scene text upstream's loader parses. For a
//   .ts fixture it is JSON.stringify(fixture, null, 2) of the object
//   upstream's test module exports (diagramFixture.ts; for elementFixture.ts
//   a scene of every exported element in source order); for an embedded
//   PNG or SVG it is what upstream's parseFileContents (data/blob.ts:32-80,
//   decodePngMetadata and decodeSvgBase64Payload) returns for the file.
//   `output` is serializeAsJSON(elements, appState, files, "local")
//   (data/json.ts:52-75) of what loadFromBlob (data/blob.ts:137-216) gives
//   for the file: restoreElements with repairBindings and
//   deleteInvisibleElements, restoreAppState of the file's exported
//   appState, with no local state. `reload` is the same for `output` loaded
//   as a .excalidraw file.
// - `kind` "library": loadLibraryFromBlob (data/blob.ts:230-235,
//   parseLibraryJSON and restoreLibraryItems) of the file, written with
//   serializeLibraryAsJSON (data/json.ts:137-145); `reload` the same for
//   `output`.
//
// The "diagramFixture-unknown-keys" case is diagramFixture with an unknown
// key added to its first element and at the top level. Upstream keeps the
// element key (restore.ts:500-508 spreads the element) and drops the
// top-level one (serializeAsJSON writes a fixed object); excali-core keeps
// both (site/content/architecture/file-format.md), and its test checks
// upstream's bytes with the top-level key removed.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test": randomId() gives id0, id1, ..., restarted by reseed(1) before
// each load; getUpdatedTimestamp() gives 1), Date.now returns 1, and
// Math.random throws while generating. window.EXCALIDRAW_EXPORT_SOURCE is
// the `source` below. Node has no FileReader, which parseFileContents uses
// to read a Blob as text (`"text" in Blob` is false for the constructor);
// the one installed here reads it with Blob#text().

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, relative, resolve } from "node:path";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "document-round-trip.json";
const SOURCE = "https://excalidraw.com";
const UPSTREAM_FIXTURES = "fixtures/upstream/packages/excalidraw/tests/fixtures";

const ENTRY = `
export { loadFromBlob, loadLibraryFromBlob } from "./packages/excalidraw/data/blob";
export { parseFileContents } from "./packages/excalidraw/data/blob";
export { serializeAsJSON, serializeLibraryAsJSON } from "./packages/excalidraw/data/json";
export { reseed } from "./packages/common/src/random";
export { diagramFixture } from "./packages/excalidraw/tests/fixtures/diagramFixture";
export * as elementFixtures from "./packages/excalidraw/tests/fixtures/elementFixture";
`;

// blob.ts pulls in the file dialogs and image resizing (pica,
// image-blob-reduce), and scene/export.ts the renderers and fonts (whose
// module reads import.meta.env.PKG_NAME while it loads); loadFromBlob
// reaches none of them for a scene file without local state
// (getScrollToContentState runs only with one). The SVG path calls
// decodeSvgBase64Payload, so scene/export itself is upstream's.
const STUBS = [
  "browser-fs-access",
  "pica",
  "image-blob-reduce",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/fonts",
  "packages/excalidraw/renderer/staticScene",
  "packages/excalidraw/renderer/staticSvgScene",
];

/** elementFixture.ts exports in source order. */
const ELEMENT_FIXTURES = [
  "rectangleFixture",
  "embeddableFixture",
  "ellipseFixture",
  "diamondFixture",
  "rectangleWithLinkFixture",
  "textFixture",
];

const MIME = { ".png": "image/png", ".svg": "image/svg+xml", ".excalidrawlib": "application/vnd.excalidrawlib+json" };

const usage = () => {
  process.stderr.write("usage: document-fixtures.mjs [--check] [--out DIR]\n");
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

const fixtureFile = (name) => `${UPSTREAM_FIXTURES}/${name}`;

const fileBlob = (rel) => {
  const ext = rel.slice(rel.lastIndexOf("."));
  const blob = new Blob([readFileSync(join(REPO_ROOT, rel))], { type: MIME[ext] });
  blob.name = basename(rel);
  return blob;
};

const jsonBlob = (text) => new Blob([text], { type: "application/json" });

/** serializeAsJSON of what loadFromBlob gives, after reseed(1). */
const loadAndSave = async (up, blob) => {
  up.reseed(1);
  const { elements, appState, files } = await up.loadFromBlob(blob, null, null);
  return up.serializeAsJSON(elements, appState, files, "local");
};

const sceneCase = async (up, id, files, input, blob = jsonBlob(input)) => {
  const output = await loadAndSave(up, blob);
  const reload = await loadAndSave(up, jsonBlob(output));
  return { id, files, kind: "scene", input, output, reload };
};

const embeddedCase = async (up, name) => {
  const rel = fixtureFile(name);
  const input = await up.parseFileContents(fileBlob(rel));
  return sceneCase(up, name, [rel], input, fileBlob(rel));
};

const libraryCase = async (up, name) => {
  const rel = fixtureFile(name);
  const save = async (blob) => {
    up.reseed(1);
    return up.serializeLibraryAsJSON(await up.loadLibraryFromBlob(blob));
  };
  const output = await save(fileBlob(rel));
  const reload = await save(new Blob([output], { type: MIME[".excalidrawlib"] }));
  return { id: name, files: [rel], kind: "library", input: readFileSync(join(REPO_ROOT, rel), "utf8"), output, reload };
};

const elementFixtureScene = (up) => {
  const exported = Object.keys(up.elementFixtures).sort();
  const listed = [...ELEMENT_FIXTURES].sort();
  if (JSON.stringify(exported) !== JSON.stringify(listed)) {
    throw new Error(`elementFixture.ts exports ${exported.join(", ")}; ELEMENT_FIXTURES lists ${listed.join(", ")}`);
  }
  return {
    type: "excalidraw",
    version: 2,
    source: SOURCE,
    elements: ELEMENT_FIXTURES.map((name) => up.elementFixtures[name]),
    appState: {},
    files: {},
  };
};

const withUnknownKeys = (scene) => {
  const [first, ...rest] = scene.elements;
  return {
    ...scene,
    futureTopLevel: { note: "kept by excali-rust", list: [1, "two", null] },
    elements: [{ ...first, futureElementKey: { nested: [1, 2, { z: 1, a: 2 }] } }, ...rest],
  };
};

/** A base element as upstream's newElement writes one, with `rest` last. */
const element = (id, type, rest = {}) => ({
  id,
  type,
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
  index: null,
  roundness: null,
  seed: 1,
  version: 3,
  versionNonce: 42,
  isDeleted: false,
  boundElements: null,
  updated: 5,
  created: 5,
  link: null,
  locked: false,
  ...rest,
});

const image = (id, fileId, rest = {}) =>
  element(id, "image", { fileId, status: "saved", scale: [1, 1], crop: null, ...rest });

const file = (id) => ({ mimeType: "image/png", id, dataURL: "data:image/png;base64,AA==", created: 1 });

/**
 * Scenes that exercise each step of loading and saving: files kept only
 * for live image elements, `files` that is an array, a string or a number
 * (indexed as JS indexes it), missing or malformed top-level values, legacy
 * appState values, dropped and deleted elements, and files loadFromBlob
 * rejects. Not upstream fixtures, so they are listed apart (`edges`).
 */
const edgeScenes = () => [
  [
    "files-of-live-images-only",
    {
      type: "excalidraw",
      version: 2,
      source: "https://example.com",
      elements: [
        image("live", "f2"),
        image("deleted", "f1", { isDeleted: true }),
        image("missing", "f9"),
        image("no-file", null),
        element("r", "rectangle"),
        image("again", "f2"),
        image("numeric", "42"),
      ],
      appState: { viewBackgroundColor: "#fafafa" },
      files: { f1: file("f1"), f2: file("f2"), 42: file("42"), orphan: file("orphan") },
    },
  ],
  ["only-type", { type: "excalidraw" }],
  ["falsy-elements-and-files", { type: "excalidraw", elements: null, appState: null, files: null }],
  ["app-state-array", { type: "excalidraw", elements: [], appState: [] }],
  [
    "legacy-app-state",
    {
      type: "excalidraw",
      elements: [],
      appState: {
        zoom: 2,
        gridSize: 7.4,
        gridStep: 1000,
        gridModeEnabled: true,
        viewBackgroundColor: "#000",
        lockedMultiSelections: { g: true },
        theme: "dark",
        currentItemStrokeWidth: 4,
        isSidebarDocked: true,
        unknownFlag: 1,
      },
    },
  ],
  [
    "dropped-and-deleted-elements",
    {
      type: "excalidraw",
      elements: [
        element("sel", "selection"),
        element("tiny", "rectangle", { width: 0, height: 0 }),
        element("empty-text", "text", {
          text: "",
          originalText: "",
          fontSize: 20,
          fontFamily: 5,
          textAlign: "left",
          verticalAlign: "top",
          containerId: null,
          lineHeight: 1.25,
          autoResize: true,
        }),
        element("dup", "ellipse"),
        element("dup", "diamond", { futureKey: [1] }),
        { type: "unknown-type", id: "u" },
        element("kept", "rectangle", { index: "a5" }),
      ],
    },
  ],
  // files[element.fileId] on a truthy `files` that is not an object:
  // an array or a string answers canonical index keys and `length`, a
  // number nothing but prototype functions (which JSON.stringify leaves
  // out); assigning `__proto__` sets the prototype, never a key.
  [
    "files-array",
    {
      type: "excalidraw",
      elements: [
        image("zero", "0"),
        image("one", "1"),
        image("two", "2"),
        image("padded", "00"),
        image("past-end", "3"),
        image("length", "length"),
        image("proto", "__proto__"),
        image("method", "constructor"),
      ],
      files: [file("f0"), 0, file("f2")],
    },
  ],
  [
    "files-string",
    {
      type: "excalidraw",
      elements: [
        image("zero", "0"),
        image("high", "1"),
        image("low", "2"),
        image("padded", "01"),
        image("length", "length"),
        image("method", "charAt"),
      ],
      files: "a\ud83d\ude00b",
    },
  ],
  ["files-number", { type: "excalidraw", elements: [image("a", "toString"), image("b", "0")], files: 7 }],
  [
    "files-own-proto-key",
    {
      type: "excalidraw",
      elements: [image("proto", "__proto__"), image("method", "constructor"), image("own", "f1")],
      files: JSON.parse('{"__proto__": {"id": "p"}, "constructor": 0, "f1": {"id": "f1"}}'),
    },
  ],
  ["not-a-scene", { type: "excalidrawlib", version: 2, libraryItems: [] }],
  ["elements-not-an-array", { type: "excalidraw", elements: { 0: element("r", "rectangle") } }],
];

const buildFixture = async (up, commit) => {
  const pretty = (value) => JSON.stringify(value, null, 2);
  const diagram = fixtureFile("diagramFixture.ts");
  const cases = [
    await sceneCase(up, "diagramFixture", [diagram], pretty(up.diagramFixture)),
    await sceneCase(up, "diagramFixture-unknown-keys", [diagram], pretty(withUnknownKeys(up.diagramFixture))),
    await sceneCase(up, "elementFixture", [fixtureFile("elementFixture.ts")], pretty(elementFixtureScene(up))),
    await embeddedCase(up, "test_embedded_v1.png"),
    await embeddedCase(up, "smiley_embedded_v2.png"),
    await embeddedCase(up, "test_embedded_v1.svg"),
    await embeddedCase(up, "smiley_embedded_v2.svg"),
    await libraryCase(up, "fixture_library.excalidrawlib"),
  ];
  const edges = [];
  for (const [id, scene] of edgeScenes()) {
    const input = pretty(scene);
    try {
      const { output, reload } = await sceneCase(up, id, [], input);
      edges.push({ id, input, output, reload });
    } catch (error) {
      edges.push({ id, input, error: error.message });
    }
  }
  const fixture = {
    description:
      "loadFromBlob then serializeAsJSON(..., \"local\") (packages/excalidraw/data/blob.ts, json.ts, restore.ts), " +
      "and the same again on the output, for every scene-bearing file of packages/excalidraw/tests/fixtures; " +
      "loadLibraryFromBlob then serializeLibraryAsJSON for the library. Upstream's test mode (randomId id0.., " +
      "getUpdatedTimestamp and Date.now 1, reseed(1) before each load). Generated by tools/goldens/document-fixtures.mjs.",
    upstream: commit,
    source: SOURCE,
    cases,
    edges,
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of invisible code points the attribution gate rejects.
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-￿]/g,
    (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

/** FileReader#readAsText for parseFileContents (see header). */
class NodeFileReader {
  static DONE = 2;
  readAsText(blob) {
    blob.text().then((text) => {
      this.readyState = NodeFileReader.DONE;
      this.result = text;
      this.onloadend?.();
    });
  }
}

/**
 * Runs fn with Math.random disabled and Date.now fixed at 1 (see header).
 * console.error is silenced: restore logs what it repairs.
 */
const deterministic = async (fn) => {
  const random = Math.random;
  const now = Date.now;
  const error = console.error;
  Math.random = () => {
    throw new Error("Math.random called while generating document fixtures");
  };
  Date.now = () => 1;
  console.error = () => {};
  try {
    return await fn();
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
    process.stderr.write(`document-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  Object.assign(globalThis.window, {
    EXCALIDRAW_EXPORT_SOURCE: SOURCE,
    atob: globalThis.atob,
    btoa: globalThis.btoa,
  });
  globalThis.FileReader = NodeFileReader;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    expose: { "packages/excalidraw/data/blob": ["parseFileContents"] },
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = await deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("document fixtures are out of date: run node tools/goldens/document-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`document fixtures up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
