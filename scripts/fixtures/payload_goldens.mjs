// Generates crates/excali-core/tests/fixtures/payload/goldens.json (ex-110).
// Run through scripts/fixtures/payload-goldens.sh, which fetches and verifies
// the pako release upstream locks and passes its location here.
//
// Upstream's encode.ts is imported verbatim from the pinned checkout (Node
// strips the TypeScript types). Two module specifiers are redirected: "pako"
// to the verified pako package, and "./encryption" (which needs a browser
// `window.crypto` and is not used by encode/decode) to a stub that throws.
// Everything written to the goldens file is the output of upstream code:
// `deflate` from pako, and `encode`/`decode` from encode.ts.
//
// The output is ASCII only: every non-ASCII code unit is written as a \uXXXX
// escape, so byte strings survive the repository's text-file gates and parse
// back to the same JS string.

import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { registerHooks } from "node:module";
import { join, relative } from "node:path";
import { pathToFileURL } from "node:url";
import { gunzipSync } from "node:zlib";
import { parseArgs } from "node:util";

const { values: opts } = parseArgs({
  options: {
    upstream: { type: "string" },
    pako: { type: "string" },
    root: { type: "string" },
    "pako-version": { type: "string" },
  },
});
for (const k of ["upstream", "pako", "root", "pako-version"]) {
  if (!opts[k]) throw new Error(`missing --${k}`);
}

const pakoUrl = pathToFileURL(join(opts.pako, "index.js")).href;
const encodeUrl = pathToFileURL(
  join(opts.upstream, "packages/excalidraw/data/encode.ts"),
).href;
const ENCRYPTION_STUB =
  "data:text/javascript," +
  encodeURIComponent(
    "const no = () => { throw new Error('encryption is not available in the goldens generator'); };" +
      "export const encryptData = no; export const decryptData = no;",
  );

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "pako") {
      return { url: pakoUrl, format: "commonjs", shortCircuit: true };
    }
    if (specifier === "./encryption" && context.parentURL === encodeUrl) {
      return { url: ENCRYPTION_STUB, format: "module", shortCircuit: true };
    }
    return nextResolve(specifier, context);
  },
});

const { encode, decode } = await import(encodeUrl);
const pako = (await import(pakoUrl)).default;

const upstreamCommit = readFileSync(join(opts.upstream, ".git/HEAD"), "utf8").trim();

// --- inputs -----------------------------------------------------------------

// xorshift32; the Rust tests implement the same generator.
const xorshift32 = (seed) => {
  let x = seed >>> 0;
  return () => {
    x ^= x << 13;
    x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5;
    x >>>= 0;
    return x;
  };
};

const readRepo = (rel) => {
  const data = readFileSync(join(opts.root, rel));
  return rel.endsWith(".gz") ? gunzipSync(data) : data;
};

// Input specs are shared with the Rust tests
// (crates/excali-core/tests/payload_codec.rs, `materialize`).
const textOf = (spec) => {
  if ("text" in spec) return spec.text;
  if ("file" in spec) {
    return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
      readRepo(spec.file),
    );
  }
  if ("repeat" in spec) return spec.repeat.repeat(spec.count);
  if ("xorshift_text" in spec) {
    const { seed, count, alphabet } = spec.xorshift_text;
    const next = xorshift32(seed);
    let s = "";
    for (let i = 0; i < count; i++) s += alphabet[next() % alphabet.length];
    return s;
  }
  throw new Error(`not a text spec: ${JSON.stringify(spec)}`);
};

const bytesOf = (spec) => {
  if ("file_bytes" in spec) return new Uint8Array(readRepo(spec.file_bytes));
  if ("xorshift_bytes" in spec) {
    const { seed, count, mask } = spec.xorshift_bytes;
    const next = xorshift32(seed);
    const out = new Uint8Array(count);
    for (let i = 0; i < count; i++) out[i] = next() & mask;
    return out;
  }
  throw new Error(`not a bytes spec: ${JSON.stringify(spec)}`);
};

// --- golden values ----------------------------------------------------------

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const INLINE_LIMIT = 4096;

