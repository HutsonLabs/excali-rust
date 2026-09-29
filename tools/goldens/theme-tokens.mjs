#!/usr/bin/env node
// Theme token goldens for excali-ui (ex-532): the CSS custom properties
// upstream's packages/excalidraw/css/theme.scss declares, compiled with
// sass 1.51.0 (the version packages/excalidraw pins), and the ones
// App.tsx writes into the editor container's inline style.
//
//   node tools/goldens/theme-tokens.mjs            write the fixture
//   node tools/goldens/theme-tokens.mjs --check    exit 1 if it is stale
//   node tools/goldens/theme-tokens.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/theme-tokens.json:
//
// - `light`: the declarations of `.excalidraw` (theme.scss:5-165) as
//   `[name, value]` pairs in source order, values as sass prints them
//   (`--color-surface-primary-container` is declared twice, :158 and :162);
// - `dark`: those of `.excalidraw.theme--dark` (theme.scss:184-278);
// - `mobile`: those of `.excalidraw--mobile.excalidraw` (the isMobile
//   mixin, theme.scss:167-169);
// - `largeScreen`: those `@media screen and (min-device-width: 1921px)`
//   redeclares on `.excalidraw` (theme.scss:171-176);
// - `container`: `--right-sidebar-width`, which App.tsx:2453 sets on the
//   container from RIGHT_SIDEBAR_WIDTH (App.viewport.ts:62).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "theme-tokens.json");

const LIGHT = ".excalidraw";
const DARK = ".excalidraw.theme--dark";
const MOBILE = ".excalidraw--mobile.excalidraw";
const LARGE_SCREEN = "@media screen and (min-device-width: 1921px)";

/**
 * The custom property declarations of each rule in compiled CSS, keyed by
 * selector (prefixed with its @media query inside one), as `[name, value]`
 * pairs in source order (a rule may declare a name twice; the last wins).
 * A declaration may span lines; it ends at the `;` closing its line.
 */
export const rules = (css) => {
  const out = {};
  const stack = [];
  let pending = null;
  for (const raw of css.split("\n")) {
    const line = raw.trim();
    if (pending !== null) {
      pending += ` ${line}`;
      if (line.endsWith(";")) {
        declare(out, stack, pending);
        pending = null;
      }
      continue;
    }
    if (line.endsWith("{")) {
      stack.push(line.slice(0, -1).trim());
      continue;
    }
    if (line === "}") {
      stack.pop();
      continue;
    }
    if (!line.startsWith("--")) continue;
    if (line.endsWith(";")) declare(out, stack, line);
    else pending = line;
  }
  return out;
};

const declare = (out, stack, text) => {
  const m = text.match(/^(--[\w-]+):\s*([\s\S]*);$/);
  if (!m) throw new Error(`unparsed declaration: ${text}`);
  const key = stack.join(" ");
  (out[key] ??= []).push([m[1], m[2]]);
};

const need = (blocks, key) => {
  if (!blocks[key]) throw new Error(`theme.scss changed: no ${key} rule`);
  return blocks[key];
};

const build = async (upstream) => {
  const sass = (await import("sass")).default;
  const excalidraw = join(upstream.dir, "packages", "excalidraw");
  const css = sass.compile(join(excalidraw, "css", "theme.scss"), { style: "expanded" }).css;
  const blocks = rules(css);
  const viewport = readFileSync(join(excalidraw, "components", "App.viewport.ts"), "utf8");
  const width = viewport.match(/^export const RIGHT_SIDEBAR_WIDTH = (\d+);$/m);
  if (!width) throw new Error("App.viewport.ts changed: no RIGHT_SIDEBAR_WIDTH");
  const fixture = {
    upstream: upstream.commit,
    light: need(blocks, LIGHT),
    dark: need(blocks, DARK),
    mobile: need(blocks, MOBILE),
    largeScreen: need(blocks, `${LARGE_SCREEN} ${LIGHT}`),
    container: { "--right-sidebar-width": `${width[1]}px` },
  };
  return { [FIXTURE]: format(fixture) };
};

const parseArgs = (argv) => {
  const args = { check: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out") args.out = resolve(argv[++i]);
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  return args;
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`theme-tokens: ${error.message}\n`);
    process.exit(1);
  }
  const files = await build(upstream);
  const dir = args.out ?? OUT_DIR;
  if (args.check) {
    const stale = Object.entries(files).filter(([f, text]) => {
      const path = join(dir, f);
      return !existsSync(path) || readFileSync(path, "utf8") !== text;
    });
    if (stale.length) {
      for (const [f] of stale) process.stderr.write(`stale: ${relative(process.cwd(), join(dir, f))}\n`);
      process.stderr.write("theme-tokens goldens are out of date: run node tools/goldens/theme-tokens.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`theme-tokens goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, text] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
    process.stdout.write(`wrote ${relative(process.cwd(), path)} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
