// Loads upstream Excalidraw's own shape code from the pinned checkout.
//
// The TypeScript sources under <upstream>/packages are bundled with esbuild
// (type-stripping only, no transforms that change arithmetic) into a single
// ES module and imported. Bare imports such as `roughjs` or
// `perfect-freehand` resolve to tools/goldens/node_modules, where
// package-lock.json pins the exact versions upstream's yarn.lock resolves,
// so the numbers in the goldens are the numbers upstream's code computes.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const TOOL_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const REPO_ROOT = resolve(TOOL_DIR, "..", "..");

const CHECKOUT = join(REPO_ROOT, "scripts", "upstream", "checkout.sh");

/** The pinned upstream commit (site/config.toml extra.upstream_commit). */
export const pinnedCommit = () =>
  execFileSync("bash", [CHECKOUT, "--print-pin"], { encoding: "utf8" }).trim();

/**
 * The upstream checkout directory. UPSTREAM_DIR overrides it the same way it
 * does for scripts/upstream/checkout.sh.
 */
export const upstreamDir = () =>
  execFileSync("bash", [CHECKOUT, "--print-dir"], { encoding: "utf8" }).trim();

/**
 * Refuses to run unless the checkout is clean and exactly at the pin, so a
 * golden can never be produced from a different upstream revision.
 */
