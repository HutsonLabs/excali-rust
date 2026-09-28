#!/usr/bin/env node
// The browser half of the ex-408 fidelity numbers (ADR-010): every subset
// the browser check covers, loaded into Chromium and drawn next to its
// original face.
//
//   cargo run --release --manifest-path tools/font-subset-eval/Cargo.toml -- \
//     --browser tools/font-subset-eval/target/browser/cases.json
//   node tools/font-subset-eval/browser/render.mjs            write browser.json
//   node tools/font-subset-eval/browser/render.mjs --check    exit 1 if it differs
//
// For each face of a browser scene in upstream-subsets.json and each
// candidate (upstream's own subset, and each subsetter's with the Rust
// encoder), the page registers the original file and the subset with the
// CSS Font Loading API, which runs Chromium's font sanitizer (OTS) as an
// SVG's @font-face does: a subset it rejects never loads. Each run of the
// scene's text in the face is then drawn on a canvas at 64 px in either
// font; the run is equal when the pixels are identical and measureText
// gives the same width. The counts go to browser.json. They compare two
// fonts inside one browser, so they do not depend on the Chromium build.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "@playwright/test";

const HERE = dirname(fileURLToPath(import.meta.url));
const EVAL = resolve(HERE, "..");
export const REPORT = join(EVAL, "browser.json");
const DEFAULT_CASES = join(EVAL, "target", "browser", "cases.json");

const parseArgs = (argv) => {
  const args = { check: false, cases: DEFAULT_CASES };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--cases" && argv[i + 1]) args.cases = resolve(argv[++i]);
    else {
      process.stderr.write("usage: render.mjs [--check] [--cases FILE]\n");
      process.exit(2);
    }
  }
  return args;
};

/** Runs in the page: loads the fonts, draws every run, counts. */
const measure = async ({ fonts, cases, candidates }) => {
  const bytes = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const loaded = new Map();
  const load = async (i) => {
    if (!loaded.has(i)) {
      const face = new FontFace(`f${i}`, bytes(fonts[i]));
      loaded.set(
        i,
        face.load().then(
          (f) => {
            document.fonts.add(f);
            return true;
          },
          () => false,
        ),
      );
    }
    return loaded.get(i);
  };
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  const draw = (family, text, width) => {
    canvas.width = width;
    canvas.height = 128;
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, width, 128);
    ctx.fillStyle = "#000";
    ctx.font = `64px ${family}`;
    ctx.textBaseline = "alphabetic";
    ctx.fillText(text, 16, 88);
    return ctx.getImageData(0, 0, width, 128).data;
  };
  const width = (family, text) => {
    ctx.font = `64px ${family}`;
    return ctx.measureText(text).width;
  };
  const totals = Object.fromEntries(
    candidates.map((c) => [c, { declarations: 0, failed: 0, loaded: 0, runs: 0, runsEqual: 0 }]),
  );
  const unequal = [];
  const rejected = [];
  for (const c of cases) {
    for (const name of candidates) {
      const s = c.subsets[name];
      const t = totals[name];
      t.declarations++;
      if (s.font === null) {
        t.failed++;
        continue;
      }
      const [okOriginal, okSubset] = await Promise.all([load(s.original), load(s.font)]);
      if (!okOriginal) throw new Error(`original ${c.file} did not load`);
      if (!okSubset) {
        rejected.push({ candidate: name, scene: c.scene, file: c.file });
        continue;
      }
      t.loaded++;
      for (const run of s.runs) {
        t.runs++;
        const a = `f${s.original}`;
        const b = `f${s.font}`;
        const wa = width(a, run);
        const wb = width(b, run);
        const w = Math.ceil(Math.max(wa, wb)) + 32;
        const pa = draw(a, run, w);
        const pb = draw(b, run, w);
        let same = wa === wb && pa.length === pb.length;
        for (let i = 0; same && i < pa.length; i++) same = pa[i] === pb[i];
        if (same) t.runsEqual++;
        else if (unequal.length < 20) unequal.push({ candidate: name, scene: c.scene, file: c.file, run });
      }
    }
  }
  return { totals, unequal, rejected };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  if (!existsSync(args.cases)) {
    process.stderr.write(
      `render: ${args.cases} is missing; run cargo run --release --manifest-path tools/font-subset-eval/Cargo.toml -- --browser ${args.cases}\n`,
    );
    process.exit(1);
  }
  const input = JSON.parse(readFileSync(args.cases, "utf8"));
  const browser = await chromium.launch();
  let result;
  let version;
  try {
    version = browser.version();
    const page = await browser.newPage();
    await page.setContent("<!doctype html><title>font subsets</title>");
    result = await page.evaluate(measure, input);
  } finally {
    await browser.close();
  }
  for (const [name, t] of Object.entries(result.totals)) {
    process.stdout.write(
      `${name.padEnd(22)} loaded ${t.loaded}/${t.declarations} (failed ${t.failed})  runs equal ${t.runsEqual}/${t.runs}\n`,
    );
  }
  for (const u of result.unequal) process.stdout.write(`  unequal: ${JSON.stringify(u)}\n`);
  for (const r of result.rejected) process.stdout.write(`  rejected: ${JSON.stringify(r)}\n`);
  const out = `${JSON.stringify(
    {
      description:
        "ex-408: each subset of the browser scenes of upstream-subsets.json (upstream's own, and each subsetter's with ttf2woff2) registered with the CSS Font Loading API in Chromium (the font sanitizer runs: loaded counts the subsets it accepted, rejected lists the others) and each run of the scene's text in the face drawn at 64 px next to the original face (runsEqual: identical pixels and measureText width). Written by tools/font-subset-eval/browser/render.mjs.",
      candidates: result.totals,
      rejected: result.rejected,
    },
    null,
    2,
  )}\n`;
  process.stdout.write(`Chromium ${version}\n`);
  if (args.check) {
    if (!existsSync(REPORT) || readFileSync(REPORT, "utf8") !== out) {
      process.stderr.write("stale: tools/font-subset-eval/browser.json; run node tools/font-subset-eval/browser/render.mjs\n");
      process.exit(1);
    }
    process.stdout.write("browser.json is current\n");
    return;
  }
  writeFileSync(REPORT, out);
  process.stdout.write("wrote tools/font-subset-eval/browser.json\n");
};

await main();
