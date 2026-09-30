#!/usr/bin/env node
// Locale goldens for excali-ui (ex-711): upstream's own loader
// (packages/excalidraw/i18n.ts) bundled from the pinned checkout, its
// `languages`, `setLanguage` and `t` run under Node, and the locale files
// it reads (packages/excalidraw/locales/*.json) copied byte for byte.
//
//   node tools/goldens/i18n.mjs            write the fixture and the locales
//   node tools/goldens/i18n.mjs --check    exit 1 if any is stale
//   node tools/goldens/i18n.mjs --out DIR  write (or --check) in DIR
//
// Writes, under crates/excali-ui:
//
// - assets/locales/*.json: every JSON file of upstream's locales directory
//   (en.json, percentages.json and the translations), byte for byte except
//   that each code point the authorship gate counts as invisible
//   (scripts/gates/attribution.py _INVISIBLE_RANGES: the zero-width
//   non-joiner Persian needs, bidi marks, variation selectors) is written
//   as its \uXXXX escape, so every file parses to upstream's values; the
//   crate embeds en.json and percentages.json, a host serves the others;
// - tests/fixtures/i18n.json:
//   - `threshold`: COMPLETION_THRESHOLD, `defaultLang`;
//   - `languages`: upstream's `languages` (the build is production, so the
//     dev-only test languages are absent);
//   - `candidates`: the same list with the threshold patched to 0, i.e.
//     every language i18n.ts names, sorted as upstream sorts them;
//   - `files`: the locale file names;
//   - `keys`: en.json's leaf key count;
//   - `cases`: per language (each listed language, each other locale file
//     as `{code, label: code}`, and a code without a file, LTR and RTL)
//     `document.documentElement`'s `dir` and `lang` after setLanguage, and
//     `t(path)` on the sampled paths: every 8th leaf key of en.json, the
//     first leaf keys the language leaves empty or lacks (so the English
//     fallback runs), a namespace (not a string), a missing key (production
//     returns ""), and `t(path, null, fallback)` for a missing and a present
//     key;
//   - `replacements`: `t(path, replacement)` in English and German for
//     templates with one and two slots, a value used twice, numbers and
//     values with `$` patterns (String.prototype.replace's substitutions).

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "i18n.json");
export const LOCALES = join("assets", "locales");

const ENTRY = `
export { languages, defaultLang, setLanguage, getLanguage, t } from "./packages/excalidraw/i18n";
export { COMPLETION_THRESHOLD } from "./packages/excalidraw/i18n";
`;

const SHIMS = {
  "packages/excalidraw/editor-jotai": `
    module.exports = {
      atom: (init) => ({ init }),
      useAtomValue: (a) => a.init,
      editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
    };`,
};

const DEFINE = {
  "import.meta.env.MODE": '"production"',
  "import.meta.env.PKG_NAME": "undefined",
  "import.meta.env.PKG_VERSION": "undefined",
};

const usage = () => {
  process.stderr.write("usage: i18n.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// scripts/gates/attribution.py _INVISIBLE_RANGES
const GATED = [
  [0x00ad, 0x00ad], [0x034f, 0x034f], [0x061c, 0x061c], [0x115f, 0x1160],
  [0x180e, 0x180e], [0x200b, 0x200f], [0x2028, 0x202e], [0x2060, 0x2064],
  [0x206a, 0x206f], [0x3164, 0x3164], [0xfe00, 0xfe0f], [0xfeff, 0xfeff],
  [0xffa0, 0xffa0], [0xe0000, 0xe007f],
];

/** JSON text with each gated code point as \uXXXX escapes (they occur only
 * inside strings, where the escape parses back to the same code point). */
export const escapeGated = (json) => {
  let out = "";
  for (const ch of json) {
    const cp = ch.codePointAt(0);
    if (!GATED.some(([a, b]) => cp >= a && cp <= b)) {
      out += ch;
      continue;
    }
    for (let i = 0; i < ch.length; i++) out += `\\u${ch.charCodeAt(i).toString(16).padStart(4, "0")}`;
  }
  return out;
};

const leafKeys = (data, prefix = "") =>
  Object.entries(data).flatMap(([k, v]) =>
    v !== null && typeof v === "object" ? leafKeys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );

const lookup = (data, path) => path.split(".").reduce((d, p) => (d == null ? undefined : d[p]), data);

const load = (upstream, threshold) =>
  loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    define: DEFINE,
    expose: { "packages/excalidraw/i18n": ["COMPLETION_THRESHOLD"] },
    patch:
      threshold === undefined
        ? {}
        : {
            "packages/excalidraw/i18n": (source) =>
              source.replace("const COMPLETION_THRESHOLD = 85;", `const COMPLETION_THRESHOLD = ${threshold};`),
          },
  });

const quiet = async (fn) => {
  const { error, warn } = console;
  console.error = () => {};
  console.warn = () => {};
  try {
    return await fn();
  } finally {
    console.error = error;
    console.warn = warn;
  }
};