const bytesGolden = (bytes) => {
  const g = { len: bytes.length, sha256: sha256(bytes) };
  if (bytes.length <= INLINE_LIMIT) g.base64 = Buffer.from(bytes).toString("base64");
  return g;
};

const textGolden = (text) => {
  const utf8 = Buffer.from(text, "utf8");
  const g = { len: utf8.length, sha256: sha256(utf8) };
  if (utf8.length <= INLINE_LIMIT) g.text = text;
  return g;
};

// A decoded JS string as UTF-16 code units, so lone surrogates are
// representable: length, SHA-256 of the UTF-16LE bytes, and the units
// themselves when short.
const utf16Golden = (s) => {
  const le = Buffer.alloc(s.length * 2);
  for (let i = 0; i < s.length; i++) le.writeUInt16LE(s.charCodeAt(i), i * 2);
  const g = { len: s.length, sha256: sha256(le) };
  if (s.length <= 256) g.units = Array.from({ length: s.length }, (_, i) => s.charCodeAt(i));
  return g;
};

const byteStringOf = (bytes) => String.fromCharCode(...bytes);

// --- cases ------------------------------------------------------------------

const WORDS = [
  "the ", "excalidraw ", "element ", "arrow ", "élément ", "straße ",
  "λόγος ", "жираф ", "中文 ", "テキスト ", "😀 ", "✓ ", "\n", "  ",
  '{"type": "rectangle", ', '"x": ', '"y": ', "12.5", ", ", "}", "[", "]",
  '"strokeColor": "#1e1e1e"', '"roughness": 1', "0", "1", "2", "3",
];
const CHARS = Array.from(
  "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 \n\t" +
    "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~" +
    "éßüñçøåλжд中文字テキスト한국어😀🎨✓",
);

const smiley = readFileSync(
  join(
    opts.root,
    "fixtures/upstream/packages/excalidraw/tests/fixtures/smiley_embedded_v2.svg",
  ),
  "utf8",
);
const smileyWrapper = JSON.parse(
  atob(/<!-- payload-start -->\s*(.+?)\s*<!-- payload-end -->/.exec(smiley)[1]),
);

const TEXT_CASES = [
  { name: "empty", input: { text: "" } },
  { name: "single_char", input: { text: "a" } },
  { name: "ascii", input: { text: "Hello, World!" } },
  { name: "unicode", input: { text: "😀 ünïcödé — 中文 テキスト ✓ \u0000\u001f\u007f" } },
  { name: "json_escapes", input: { text: '"quoted"\\ \b\f\n\r\t </script>' } },
  { name: "smiley_scene", input: { text: decode(smileyWrapper) } },
  {
    name: "empty_scene",
    input: { file: "crates/excali-core/tests/fixtures/empty-scene.excalidraw" },
  },
  {
    name: "fixture_library",
    input: {
      file: "fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib",
    },
  },
  {
    name: "library_enterprise_integration_patterns",
    input: {
      file: "fixtures/libraries/stuc2010/enterprise-integration-patterns.excalidrawlib.gz",
    },
  },
  { name: "run_a_100000", input: { repeat: "a", count: 100000 } },
  { name: "run_emoji_40000", input: { repeat: "😀", count: 40000 } },
  {
    name: "words_60000",
    input: { xorshift_text: { seed: 110, count: 60000, alphabet: WORDS } },
  },
  {
    name: "chars_50000",
    input: { xorshift_text: { seed: 2026, count: 50000, alphabet: CHARS } },
  },
];

const BYTE_CASES = [
  { name: "random_100000", input: { xorshift_bytes: { seed: 1, count: 100000, mask: 255 } } },
  { name: "nibbles_70000", input: { xorshift_bytes: { seed: 7, count: 70000, mask: 15 } } },
  { name: "bits_140000", input: { xorshift_bytes: { seed: 9, count: 140000, mask: 1 } } },
];
const libDir = join(opts.root, "fixtures/libraries");
for (const author of readdirSync(libDir).sort()) {
  const dir = join(libDir, author);
  if (!statSync(dir).isDirectory()) continue;
  for (const f of readdirSync(dir).sort()) {
    if (!f.endsWith(".excalidrawlib.gz")) continue;
    const rel = relative(opts.root, join(dir, f));
    BYTE_CASES.push({ name: `library:${author}/${f}`, input: { file_bytes: rel } });
  }
}

