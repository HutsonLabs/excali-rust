#!/usr/bin/env node
// Host parsing fixtures for excali-core's `whatwg_url` (ex-109): what
// `new URL(input)` gives in Node (ada, the engine the library URL fixtures
// are recorded with) for hosts outside ASCII, where the domain goes through
// ada's IDNA (`ada::idna::to_ascii`). Upstream reads `hostname` and
// `pathname` of such URLs in validateLibraryUrl
// (packages/excalidraw/data/library.ts:497-524) and toValidURL
// (packages/common/src/url.ts:24-37).
//
//   node tools/goldens/url-host-fixtures.mjs            write the fixture
//   node tools/goldens/url-host-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/url-host-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/url-hosts.json:
//
//   { "description", "node", "ada", "unicode", "cases": [...],
//     "sweep": {...}, "random": {...} }
//
// - cases: { input, hostname, pathname } or { input, error: true } where
//   `new URL(input)` throws.
// - sweep: every code point from U+0080 to U+10FFFF but the surrogates, as
//   the host on its own and after `a` (`forms`, `{}` the code point). Each
//   block of `blockSize` code points has the CRC-32 of the UTF-8 of its
//   results, one line per input in order (the code point's forms in turn):
//   the hostname, or `!` where `new URL` throws.
// - random: `count` URLs, each a scheme from `schemes` and 1 to `maxTokens`
//   tokens from `tokens` (Park-Miller minimal standard generator from
//   `seed`: next = seed * 48271 mod 2^31 - 1, index = floor(next / 2^31-1 *
//   length), first the token count, then the scheme, then each token), then
//   `/p`. Each chunk of `chunkSize` has the CRC-32 of its lines,
//   `hostname\tpathname` or `!`.
//
// The fixture is a function of the Node release (ada and its Unicode
// tables); tools/goldens/.node-version pins it, and generating under another
// release is refused.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { crc32 } from "node:zlib";

import { REPO_ROOT } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "url-hosts.json";
const NODE_VERSION = readFileSync(join(REPO_ROOT, "tools", "goldens", ".node-version"), "utf8").trim();