const build = async (upstream) => {
  const localesDir = join(upstream.dir, "packages", "excalidraw", "locales");
  const files = readdirSync(localesDir)
    .filter((f) => f.endsWith(".json"))
    .sort();
  const en = JSON.parse(readFileSync(join(localesDir, "en.json"), "utf8"));
  const keys = leafKeys(en);

  globalThis.document = { documentElement: { dir: "", lang: "" } };
  const up = await load(upstream);
  const all = await load(upstream, 0);
  if (up.COMPLETION_THRESHOLD !== 85) throw new Error(`threshold ${up.COMPLETION_THRESHOLD}`);

  const sample = keys.filter((_, i) => i % 8 === 0);
  const extra = ["labels", "labels.doesNotExist", "doesNotExist.either"];
  const listed = new Map(up.languages.map((l) => [l.code, l]));
  const langs = [
    ...up.languages,
    ...files
      .map((f) => f.replace(/\.json$/, ""))
      .filter((c) => c !== "percentages" && !listed.has(c))
      .map((code) => ({ code, label: code })),
    { code: "xx-XX", label: "no file" },
    { code: "xx-XX", label: "no file (rtl)", rtl: true },
  ];

  const cases = [];
  for (const lang of langs) {
    const file = join(localesDir, `${lang.code}.json`);
    const data = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
    const untranslated = keys.filter((k) => !lookup(data, k)).slice(0, 4);
    const paths = [...new Set([...sample, ...untranslated, ...extra])];
    await quiet(() => up.setLanguage(lang));
    const t = await quiet(() =>
      Object.fromEntries(paths.map((p) => [p, up.t(p)])),
    );
    const fallback = await quiet(() => ({
      "labels.doesNotExist": up.t("labels.doesNotExist", null, "fallback text"),
      "labels.delete": up.t("labels.delete", null, "fallback text"),
    }));
    cases.push({
      lang,
      dir: globalThis.document.documentElement.dir,
      htmlLang: globalThis.document.documentElement.lang,
      t,
      fallback,
    });
  }

  const REPLACEMENTS = [
    ["labels.copySource", undefined],
    ["hints.canvasPanning", { shortcut_1: "Space", shortcut_2: "Wheel" }],
    ["hints.canvasPanning", { shortcut_2: "B", shortcut_1: "A" }],
    ["hints.canvasPanning", { shortcut_1: "$&-$$-$`-$'-$1" }],
    ["alerts.removeItemsFromsLibrary", { count: 3 }],
    ["overwriteConfirm.action.saveToDisk.description", { x: "unused" }],
  ];
  // keys with a {{slot}} in en.json, each with its slots filled
  const templated = keys
    .filter((k) => /\{\{/.test(lookup(en, k)))
    .map((k) => [
      k,
      Object.fromEntries([...lookup(en, k).matchAll(/\{\{([^}]+)\}\}/g)].map((m, i) => [m[1], i === 0 ? 1.5 : `v${i}`])),
    ]);
  const replacements = [];
  for (const code of ["en", "de-DE"]) {
    await quiet(() => up.setLanguage(listed.get(code)));
    for (const [path, replacement] of [...REPLACEMENTS, ...templated]) {
      if (replacement !== undefined && lookup(en, path) === undefined) throw new Error(`no ${path}`);
      replacements.push({ code, path, replacement: replacement ?? null, result: up.t(path, replacement) });
    }
  }
  await up.setLanguage(up.defaultLang);

  const fixture = {
    upstream: upstream.commit,
    threshold: up.COMPLETION_THRESHOLD,
    defaultLang: up.defaultLang,
    languages: up.languages,
    candidates: all.languages,
    files,
    keys: keys.length,
    cases,
    replacements,
  };
  const out = { [FIXTURE]: escapeGated(format(fixture)) };
  for (const f of files) {
    const source = readFileSync(join(localesDir, f), "utf8");
    const copy = escapeGated(source);
    if (JSON.stringify(JSON.parse(copy)) !== JSON.stringify(JSON.parse(source))) throw new Error(`${f} changed`);
    out[join(LOCALES, f)] = copy;
  }
  return out;
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`i18n: ${error.message}\n`);
    process.exit(1);
  }
  const files = await build(upstream);
  const dir = args.out ?? OUT_DIR;
  if (args.check) {
    const stale = Object.entries(files).filter(([f, text]) => {
      const path = join(dir, f);
      return !existsSync(path) || readFileSync(path, "utf8") !== text;
    });
    const extra = existsSync(join(dir, LOCALES))
      ? readdirSync(join(dir, LOCALES)).filter((f) => !Object.hasOwn(files, join(LOCALES, f)))
      : [];
    if (stale.length || extra.length) {
      for (const [f] of stale) process.stderr.write(`stale: ${relative(process.cwd(), join(dir, f))}\n`);
      for (const f of extra) process.stderr.write(`not upstream's: ${relative(process.cwd(), join(dir, LOCALES, f))}\n`);
      process.stderr.write("i18n goldens are out of date: run node tools/goldens/i18n.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`i18n goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, text] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
  }
  process.stdout.write(
    `wrote ${relative(process.cwd(), join(dir, FIXTURE))} and ${Object.keys(files).length - 1} locale files from upstream ${upstream.commit.slice(0, 7)}\n`,
  );
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