const encodeGoldens = TEXT_CASES.map(({ name, input }) => {
  const text = textOf(input);
  const compressed = encode({ text });
  const raw = encode({ text, compress: false });
  if (!compressed.compressed || raw.compressed) throw new Error(`${name}: compression flags`);
  if (decode(compressed) !== text || decode(raw) !== text) {
    throw new Error(`${name}: decode(encode(text)) !== text`);
  }
  const deflated = pako.deflate(text);
  if (byteStringOf(deflated) !== compressed.encoded) {
    throw new Error(`${name}: encode() is not the byte string of pako.deflate()`);
  }
  return {
    name,
    input,
    deflate: bytesGolden(deflated),
    encoded_json: textGolden(JSON.stringify(compressed)),
    raw_json: textGolden(JSON.stringify(raw)),
  };
});

const deflateGoldens = BYTE_CASES.map(({ name, input }) => ({
  name,
  input,
  deflate: bytesGolden(pako.deflate(bytesOf(input))),
}));

// decode(): what upstream returns for wrappers encode() never produces but a
// file can contain. `result` is {utf16}, {error} or {undefined: true}.
const z = (x) => byteStringOf(pako.deflate(x));
const bytes = (...xs) => new Uint8Array(xs);
const DECODE_CASES = [
  ["raw_ascii", { encoding: "bstring", compressed: false, encoded: "plain" }],
  ["raw_utf8", { encoding: "bstring", compressed: false, encoded: byteStringOf(Buffer.from("é中😀")) }],
  ["raw_bom_stripped", { encoding: "bstring", compressed: false, encoded: "\xef\xbb\xbfhi" }],
  ["raw_double_bom", { encoding: "bstring", compressed: false, encoded: "\xef\xbb\xbf\xef\xbb\xbfhi" }],
  ["raw_invalid_utf8", { encoding: "bstring", compressed: false, encoded: "a\xffb\xc3(c\xe2\x82d\xf0\x9f\x98" }],
  ["raw_overlong_and_surrogates", { encoding: "bstring", compressed: false, encoded: "\xc0\xaf\xe0\x80\x80\xed\xa0\x80\xf4\x90\x80\x80\xf8\x88\x80\x80\x80" }],
  ["raw_code_units_above_ff", { encoding: "bstring", compressed: false, encoded: "Ł☃x😀" }],
  ["raw_missing_compressed_flag", { encoding: "bstring", encoded: "no flag" }],
  ["raw_with_version", { version: "1", encoding: "bstring", compressed: false, encoded: "v" }],
  ["compressed_utf8", { encoding: "bstring", compressed: true, encoded: z("é中😀 text") }],
  ["compressed_bom_kept", { encoding: "bstring", compressed: true, encoded: z(bytes(0xef, 0xbb, 0xbf, 0x68, 0x69)) }],
  ["compressed_invalid_utf8", { encoding: "bstring", compressed: true, encoded: z(bytes(0x61, 0xff, 0x62, 0xc3, 0x28, 0x63, 0xe2, 0x82, 0x64, 0xfe, 0x41)) }],
  ["compressed_overlong_and_surrogates", { encoding: "bstring", compressed: true, encoded: z(bytes(0xc0, 0xaf, 0xe0, 0x80, 0x80, 0xed, 0xa0, 0x80, 0xed, 0xb0, 0x80, 0xf4, 0x90, 0x80, 0x80, 0xf8, 0x88, 0x80, 0x80, 0x80, 0x5a)) }],
  ["compressed_trailing_partial_sequence", { encoding: "bstring", compressed: true, encoded: z(bytes(0x61, 0x62, 0xe2, 0x82)) }],
  ["compressed_only_partial_sequence", { encoding: "bstring", compressed: true, encoded: z(bytes(0xe2, 0x82)) }],
  ["compressed_chunk_border_sequence", { encoding: "bstring", compressed: true, encoded: z(Buffer.concat([Buffer.alloc(65534, 0x61), Buffer.from("€€ tail")])) }],
  ["compressed_chunk_border_invalid", { encoding: "bstring", compressed: true, encoded: z(Buffer.concat([Buffer.alloc(65535, 0x61), bytes(0xf0, 0x41, 0x42, 0x43, 0x44)])) }],
  ["compressed_chunk_border_continuations", { encoding: "bstring", compressed: true, encoded: z(Buffer.concat([bytes(0x41), Buffer.alloc(70000, 0x80), bytes(0x42)])) }],
  ["compressed_empty", { encoding: "bstring", compressed: true, encoded: z("") }],
  ["compressed_gzip", { encoding: "bstring", compressed: true, encoded: byteStringOf(pako.gzip("gzip wrapped é")) }],
  ["compressed_gzip_with_header", { encoding: "bstring", compressed: true, encoded: byteStringOf(pako.gzip("named", { header: { name: "scene.json", comment: "c", extra: bytes(1, 2, 3), hcrc: true, time: 1 } })) }],
  ["compressed_concatenated", { encoding: "bstring", compressed: true, encoded: z("first ") + z("second") }],
  ["compressed_trailing_zeros", { encoding: "bstring", compressed: true, encoded: z("abc") + "\x00\x00\x07" }],
  ["compressed_trailing_garbage", { encoding: "bstring", compressed: true, encoded: z("abc") + "junk" }],
  ["compressed_truncated", { encoding: "bstring", compressed: true, encoded: z("some text that gets cut off").slice(0, -6) }],
  ["compressed_bad_checksum", { encoding: "bstring", compressed: true, encoded: z("abc").slice(0, -1) + "\x00" }],
  ["compressed_bad_header", { encoding: "bstring", compressed: true, encoded: "\x78\x00" + z("abc").slice(2) }],
  ["compressed_bad_method", { encoding: "bstring", compressed: true, encoded: "\x79\x18" + z("abc").slice(2) }],
  ["compressed_window_too_large", { encoding: "bstring", compressed: true, encoded: "\x88\x98" + z("abc").slice(2) }],
  ["compressed_preset_dictionary", { encoding: "bstring", compressed: true, encoded: byteStringOf(pako.deflate("abc", { dictionary: "abc" })) }],
  ["compressed_raw_deflate", { encoding: "bstring", compressed: true, encoded: byteStringOf(pako.deflateRaw("abc")) }],
  ["compressed_empty_input", { encoding: "bstring", compressed: true, encoded: "" }],
  ["compressed_stored_block", { encoding: "bstring", compressed: true, encoded: byteStringOf(pako.deflate("stored", { level: 0 })) }],
  ["compressed_code_units_above_ff", { encoding: "bstring", compressed: true, encoded: Array.from(z("abc")).map((c) => String.fromCharCode(c.charCodeAt(0) | 0x100)).join("") }],
  ["unknown_encoding", { encoding: "base64", compressed: false, encoded: "YQ==" }],
];

const decodeGoldens = DECODE_CASES.map(([name, data]) => {
  let result;
  try {
    const out = decode(data);
    result = out === undefined ? { undefined: true } : { utf16: utf16Golden(out) };
  } catch (error) {
    result = { error: String(error instanceof Error ? error.message : error) };
  }
  return { name, data, result };
});

const goldens = {
  about:
    "Generated by scripts/fixtures/payload-goldens.sh from upstream encode.ts and pako; do not edit.",
  upstream_commit: upstreamCommit,
  pako: opts["pako-version"],
  encode: encodeGoldens,
  deflate: deflateGoldens,
  decode: decodeGoldens,
};

const ascii = JSON.stringify(goldens, null, 1).replace(
  /[\u007f-￿]/g,
  (c) => "\\u" + c.charCodeAt(0).toString(16).padStart(4, "0"),
);
process.stdout.write(ascii + "\n");
