#!/usr/bin/env node
// The icon set for excali-ui (ex-517): every icon upstream's
// packages/excalidraw/components/icons.tsx exports, rendered by React
// 19.0.0 (the version excalidraw-app pins) with react-dom/server's
// renderToStaticMarkup, so each is the static SVG string upstream's own
// createIcon produces.
//
//   node tools/goldens/icons.mjs            write the module and fixture
//   node tools/goldens/icons.mjs --check    exit 1 if either is stale
//   node tools/goldens/icons.mjs --out DIR  write (or --check) in DIR
//
// icons.tsx has 209 exports; `iconFillColor` and `createIcon` are helpers,
// which leaves 207 icons of three kinds:
//
// - `static`: a `createIcon(...)` element (or `emptyIcon`, a sized div),
//   rendered once;
// - `themed`: a `React.memo(({ theme }) => createIcon(...))` component,
//   rendered with `theme` "light" and "dark" (GroupIcon, UngroupIcon and
//   others draw with handlerColor, icons.tsx:18-19);
// - `paths`: an array of path data (bucketFillIconSvgPaths,
//   eyeDropperIconSvgPaths).
//
// Each icon records whether it mirrors in right-to-left languages (the
// `mirror` option, icons.tsx:22-25, which createIcon renders as the
// `rtl-mirror` class) and which size preset it was built with: `tabler`
// (tablerIconProps, 24×24, icons.tsx:53-61), `modifiedTabler`
// (modifiedTablerIconProps, 20×20, :63-70) or `arrowheadPreview`
// (arrowheadPreviewIconProps, 40×20, :72-75), directly or spread with
// overrides. Both are read from the options createIcon receives: the
// module is compiled with createIcon wrapped to record its options per
// element, and each preset object tagged with a `__preset` key the wrapper
// removes before calling upstream's createIcon, so the SVG is unchanged.
//
// Writes, under crates/excali-ui:
//
// - tests/fixtures/icons.json: `{ upstream, icons: [{ name, kind, mirror,
//   preset, markup | light, dark | paths }] }` in export order;
// - src/icons/generated.rs: the same icons as `pub const` Rust items
//   named as upstream exports them, and the `ICONS` table;
// - src/icons/icons.css: the rule that flips a mirrored icon in a
//   right-to-left document (`.rtl-mirror`, css/styles.scss:679-683),
//   compiled with sass 1.51.0 (the version packages/excalidraw pins) from
//   styles.scss, the stylesheet it sits in (ex-709).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "icons.json");
export const MODULE = join("src", "icons", "generated.rs");
export const STYLESHEET = join("src", "icons", "icons.css");

const ICONS_MODULE = "packages/excalidraw/components/icons";

const HELPERS = new Set(["iconFillColor", "createIcon"]);

const PRESETS = {
  tablerIconProps: "tabler",
  modifiedTablerIconProps: "modifiedTabler",
  arrowheadPreviewIconProps: "arrowheadPreview",
};

const ENTRY = `
export * as icons from "./${ICONS_MODULE}";
export { renderToStaticMarkup } from "react-dom/server.browser";
export { default as React } from "react";
`;

const replaceOnce = (source, from, to) => {
  const at = source.indexOf(from);
  if (at < 0 || source.indexOf(from, at + 1) >= 0) {
    throw new Error(`icons.tsx changed: expected exactly one ${JSON.stringify(from)}`);
  }
  return source.slice(0, at) + to + source.slice(at + from.length);
};

// Wraps createIcon to report its options and tags the presets (see the
// header). The wrapper sits right after upstream's createIcon, before the
// first icon is built.
const instrument = (source) => {
  let out = replaceOnce(source, "export const createIcon = (", "const upstreamCreateIcon = (");
  out = replaceOnce(
    out,
    "const tablerIconProps: Opts = {",
    `export const createIcon = (d: any, opts: any = 512) => {
  const { __preset, ...rest } = typeof opts === "number" ? { __preset: undefined } : opts;
  const icon = upstreamCreateIcon(d, typeof opts === "number" ? opts : rest);
  (globalThis as any).__createdIcon(icon, opts);
  return icon;
};

const tablerIconProps: Opts = {`,
  );
  for (const [name, preset] of Object.entries(PRESETS)) {
    out = replaceOnce(out, `const ${name}: Opts = {`, `const ${name}: Opts = {\n  __preset: "${preset}",`);
  }
  return out;
};

const kindOf = (value) => {
  if (Array.isArray(value) && value.every((p) => typeof p === "string")) return "paths";
  if (value && value.$$typeof === Symbol.for("react.memo")) return "themed";
  if (value && value.$$typeof === Symbol.for("react.transitional.element")) return "static";
  throw new Error(`icons.tsx export of an unknown kind: ${String(value)}`);
};

const describeOptions = (opts) => ({
  mirror: typeof opts === "object" && opts.mirror === true,
  preset: (typeof opts === "object" && opts.__preset) || null,
});