const usage = () => {
  process.stderr.write("usage: url-host-fixtures.mjs [--check] [--out DIR]\n");
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

const CASES = [
  // A combining mark ada's validity table (Unicode 13) does not have
  // starting a label, which UTS 46 with Unicode 17 data rejects.
  "https://\u1AD3/",
  "file://\u1AD3/",
  "https://\u1AD3.excalidraw.com/x",
  "https://\u0C3C/",
  "https://\u{1E6E3}/",
  "https://\u0897/",
  // ... and one it has.
  "https://\u0301/",
  "https://\u0301a/",
  "https://a.\u0301b/",
  "https://a\u0301/",
  "https://a\u1AD3/",
  // Right-to-left: ada's direction table (Unicode 13) and its per-label rule.
  "https://a\u{10D50}/",
  "https://a\u{10D70}/",
  "https://\u{10D70}/",
  "https://\u{10D40}/",
  "https://a\u0870/",
  "https://a\u05D0/",
  "https://\u05D0a/",
  "https://\u05D0/",
  "https://\u05D01/",
  "https://1\u05D0/",
  "https://\u05D0\u1AD3/",
  "https://\u05D0\u0301/",
  "https://1.\u05D0/",
  "https://a.\u05D0/",
  "https://\u05D0.a/",
  "https://\u05D0.1a/",
  "https://\u0627\u0661\u06F1/",
  "https://\u0627\u0661/",
  "https://\u0627\u06F1/",
  "https://\u0661\u0627/",
  "https://\u0627-\u0628/",
  "https://\u05D0\u0021/",
  "https://ab\u05D0.c/",
  // Joiners (ContextJ).
  "https://a\u200Db/",
  "https://a\u094D\u200Db/",
  "https://\u0915\u094D\u200D\u0937/",
  "https://a\u200Cb/",
  "https://\u0628\u200C\u0628/",
  "https://\u0628\u200C/",
  "https://\u200C\u0628/",
  "https://\u0915\u094D\u200C\u05D0a/",
  "https://\u0628\u200C\u0627/",
  "https://\u0627\u200C\u0628/",
  "https://\uA872\u200C\u0627/",
  // Normalization (NFC) after mapping.
  "https://e\u0301/",
  "https://E\u0301/",
  "https://\u212B/",
  "https://a\u0323\u0302/",
  "https://a\u0302\u0323/",
  "https://\u1100\u1161\u11A8/",
  "https://\uAC00\u11A8/",
  "https://\u1100\u1161/",
  "https://\u0041\u030A\u0327/",
  "https://\u0FB2\u0F80/",
  "https://\u{11131}\u{11127}/",
  // Mapping: case, width, ignored, deviation, disallowed.
  "https://\u00C9XAMPLE.com/",
  "https://\uFF45\uFF58\uFF41\uFF4D\uFF50\uFF4C\uFF45.com/",
  "https://a\u3002b/",
  "https://a\uFF0Eb\uFF61c/",
  "https://\uFF11\uFF12\uFF17.0.0.1/",
  "https://\uFF10\uFF58\uFF17\uFF26.1/",
  "https://\uFF11.\uFF12/",
  "https://\u00e9.1/",
  "https://\u00AD/",
  "https://\u00AD.a/",
  "https://a\u00ADb/",
  "https://\u{E0100}a/",
  "https://fa\u00DF.de/",
  "https://\u03C2.com/",
  "https://\u0130.com/",
  "https://\u2100/",
  "https://\uFF03/",
  "https://\uFFFD/",
  "https://a\u{E0001}/",
  "https://a\u2488/",
  "https://\u2474/",
  "https://\u{1F4A9}/",
  "https://\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9/",
  // Percent-encoded.
  "https://%C3%A9/",
  "https://%c3%a9.excalidraw.com/",
  "https://%E1%AB%93/",
  "https://%C3/",
  "https://%ED%A0%80/",
  "https://%F4%90%80%80/",
  "https://%C0%AF/",
  "https://%C3%A9%2F/",
  // `xn--` labels in a domain that is not ASCII.
  "https://xn--ls8h.\u00e9/",
  "https://xn--9ca.\u00e9/",
  "https://XN--9CA.\u00e9/",
  "https://xn--e-ufa.\u00e9/",
  "https://xn--e-ufa/",
  "https://xn--trf.\u00e9/",
  "https://xn--a-ho6i.\u00e9/",
  "https://xn--zz.\u00e9/",
  "https://xn--.\u00e9/",
  "https://xn--a.\u00e9/",
  "https://xn--xn--a-ecp.\u00e9/",
  "https://xn--99999999999999a.\u00e9/",
  "https://xn--tda.\u00e9/",
  "https://xn--4db.\u00e9/",
  "https://xn--1-zhc.\u00e9/",
  "https://xn--ls8h\u00AD/",
  "https://xn--zz\u00AD/",
  "ws:\u00ADXN--A_xn--LOCALHOSTxn--ls8h",
  "https://xn--a_xn--localhostxn--ls8h/",
  // Labels, dots and ends.
  "https://\u00e9./",
  "https://\u00e9..a/",
  "https://.\u00e9/",
  "https://\u00e9.\u00e9/",
  // Where the host sits in the URL.
  "https://u:p@\u00e9:8080/x?q#h",
  "https:\u00e9/x",
  "https:\\\\\u00e9\\x",
  "ftp://\u00e9/",
  "ws://\u00e9/",
  "wss://\u05D0\u1AD3/",
  "http://\u00e9:99999/",
  "file://\u00e9/x",
  "file://\u00e9/C:/..",
  "file://\uFF4C\uFF4F\uFF43\uFF41\uFF4C\uFF48\uFF4F\uFF53\uFF54/x",
  "file://\u1AD3.\u05D0\u1AD3/x",
  "x://\u00e9/",
  "x://\u1AD3/",
  "https://[\u00e9]/",
  "https://raw.githubusercontent.com\u3002/excalidraw/excalidraw-libraries/x",
  "https://\u1AD3.raw.githubusercontent.com/excalidraw/excalidraw-libraries/x",
  "https://\u05D0\u1AD3.excalidraw.com/x",
];

const SWEEP = { blockSize: 0x1000, forms: ["https://{}/", "https://a{}/"] };

const RANDOM = {
  seed: 20260928,
  count: 20000,
  chunkSize: 1000,
  maxTokens: 6,
  schemes: ["https://", "http:", "file://", "ws:", "ftp://", "https://u@"],
  tokens: [
    "a", "B", "-", ".", "1", "_",
    "\u00e9", "e\u0301", "\u0301", "\u0327", "\u0323", "\u0302",
    "\u1AD3", "\u0897", "\u{1E6E3}",
    "\u05D0", "\u0627", "\u0628", "\u0661", "\u06F1", "\u{10D50}", "\u{10D40}", "\u0870",
    "\u200C", "\u200D", "\u094D", "\u0915",
    "\u00AD", "xn--", "XN--", "ls8h", "9ca", "trf",
    "\u1100", "\u1161", "\u11A8", "\uAC00",
    "\uFF21", "\uFF11", "\u3002", "\u1C8A", "\u212B", "\u{1F4A9}",
    "\u00DF", "\u03C2", "\u0130", "\u2100",
    "%C3%A9", "%41", "%E1%AB%93", "%C3", ":8", "@",
  ],
};

// -- runs -----------------------------------------------------------------------

const parse = (input) => {
  try {
    const url = new URL(input);
    return { hostname: url.hostname, pathname: url.pathname };
  } catch {
    return null;
  }
};

const runCase = (input) => {
  const r = parse(input);
  return r ? { input, ...r } : { input, error: true };
};

const crcOf = (lines) => crc32(Buffer.from(lines.map((l) => `${l}\n`).join(""), "utf8")) >>> 0;

const runSweep = () => {
  const blocks = [];
  for (let start = 0; start < 0x110000; start += SWEEP.blockSize) {
    const lines = [];
    for (let cp = Math.max(start, 0x80); cp < start + SWEEP.blockSize; cp++) {
      if (cp >= 0xd800 && cp <= 0xdfff) continue;
      const c = String.fromCodePoint(cp);
      for (const form of SWEEP.forms) {
        const r = parse(form.replace("{}", c));
        lines.push(r ? r.hostname : "!");
      }
    }
    if (lines.length) blocks.push({ start, inputs: lines.length, crc32: crcOf(lines) });
  }
  return { ...SWEEP, blocks };
};

export const randomInputs = ({ seed, count, maxTokens, schemes, tokens }) => {
  let state = seed;
  const pick = (n) => {
    state = (state * 48271) % 2147483647;
    return Math.floor((state / 2147483647) * n);
  };
  const out = [];
  for (let i = 0; i < count; i++) {
    const n = 1 + pick(maxTokens);
    let s = schemes[pick(schemes.length)];
    for (let k = 0; k < n; k++) s += tokens[pick(tokens.length)];
    out.push(`${s}/p`);
  }
  return out;
};

const runRandom = () => {
  const inputs = randomInputs(RANDOM);
  const chunks = [];
  let accepted = 0;
  for (let i = 0; i < inputs.length; i += RANDOM.chunkSize) {
    const lines = inputs.slice(i, i + RANDOM.chunkSize).map((input) => {
      const r = parse(input);
      if (r) accepted++;
      return r ? `${r.hostname}\t${r.pathname}` : "!";
    });
    chunks.push(crcOf(lines));
  }
  return { ...RANDOM, accepted, chunks };
};

const buildFixture = () => {
  const fixture = {
    description:
      "new URL(input) in Node (ada) for hosts outside ASCII: hostname and pathname, " +
      "the domain through ada::idna::to_ascii. Generated by tools/goldens/url-host-fixtures.mjs.",
    node: process.version,
    ada: process.versions.ada,
    unicode: process.versions.unicode,
    cases: CASES.map(runCase),
    sweep: runSweep(),
    random: runRandom(),
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of invisible code points the attribution gate rejects.
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-\uffff]/g,
    (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

const main = () => {
  const args = parseArgs(process.argv.slice(2));
  if (process.version !== `v${NODE_VERSION}`) {
    process.stderr.write(
      `url-host-fixtures: Node ${process.version}, the fixture is recorded with v${NODE_VERSION} (tools/goldens/.node-version)\n`,
    );
    process.exit(1);
  }
  const text = buildFixture();
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("URL host fixtures are out of date: run node tools/goldens/url-host-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`URL host fixtures up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where}\n`);
};

if (import.meta.url === `file://${process.argv[1]}`) main();
