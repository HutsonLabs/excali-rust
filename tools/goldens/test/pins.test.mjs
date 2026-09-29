// The generator must run upstream's code against exactly the rendering
// dependencies upstream ships: roughjs 4.6.4 and perfect-freehand 1.2.0
// (packages/excalidraw/package.json:106,114), and the same transitive
// tarballs upstream's yarn.lock resolves.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { readJson, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const pkg = readJson(join(TOOL_DIR, "package.json"));
const lock = readJson(join(TOOL_DIR, "package-lock.json"));

const RENDERING = {
  roughjs: "4.6.4",
  "perfect-freehand": "1.2.0",
  "points-on-curve": "1.0.1",
  tinycolor2: "1.6.0",
};

test("package.json pins the rendering dependencies exactly", () => {
  for (const [name, version] of Object.entries(RENDERING)) {
    assert.equal(pkg.dependencies[name], version, name);
  }
  for (const [name, spec] of Object.entries(pkg.dependencies)) {
    assert.match(spec, /^\d+\.\d+\.\d+$/, `${name} must be an exact version, got ${spec}`);
  }
});

test("pins agree with upstream's package.json files", () => {
  const upstream = upstreamDir();
  const declared = {};
  for (const p of ["excalidraw", "utils", "common"]) {
    const deps = readJson(join(upstream, "packages", p, "package.json")).dependencies;
    Object.assign(declared, deps);
  }
  // The DOM upstream's own test suite runs in (root package.json
  // devDependencies), for the svg-export goldens.
  const testEnvironment = readJson(join(upstream, "package.json")).devDependencies;
  for (const [name, version] of Object.entries(pkg.dependencies)) {
    if (name === "esbuild") continue; // build tool for the generator only
    if (name === "jsdom") {
      assert.equal(testEnvironment[name], version, `${name} differs from upstream's test environment`);
      continue;
    }
    // React is a peer dependency of the package; the version upstream runs
    // it on is the app's (excalidraw-app/package.json), for ui-primitives.
    if (name === "react" || name === "react-dom") {
      const app = readJson(join(upstream, "excalidraw-app", "package.json")).dependencies;
      assert.equal(app[name], version, `${name} differs from upstream's app`);
      continue;
    }
    assert.equal(declared[name], version, `${name} differs from upstream`);
  }
});

test("package-lock.json resolves the pinned versions", () => {
  for (const [name, version] of Object.entries(RENDERING)) {
    assert.equal(lock.packages[`node_modules/${name}`].version, version, name);
  }
});

/** name@version -> integrity from upstream's yarn.lock (v1 format). */
const yarnIntegrity = () => {
  const text = readFileSync(join(upstreamDir(), "yarn.lock"), "utf8");
  const map = new Map();
  for (const block of text.split(/\n\n+/)) {
    const version = block.match(/^\s+version "([^"]+)"/m);
    const integrity = block.match(/^\s+integrity (\S+)/m);
    if (!version || !integrity) continue;
    const header = block.split("\n").find((l) => l && !l.startsWith("#") && !l.startsWith(" "));
    for (const spec of header.replace(/:$/, "").split(/,\s*/)) {
      const name = spec.replace(/^"|"$/g, "").replace(/@[^@]*$/, "");
      map.set(`${name}@${version[1]}`, integrity[1]);
    }
  }
  return map;
};

test("roughjs and perfect-freehand trees are byte-identical to upstream's yarn.lock", () => {
  const upstream = yarnIntegrity();
  const tree = [
    "roughjs",
    "perfect-freehand",
    "points-on-curve",
    "tinycolor2",
    "roughjs/node_modules/points-on-curve",
    "hachure-fill",
    "path-data-parser",
    "points-on-path",
    "points-on-path/node_modules/points-on-curve",
  ];
  for (const path of tree) {
    const entry = lock.packages[`node_modules/${path}`];
    assert.ok(entry, `lockfile has ${path}`);
    const name = path.split("node_modules/").pop();
    const key = `${name}@${entry.version}`;
    assert.ok(upstream.has(key), `upstream yarn.lock has ${key}`);
    assert.equal(entry.integrity, upstream.get(key), `${key} integrity`);
  }
});

// document-fixtures.mjs loads scenes embedded in PNG and SVG with
// upstream's decodePngMetadata and decodeSvgBase64Payload, which read the
// chunks and inflate with these (packages/excalidraw/package.json:105-110).
const PAYLOAD = {
  pako: "2.0.3",
  "png-chunk-text": "1.0.0",
  "png-chunks-extract": "1.0.0",
  "png-chunks-encode": "1.0.0",
};

test("payload codec packages are upstream's tarballs", () => {
  const upstream = yarnIntegrity();
  for (const [name, version] of Object.entries(PAYLOAD)) {
    assert.equal(pkg.dependencies[name], version, name);
  }
  for (const path of [...Object.keys(PAYLOAD), "crc-32", "sliced"]) {
    const entry = lock.packages[`node_modules/${path}`];
    assert.ok(entry, `lockfile has ${path}`);
    const key = `${path}@${entry.version}`;
    assert.ok(upstream.has(key), `upstream yarn.lock has ${key}`);
    assert.equal(entry.integrity, upstream.get(key), `${key} integrity`);
  }
});

test("installed modules are the pinned versions", () => {
  for (const [name, version] of Object.entries(RENDERING)) {
    const installed = readJson(join(TOOL_DIR, "node_modules", name, "package.json"));
    assert.equal(installed.version, version, `${name} (run npm ci in tools/goldens)`);
  }
});

// ui-primitives.mjs renders upstream's components with React 19.0.0 and
// compiles their SCSS with sass 1.51.0 (excalidraw-app/package.json:36-37,
// packages/excalidraw/package.json:95,115).
const UI = { react: "19.0.0", "react-dom": "19.0.0", clsx: "1.1.1", sass: "1.51.0", scheduler: "0.25.0" };

test("ui primitive packages are upstream's tarballs", () => {
  const upstream = yarnIntegrity();
  for (const [name, version] of Object.entries(UI)) {
    if (name !== "scheduler") assert.equal(pkg.dependencies[name], version, name);
    const entry = lock.packages[`node_modules/${name}`];
    assert.ok(entry, `lockfile has ${name}`);
    assert.equal(entry.version, version, name);
    const key = `${name}@${entry.version}`;
    assert.ok(upstream.has(key), `upstream yarn.lock has ${key}`);
    assert.equal(entry.integrity, upstream.get(key), `${key} integrity`);
  }
});