/** The top-level rules of compiled `css` naming `.rtl-mirror`. */
export const rtlMirrorRules = (css) =>
  [...css.matchAll(/^([^\s@}][^{}]*)\{[^{}]*\}/gm)]
    .map((m) => m[0])
    .filter((rule) => rule.split("{")[0].includes(".rtl-mirror"));

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const path = join(upstream.dir, "packages", "excalidraw", "css", "styles.scss");
  const rules = rtlMirrorRules(sass.compile(path, { style: "expanded" }).css);
  if (rules.length !== 1) throw new Error(`styles.scss: ${rules.length} .rtl-mirror rules, want 1`);
  return (
    "/* Generated by tools/goldens/icons.mjs; do not edit. The `.rtl-mirror`\n" +
    " * rule of upstream's packages/excalidraw/css/styles.scss at the pin,\n" +
    " * compiled with sass 1.51.0 (expanded). */\n" +
    `${rules.join("\n\n")}\n`
  );
};

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const created = new WeakMap();
  let last = null;
  globalThis.__createdIcon = (icon, opts) => {
    created.set(icon, opts);
    last = opts;
  };
  const { icons, renderToStaticMarkup, React } = await loadUpstream(upstream, {
    entry: ENTRY,
    patch: { [ICONS_MODULE]: instrument },
    jsx: "automatic",
  });

  // A module namespace lists its exports alphabetically; the fixture keeps
  // the source's order.
  const source = readFileSync(join(upstream.dir, `${ICONS_MODULE}.tsx`), "utf8");
  const order = [...source.matchAll(/^export const (\w+)/gm)].map((m) => m[1]);
  const names = Object.keys(icons);
  if (JSON.stringify([...order].sort()) !== JSON.stringify([...names].sort())) {
    throw new Error("icons.tsx exports differ from its `export const` lines");
  }

  const list = [];
  for (const name of order) {
    if (HELPERS.has(name)) continue;
    const value = icons[name];
    const kind = kindOf(value);
    if (kind === "paths") {
      list.push({ name, kind, mirror: false, preset: null, paths: [...value] });
    } else if (kind === "static") {
      const opts = created.get(value);
      const described = opts === undefined ? { mirror: false, preset: null } : describeOptions(opts);
      list.push({ name, kind, ...described, markup: renderToStaticMarkup(value) });
    } else {
      const render = (theme) => {
        last = null;
        const markup = renderToStaticMarkup(React.createElement(value, { theme }));
        if (last === null) throw new Error(`${name} did not call createIcon`);
        return { markup, described: describeOptions(last) };
      };
      const light = render("light");
      const dark = render("dark");
      if (JSON.stringify(light.described) !== JSON.stringify(dark.described)) {
        throw new Error(`${name}: options differ between themes`);
      }
      list.push({ name, kind, ...light.described, light: light.markup, dark: dark.markup });
    }
  }
  delete globalThis.__createdIcon;

  const fixture = { upstream: upstream.commit, icons: list };
  return {
    [FIXTURE]: format(fixture),
    [MODULE]: rust(upstream.commit, list),
    [STYLESHEET]: css,
  };
};

// -- Rust ----------------------------------------------------------------------

const rustString = (s) => {
  let hashes = "#";
  while (s.includes(`"${hashes}`)) hashes += "#";
  return `r${hashes}"${s}"${hashes}`;
};

const PRESET_VARIANTS = {
  tabler: "Tabler",
  modifiedTabler: "ModifiedTabler",
  arrowheadPreview: "ArrowheadPreview",
};

const rustMarkup = (icon) => {
  if (icon.kind === "static") return `Markup::Static(${rustString(icon.markup)})`;
  if (icon.kind === "themed") {
    return `Markup::Themed {\n        light: ${rustString(icon.light)},\n        dark: ${rustString(icon.dark)},\n    }`;
  }
  return `Markup::Paths(&[\n${icon.paths.map((p) => `        ${rustString(p)},\n`).join("")}    ])`;
};

const rust = (commit, list) => {
  const items = list.map((icon) => {
    const preset = icon.preset ? `Some(Preset::${PRESET_VARIANTS[icon.preset]})` : "None";
    return [
      `/// \`${icon.name}\` (\`icons.tsx\`).`,
      `pub const ${icon.name}: Icon = Icon {`,
      `    name: "${icon.name}",`,
      `    markup: ${rustMarkup(icon)},`,
      `    mirror: ${icon.mirror},`,
      `    preset: ${preset},`,
      `};`,
      "",
    ].join("\n");
  });
  return [
    `// Generated by tools/goldens/icons.mjs from upstream ${commit}`,
    "// packages/excalidraw/components/icons.tsx (MIT). Do not edit: run",
    "// node tools/goldens/icons.mjs.",
    "",
    "// The items keep upstream's export names: icons.tsx has pairs that differ",
    "// only in case (helpIcon and HelpIcon), so no Rust casing is unambiguous.",
    "#![allow(non_upper_case_globals)]",
    "",
    "use super::{Icon, Markup, Preset};",
    "",
    ...items,
    "/// Every icon, in the order icons.tsx exports them.",
    `pub static ICONS: [&Icon; ${list.length}] = [`,
    ...list.map((icon) => `    &${icon.name},`),
    "];",
    "",
  ].join("\n");
};

// -- main ----------------------------------------------------------------------

const parseArgs = (argv) => {
  const args = { check: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
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
    process.stderr.write(`icons: ${error.message}\n`);
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
      process.stderr.write("icon goldens are out of date: run node tools/goldens/icons.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`icon goldens up to date: ${Object.keys(files).length} files\n`);
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
  // React's server renderer leaves a scheduler handle open.
  process.exit(0);
}