export const verifyUpstream = () => {
  try {
    execFileSync("bash", [CHECKOUT, "--verify"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    const detail = String(error.stderr || error.message).trim();
    throw new Error(
      `upstream checkout is not clean at the pin (${detail}); run scripts/upstream/checkout.sh`,
    );
  }
  return { dir: upstreamDir(), commit: pinnedCommit() };
};

// Everything the generator needs from upstream, imported from upstream's own
// modules. Nothing here re-implements upstream logic.
const ENTRY = `
export { ShapeCache, generateRoughOptions, getFreedrawOutlinePoints }
  from "./packages/element/src/shape";
export * as elementFixtures from "./packages/excalidraw/tests/fixtures/elementFixture";
export * as math from "./packages/math/src/index";
export { RoughGenerator } from "roughjs/bin/generator";
export { Random } from "roughjs/bin/math";
export { getStroke, getStrokePoints } from "perfect-freehand";
export { LaserPointer } from "@excalidraw/laser-pointer";
export * as fractionalIndexing from "./packages/fractional-indexing/src/index";
export {
  orderByFractionalIndex,
  syncInvalidIndices,
  syncInvalidIndicesImmutable,
  syncMovedIndices,
  validateFractionalIndices,
} from "./packages/element/src/fractionalIndex";
`;

const PACKAGES = [
  "common",
  "element",
  "excalidraw",
  "fractional-indexing",
  "laser-pointer",
  "math",
  "utils",
];

/**
 * Resolves `@excalidraw/<pkg>` and `@excalidraw/<pkg>/<sub>` to the package
 * sources in the checkout, mirroring the `paths` block of upstream's root
 * tsconfig.json (excalidraw's entry is index.tsx at the package root).
 */
const workspaceAliases = (upstream) => ({
  name: "upstream-workspace",
  setup(build) {
    const filter = new RegExp(`^@excalidraw/(${PACKAGES.join("|")})(/.*)?$`);
    build.onResolve({ filter }, async (args) => {
      const [, pkg, sub] = args.path.match(filter);
      const base =
        pkg === "excalidraw"
          ? join(upstream, "packages", pkg)
          : join(upstream, "packages", pkg, "src");
      const entry = pkg === "excalidraw" ? "index.tsx" : "index.ts";
      const target = sub ? join(base, sub) : join(base, entry);
      return build.resolve(target, { kind: args.kind, resolveDir: upstream });
    });
  },
});

/**
 * Resolves every other bare import (roughjs, perfect-freehand, ...) from
 * tools/goldens/node_modules only, so a node_modules someone installed in the
 * upstream checkout can never supply different package versions.
 */
const pinnedPackages = () => ({
  name: "pinned-packages",
  setup(build) {
    build.onResolve({ filter: /^[^./]/ }, async (args) => {
      if (args.pluginData?.pinned) return undefined;
      const result = await build.resolve(args.path, {
        kind: args.kind,
        resolveDir: TOOL_DIR,
        pluginData: { pinned: true },
      });
      if (result.errors.length) return { errors: result.errors };
      if (result.external) return result; // node builtins
      if (!result.path.startsWith(join(TOOL_DIR, "node_modules"))) {
        return { errors: [{ text: `${args.path} resolved outside tools/goldens/node_modules` }] };
      }
      return result;
    });
  },
});

/**
 * Replaces each listed module with an empty CommonJS module: a package name
 * (and its subpaths), or a checkout module given by its path from the
 * checkout root without extension (e.g. `packages/excalidraw/data/blob`),
 * matched on relative imports. Meant for browser-only code (file dialogs,
 * image codecs) that a module the generator imports pulls in but the
 * functions it calls never reach; every export of a stub is undefined, so a
 * call into one fails loudly.
 *
 * `shims` maps more such names to the CommonJS source that replaces them,
 * for a module whose exports the importing module calls while it loads
 * (e.g. jotai's `atom(...)` at module level); what the shim does not define
 * is undefined, as in a stub.
 */
const stubbedModules = (upstream, stubs, shims = {}) => ({
  name: "stubbed-modules",
  setup(build) {
    const names = [...stubs, ...Object.keys(shims)];
    if (!names.length) return;
    const files = new Set(names.filter((n) => n.includes("/") && !n.startsWith("@")).map((n) => join(upstream, n)));
    const packages = names.filter((n) => !files.has(join(upstream, n)));
    const stub = (args) => ({ path: args.path, namespace: "stub" });
    const stubFile = (args) => ({ path: join(args.resolveDir, args.path), namespace: "stub" });
    if (packages.length) {
      const escaped = packages.map((n) => n.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&"));
      build.onResolve({ filter: new RegExp(`^(${escaped.join("|")})(/.*)?$`) }, stub);
    }
    if (files.size) {
      build.onResolve({ filter: /^\.\.?\// }, (args) =>
        files.has(join(args.resolveDir, args.path)) ? stubFile(args) : undefined,
      );
    }
    const shimFor = (path) => {
      const name = names.find((n) => path === n || path.startsWith(`${n}/`) || join(upstream, n) === path);
      return name !== undefined && Object.hasOwn(shims, name) ? shims[name] : undefined;
    };
    build.onLoad({ filter: /.*/, namespace: "stub" }, (args) => ({
      contents: shimFor(args.path) ?? "module.exports = {};",
      loader: "js",
    }));
  },
});

/**
 * Resolves relative imports of one checkout module to another: `redirects`
 * maps a checkout module path from the root without extension, as an
 * importing module names it (e.g. `packages/excalidraw/actions`, the
 * actions index), to the checkout file that answers it (e.g.
 * `packages/excalidraw/actions/actionExport.tsx`). Meant for an index
 * module whose other re-exports pull in React components the generator
 * never reaches, when the names the importer takes all come from the one
 * file; the file is upstream's own and loads unchanged.
 */
const redirectedModules = (upstream, redirects) => ({
  name: "redirected-modules",
  setup(build) {
    const map = new Map(Object.entries(redirects).map(([from, to]) => [join(upstream, from), join(upstream, to)]));
    if (!map.size) return;
    build.onResolve({ filter: /^\.\.?\// }, (args) => {
      const target = map.get(join(args.resolveDir, args.path));
      return target ? { path: target } : undefined;
    });
  },
});

/**
 * Exports module-private functions of checkout modules so a generator can
 * call them directly: `exposed` maps a checkout module path from the root
 * without extension (e.g. `packages/excalidraw/data/restore`) to the names
 * to export. The module's source is loaded unchanged with one line
 * appended, `export { name, ... };`, so the functions are upstream's own.
 *
 * `patched` maps a checkout module path the same way to a function of its
 * source returning the source to compile, for probes that record what a
 * module passes to the functions it calls (see restore-elements-fixtures.mjs);
 * the patch runs before the export line is appended.
 */
const exposedModules = (upstream, exposed, patched = {}) => ({
  name: "exposed-modules",
  setup(build) {
    const modules = new Set([...Object.keys(exposed), ...Object.keys(patched)]);
    if (!modules.size) return;
    const files = new Map(
      [...modules].map((module) => [
        join(upstream, `${module}.ts`),
        { names: exposed[module] ?? [], patch: patched[module] ?? ((source) => source) },
      ]),
    );
    build.onLoad({ filter: /\.ts$/ }, (args) => {
      const file = files.get(args.path);
      if (!file) return undefined;
      const source = file.patch(readFileSync(args.path, "utf8"));
      const exports = file.names.length ? `\nexport { ${file.names.join(", ")} };\n` : "";
      return { contents: `${source}${exports}`, loader: "ts" };
    });
  },
});

/**
 * Makes each `.woff2` import under packages/excalidraw/fonts the uri
 * upstream's package build gives it instead of an empty module, so a
 * generator sees each font face's real urls (font-assets.mjs,
 * svg-export.mjs). scripts/buildPackage.js bundles with esbuild's `file`
 * loader for `.woff2`, `assetNames: "[dir]/[name]"` and entry points
 * `index.tsx` and the `*.chunk.ts` files under packages/excalidraw (so that
 * directory is the outbase), which emits the file at
 * `dist/prod/fonts/<Family>/<name>.woff2` and imports it as
 * `./fonts/<Family>/<name>.woff2`; ExcalidrawFontFace.createUrls resolves
 * that against ASSETS_FALLBACK_URL. Only with `fontUris: true`; the
 * layout is checked against esbuild in test/font-assets.test.mjs.
 */
export const fontUri = (fontsRelativePath) => `./fonts/${fontsRelativePath}`;

const fontUris = (upstream) => ({
  name: "font-uris",
  setup(build) {
    const fontsDir = join(upstream, "packages", "excalidraw", "fonts");
    build.onLoad({ filter: /\.woff2$/ }, (args) => {
      if (!args.path.startsWith(`${fontsDir}/`)) return undefined;
      return {
        contents: `export default ${JSON.stringify(fontUri(args.path.slice(fontsDir.length + 1)))};`,
        loader: "js",
      };
    });
  },
});

let loads = 0;

/**
 * Bundles upstream's code and returns the bundle's source. The options are
 * loadUpstream's, plus esbuild's `platform` and `format` (default "node"
 * and "esm") and `globalName` for an "iife" bundle, so a script can run
 * upstream's own code in a browser page
 * (scripts/fixtures/text_width_causes.mjs), and esbuild's `jsx` ("automatic"
 * for upstream's components, which use React's automatic runtime as its
 * vite config does, so a module need not import React for its JSX).
 */
export const bundleUpstream = async (
  { dir },
  {
    entry = ENTRY,
    stubs = [],
    shims = {},
    expose = {},
    patch = {},
    redirects = {},
    define = {},
    fontUris: withFontUris = false,
    platform = "node",
    format = "esm",
    globalName,
    jsx,
  } = {},
) => {
  const esbuild = await import("esbuild");
  const result = await esbuild.build({
    stdin: {
      contents: entry,
      resolveDir: dir,
      sourcefile: "goldens-entry.ts",
      loader: "ts",
    },
    bundle: true,
    write: false,
    format,
    ...(globalName ? { globalName } : {}),
    platform,
    ...(jsx ? { jsx } : {}),
    target: "esnext",
    logLevel: "silent",
    plugins: [
      workspaceAliases(dir),
      redirectedModules(dir, redirects),
      stubbedModules(dir, stubs, shims),
      exposedModules(dir, expose, patch),
      ...(withFontUris ? [fontUris(dir)] : []),
      pinnedPackages(),
    ],
    loader: { ".png": "empty", ".svg": "empty", ".scss": "empty", ".css": "empty", ".woff2": "empty" },
    define: { "import.meta.env.DEV": "false", "import.meta.env.PROD": "true", ...define },
  });
  return result.outputFiles[0].text;
};

/**
 * Bundles upstream's code and returns the imported module. `entry` is the
 * TypeScript entry's source, resolved from the checkout root (the default
 * exports the shape code the goldens use); `stubs` lists modules replaced
 * by empty ones and `shims` maps modules to replacement CommonJS sources
 * (see stubbedModules); `expose` exports module-private functions and
 * `patch` rewrites checkout modules (see exposedModules); `redirects`
 * points relative imports of a checkout module at another (see
 * redirectedModules); `define` adds
 * compile-time constants; `fontUris` makes each font file import its path
 * (see fontUris).
 */
export const loadUpstream = async (upstream, options = {}) => {
  const source = await bundleUpstream(upstream, { ...options, platform: "node", format: "esm" });
  // One file per process and load: concurrent runs (the test suite) never
  // share it, and a second load in the same process (another `define`) is
  // not answered from the ES module cache with the first bundle.
  const buildDir = join(TOOL_DIR, ".build");
  mkdirSync(buildDir, { recursive: true });
  const file = join(buildDir, `upstream-${process.pid}-${loads++}.mjs`);
  writeFileSync(file, source);
  try {
    return await import(pathToFileURL(file).href);
  } finally {
    rmSync(file, { force: true });
  }
};

export const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
