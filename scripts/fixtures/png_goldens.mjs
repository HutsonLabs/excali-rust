// Generates crates/excali-core/tests/fixtures/png/goldens.json (ex-111), and
// with --decode-dir checks that upstream decodes the PNGs the Rust port
// writes. Run through scripts/fixtures/png-goldens.sh, which fetches and
// verifies the npm releases upstream locks and passes their locations here.
//
// Upstream's image.ts and encode.ts are imported verbatim from the pinned
// checkout (Node strips the TypeScript types), on the locked png-chunk-text,
// png-chunks-encode, png-chunks-extract (with crc-32 and sliced) and pako.
// Module specifiers image.ts uses that pull in the rest of the app are
// redirected:
//
// - "@excalidraw/common" to a module holding IMAGE_MIME_TYPES,
//   STRING_MIME_TYPES, MIME_TYPES and EXPORT_DATA_TYPES, whose declarations
//   are cut verbatim out of packages/common/src/constants.ts;
// - "./blob" to a module holding `blobToArrayBuffer`, cut verbatim out of
//   packages/excalidraw/data/blob.ts;
// - "./encode" to encode.ts, and from there "./encryption" (which needs a
//   browser `window.crypto` and is not used by encode/decode) to a stub that
//   throws.
//
// Everything written as an expected value is the output of upstream code.
// The output is ASCII only: every non-ASCII code unit is written as a \uXXXX
// escape.

import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { registerHooks, stripTypeScriptTypes } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { crc32 } from "node:zlib";
import { parseArgs } from "node:util";

const { values: opts } = parseArgs({
  options: {
    upstream: { type: "string" },
    root: { type: "string" },
    package: { type: "string", multiple: true },
    "decode-dir": { type: "string" },
  },
});
for (const k of ["upstream", "root", "package"]) {
  if (!opts[k]) throw new Error(`missing --${k}`);
}

// --- module wiring ----------------------------------------------------------

const packages = new Map();
for (const p of opts.package) {
  const i = p.indexOf("=");
  const dir = p.slice(i + 1);
  let main = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).main || "index.js";
  if (!main.endsWith(".js")) main += ".js";
  packages.set(p.slice(0, i), { dir, url: pathToFileURL(join(dir, main)).href });
}
const versionOf = (name) =>
  JSON.parse(readFileSync(join(packages.get(name).dir, "package.json"), "utf8")).version;

const upstreamFile = (rel) => join(opts.upstream, rel);
const imageUrl = pathToFileURL(upstreamFile("packages/excalidraw/data/image.ts")).href;
const encodeUrl = pathToFileURL(upstreamFile("packages/excalidraw/data/encode.ts")).href;

// `export const NAME ... ;` (or `= (...) => { ... };`) cut out of a source
// file, up to the first line that is exactly the closing `};` / `} as const;`.
const cutDeclaration = (file, name) => {
  const lines = readFileSync(upstreamFile(file), "utf8").split("\n");
  const start = lines.findIndex((l) => l.startsWith(`export const ${name} =`));
  if (start < 0) throw new Error(`${name} not found in ${file}`);
  const end = lines.findIndex((l, i) => i > start && /^}( as const)?;$/.test(l));
  if (end < 0) throw new Error(`end of ${name} not found in ${file}`);
  return lines.slice(start, end + 1).join("\n");
};
const moduleUrl = (ts) =>
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(ts));

const COMMON_STUB = moduleUrl(
  ["IMAGE_MIME_TYPES", "STRING_MIME_TYPES", "MIME_TYPES", "EXPORT_DATA_TYPES"]
    .map((n) => cutDeclaration("packages/common/src/constants.ts", n))
    .join("\n"),
);
const BLOB_STUB = moduleUrl(cutDeclaration("packages/excalidraw/data/blob.ts", "blobToArrayBuffer"));
const ENCRYPTION_STUB =
  "data:text/javascript," +
  encodeURIComponent(
    "const no = () => { throw new Error('encryption is not available in the goldens generator'); };" +
      "export const encryptData = no; export const decryptData = no;",
  );

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (packages.has(specifier)) {
      return { url: packages.get(specifier).url, format: "commonjs", shortCircuit: true };
    }
    if (context.parentURL === imageUrl) {
      if (specifier === "@excalidraw/common") return { url: COMMON_STUB, format: "module", shortCircuit: true };
      if (specifier === "./blob") return { url: BLOB_STUB, format: "module", shortCircuit: true };
      if (specifier === "./encode") return { url: encodeUrl, format: "module-typescript", shortCircuit: true };
    }
    if (specifier === "./encryption" && context.parentURL === encodeUrl) {
      return { url: ENCRYPTION_STUB, format: "module", shortCircuit: true };
    }
    return nextResolve(specifier, context);
  },
});

const { encodePngMetadata, decodePngMetadata, getTEXtChunk } = await import(imageUrl);
const { encode } = await import(encodeUrl);
const pako = (await import(packages.get("pako").url)).default;
const encodeChunks = (await import(packages.get("png-chunks-encode").url)).default;
const extractChunks = (await import(packages.get("png-chunks-extract").url)).default;

// decodePngMetadata logs what it catches; the goldens record what it throws.
const log = (line) => process.stderr.write(`${line}\n`);
console.error = () => {};

const blobOf = (bytes) => new Blob([bytes], { type: "image/png" });
const bytesOfBlob = async (blob) => new Uint8Array(await blob.arrayBuffer());

// --- --decode-dir: upstream reads what the port wrote ------------------------

if (opts["decode-dir"]) {
  const dir = opts["decode-dir"];
  const names = readdirSync(dir).filter((n) => n.endsWith(".png")).sort();
  if (names.length === 0) throw new Error(`no PNGs in ${dir}`);
  let failed = 0;
  for (const name of names) {
    const want = readFileSync(join(dir, name.replace(/\.png$/, ".txt")), "utf8");
    let got;
    try {
      got = await decodePngMetadata(blobOf(readFileSync(join(dir, name))));
    } catch (error) {
      got = `<threw ${error.message}>`;
    }
    if (got === want) {
      log(`ok   ${name}: upstream decodePngMetadata gives the embedded text (${want.length} code units)`);
    } else {
      failed++;
      log(`FAIL ${name}: upstream decoded ${JSON.stringify(String(got).slice(0, 200))}`);
    }
  }
  if (failed) {
    log(`${failed} of ${names.length} PNGs written by the port did not decode in upstream`);
    process.exit(1);
  }
  log(`all ${names.length} PNGs written by the port decode in upstream image.ts`);
  process.exit(0);
}

// --- inputs -----------------------------------------------------------------

const upstreamCommit = readFileSync(join(opts.upstream, ".git/HEAD"), "utf8").trim();
const FIXTURE_DIR = "fixtures/upstream/packages/excalidraw/tests/fixtures";
const readRepo = (rel) => new Uint8Array(readFileSync(join(opts.root, rel)));

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

// Input specs are shared with the Rust tests
// (crates/excali-core/tests/png_payload.rs, `materialize`).
const materialize = (spec) => {
  if ("file" in spec) return readRepo(spec.file);
  if ("base64" in spec) return new Uint8Array(Buffer.from(spec.base64, "base64"));
  if ("prefix" in spec) return readRepo(spec.prefix.file).slice(0, spec.prefix.len);
  if ("flip" in spec) {
    const { file, seed, count } = spec.flip;
    const out = readRepo(file);
    const next = xorshift32(seed);
    for (let i = 0; i < count; i++) {
      const pos = next() % out.length;
      out[pos] ^= (next() & 0xff) | 1;
    }
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

// A JS string: its UTF-16 length and the SHA-256 of its UTF-16LE code units,
// and the string itself when short (and `inline`).
const STRING_INLINE_LIMIT = 1024;
const stringGolden = (s, inline = true) => {
  const le = Buffer.alloc(s.length * 2);
  for (let i = 0; i < s.length; i++) le.writeUInt16LE(s.charCodeAt(i), i * 2);
  const g = { len: s.length, sha256: sha256(le) };
  if (inline && s.length <= STRING_INLINE_LIMIT) g.text = s;
  return g;
};

const outcome = async (f, ok) => {
  try {
    const v = await f();
    return v === undefined ? { undefined: true } : ok(v);
  } catch (error) {
    return { error: error.message };
  }
};

const decodeCase = async (name, input, inline = true) => {
  const bytes = materialize(input);
  return {
    name,
    input,
    text_chunk: await outcome(
      () => getTEXtChunk(blobOf(bytes)),
      (c) => (c === null ? { null: true } : { keyword: c.keyword, text: stringGolden(c.text, inline) }),
    ),
    decoded: await outcome(() => decodePngMetadata(blobOf(bytes)), (s) => ({ ok: stringGolden(s) })),
  };
};

// --- hand-built PNGs ----------------------------------------------------------

const latin1 = (s) => {
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c > 0xff) throw new Error(`not latin-1: ${s}`);
    out[i] = c;
  }
  return out;
};
const concat = (...parts) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let i = 0;
  for (const p of parts) {
    out.set(p, i);
    i += p.length;
  }
  return out;
};
const u32 = (n) => new Uint8Array([n >>> 24, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff]);
// A chunk as bytes, with the CRC of its name and data unless `crc` is given.
const rawChunk = (name, data, crc) =>
  concat(u32(data.length), latin1(name), data, u32(crc ?? crc32(concat(latin1(name), data))));

const KEYWORD = "application/vnd.excalidraw+json";
const SIGNATURE = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const smileyChunks = extractChunks(readRepo(`${FIXTURE_DIR}/smiley.png`));
const byName = (n) => smileyChunks.find((c) => c.name === n);
const IHDR = rawChunk("IHDR", byName("IHDR").data);
const PLTE = rawChunk("PLTE", byName("PLTE").data);
const IDAT = rawChunk("IDAT", byName("IDAT").data);
const IEND = rawChunk("IEND", new Uint8Array(0));
const png = (...chunks) => concat(SIGNATURE, ...chunks);
// smiley.png with `extra` chunks inserted before IEND.
const smileyWith = (...extra) => png(IHDR, PLTE, IDAT, ...extra, IEND);
const tEXtRaw = (bytes) => rawChunk("tEXt", bytes);
const tEXtOf = (keyword, text) => tEXtRaw(concat(latin1(keyword), new Uint8Array([0]), latin1(text)));
const payload = (text) => tEXtOf(KEYWORD, text);
const b64 = (bytes) => ({ base64: Buffer.from(bytes).toString("base64") });
const byteStringOf = (bytes) => String.fromCharCode(...bytes);
const z = (s) => byteStringOf(pako.deflate(s));

const SCENE = JSON.stringify({
  type: "excalidraw",
  version: 2,
  source: "https://excalidraw.com",
  elements: [{ id: "A", type: "text", text: "\u00e9 \u{1F600}" }],
  appState: {},
  files: {},
});
const wrapper = (fields) => JSON.stringify({ version: "1", encoding: "bstring", compressed: true, encoded: z(SCENE), ...fields });

// A chunk that claims `claim` data bytes but whose file ends after `have` of
// them. png-chunks-extract reads past the end as zeros, the CRC included, so
// the chunk only passes the CRC check (and the error becomes "no IEND")
// when the CRC of the zero-padded chunk is 0. The last four data bytes
// present are solved for so that it is: CRC-32 is affine over GF(2) in them.
const zeroPaddedCrcMatch = (claim, have) => {
  const name = latin1("IDAT");
  const crcOf = (tail) => {
    const chunk = new Uint8Array(4 + claim);
    chunk.set(name, 0);
    for (let i = 0; i < have - 4; i++) chunk[4 + i] = (i * 7 + 3) & 0xff;
    chunk.set(tail, have);
    return crc32(chunk) >>> 0;
  };
  const tailOf = (bits) => new Uint8Array([bits & 0xff, (bits >>> 8) & 0xff, (bits >>> 16) & 0xff, bits >>> 24]);
  const base = crcOf(tailOf(0));
  // Column i: the CRC change from flipping bit i of the tail.
  const cols = Array.from({ length: 32 }, (_, i) => (crcOf(tailOf((1 << i) >>> 0)) ^ base) >>> 0);
  // Solve sum(x_i * cols[i]) = base by Gaussian elimination on rows.
  const rows = [];
  for (let r = 0; r < 32; r++) {
    let row = 0;
    for (let i = 0; i < 32; i++) if ((cols[i] >>> r) & 1) row |= 1 << i;
    rows.push({ row: row >>> 0, rhs: (base >>> r) & 1 });
  }
  const pivots = [];
  let rank = 0;
  for (let c = 0; c < 32; c++) {
    const p = rows.findIndex((x, i) => i >= rank && (x.row >>> c) & 1);
    if (p < 0) continue;
    [rows[rank], rows[p]] = [rows[p], rows[rank]];
    for (let i = 0; i < 32; i++) {
      if (i !== rank && (rows[i].row >>> c) & 1) {
        rows[i].row = (rows[i].row ^ rows[rank].row) >>> 0;
        rows[i].rhs ^= rows[rank].rhs;
      }
    }
    pivots.push(c);
    rank++;
  }
  let x = 0;
  pivots.forEach((c, i) => {
    if (rows[i].rhs) x |= 1 << c;
  });
  const tail = tailOf(x >>> 0);
  if (crcOf(tail) !== 0) throw new Error("zero-padded CRC solve failed");
  const data = new Uint8Array(have);
  for (let i = 0; i < have - 4; i++) data[i] = (i * 7 + 3) & 0xff;
  data.set(tail, have - 4);
  return png(IHDR, concat(u32(claim), name, data));
};

const LENGTH_VALUES = [
  0, 2, -0.9, -1, 0.5, 1e300, true, false, null, "0", "3", " -0.5 ", "-1", "abc", "",
  "  ", "0x0", "0x1", "0b0", "0o7", "Infinity", "-Infinity", "1e-400", "\u00a0 0 \n", "+0", "-0", ".0", "0.",
  "1_0", "00", [], [0], [1], [[]], ["-0.5"], [1, 2], {}, { length: 1 },
  // ToPrimitive throws on an own toString key; valueOf never gives a
  // primitive, so { valueOf } reads as "[object Object]".
  { toString: 1 }, [{ toString: 1 }], { valueOf: 1 },
];
const ENCODED_VALUES = [
  ["null", null], ["number", 5], ["zero", 0], ["true", true], ["false", false],
  ["empty_array", []], ["array", ["a"]], ["array_of_empty", [[]]], ["object", {}],
  ...LENGTH_VALUES.map((v, i) => [`length_${i}`, { length: v }]),
];

const HAND = [
  ["no_text_chunk", smileyWith()],
  ["keyword_other", smileyWith(tEXtOf("Comment", wrapper({})))],
  ["first_text_wins_other_keyword", smileyWith(tEXtOf("Comment", "x"), payload(wrapper({})))],
  ["first_text_wins", smileyWith(payload(JSON.stringify({ type: "excalidraw", elements: [] })), payload(wrapper({})))],
  ["text_before_idat", png(IHDR, PLTE, payload(wrapper({})), IDAT, IEND)],
  ["text_right_after_ihdr_only", png(IHDR, payload(wrapper({})), IEND)],
  ["other_chunks_named_like_text", smileyWith(rawChunk("TEXT", latin1(`${KEYWORD}\0{}`)), rawChunk("zTXt", latin1(`${KEYWORD}\0\0x`)))],
  ["keyword_without_separator", smileyWith(tEXtRaw(latin1(KEYWORD)))],
  ["keyword_prefix_only", smileyWith(tEXtOf("application/vnd.excalidraw", wrapper({})))],
  ["keyword_case", smileyWith(tEXtOf(KEYWORD.toUpperCase(), wrapper({})))],
  ["empty_text_chunk", smileyWith(tEXtRaw(new Uint8Array(0)))],
  ["keyword_leading_nul", smileyWith(tEXtRaw(concat(new Uint8Array([0]), latin1(`${KEYWORD}\0{}`))))],
  ["nul_in_text", smileyWith(tEXtRaw(latin1(`${KEYWORD}\0{"a":\0}`)))],
  ["nul_in_text_other_keyword", smileyWith(tEXtRaw(latin1(`Comment\0a\0b`)))],
  ["nul_twice_after_keyword", smileyWith(tEXtRaw(latin1(`${KEYWORD}\0\0{}`)))],
  ["text_empty", smileyWith(payload(""))],
  ["text_not_json", smileyWith(payload("not json"))],
  ["text_number", smileyWith(payload("1"))],
  ["text_string", smileyWith(payload('"encoded"'))],
  ["text_true", smileyWith(payload("true"))],
  ["text_null", smileyWith(payload("null"))],
  ["text_array", smileyWith(payload('["encoded"]'))],
  ["text_object_empty", smileyWith(payload("{}"))],
  ["text_trailing_garbage", smileyWith(payload(`${wrapper({})}x`))],
  ["text_surrounding_whitespace", smileyWith(payload(` \n\t${wrapper({})}\r\n `))],
  ["text_latin1_bom", smileyWith(payload(`\u00ef\u00bb\u00bf${wrapper({})}`))],
  ["legacy_scene", smileyWith(payload(`{\n  "type": "excalidraw",\n  "elements": []\n}`))],
  ["legacy_scene_utf8_bytes", smileyWith(payload(byteStringOf(Buffer.from(SCENE, "utf8"))))],
  ["legacy_scene_latin1", smileyWith(payload('{"type":"excalidraw","elements":[{"text":"caf\u00e9"}]}'))],
  ["legacy_scene_escapes", smileyWith(payload('{"type":"excalidraw","x":"\\ud83d\\ude00 \\ud83d \\u00e9"}'))],
  ["legacy_type_library", smileyWith(payload('{"type":"excalidrawlib","libraryItems":[]}'))],
  ["legacy_type_missing", smileyWith(payload('{"elements":[]}'))],
  ["legacy_type_not_string", smileyWith(payload('{"type":["excalidraw"]}'))],
  ["legacy_type_duplicate_last_wins", smileyWith(payload('{"type":"excalidraw","type":"other"}'))],
  ["legacy_type_duplicate_last_wins_ok", smileyWith(payload('{"type":"other","type":"excalidraw"}'))],
  ["wrapper_compressed", smileyWith(payload(wrapper({})))],
  ["wrapper_with_type", smileyWith(payload(wrapper({ type: "excalidraw" })))],
  ["wrapper_uncompressed", smileyWith(payload(wrapper({ compressed: false, encoded: byteStringOf(Buffer.from(SCENE, "utf8")) })))],
  ["wrapper_uncompressed_invalid_utf8", smileyWith(payload(wrapper({ compressed: false, encoded: "a\u00ffb\u00c3(" })))],
  ["wrapper_compressed_missing", smileyWith(payload(JSON.stringify({ encoding: "bstring", encoded: "plain" })))],
  ["wrapper_unknown_encoding", smileyWith(payload(wrapper({ encoding: "base64" })))],
  ["wrapper_encoding_missing", smileyWith(payload(JSON.stringify({ compressed: true, encoded: z(SCENE) })))],
  ["wrapper_encoding_null", smileyWith(payload(wrapper({ encoding: null })))],
  ["wrapper_truncated_deflate", smileyWith(payload(wrapper({ encoded: z(SCENE).slice(0, -8) })))],
  ["wrapper_corrupt_deflate", smileyWith(payload(wrapper({ encoded: "x\u0000" + z(SCENE).slice(2) })))],
  ["wrapper_bad_checksum", smileyWith(payload(wrapper({ encoded: z(SCENE).slice(0, -1) + "\u0000" })))],
  ["wrapper_empty_encoded_compressed", smileyWith(payload(wrapper({ encoded: "" })))],
  ["wrapper_empty_encoded_uncompressed", smileyWith(payload(wrapper({ compressed: false, encoded: "" })))],
  ["wrapper_encoded_escapes_above_ff", smileyWith(payload('{"encoding":"bstring","compressed":false,"encoded":"\\u0141\\u2603x\\u0161"}'))],
  ["wrapper_encoded_lone_surrogate_escape", smileyWith(payload(`{"encoding":"bstring","compressed":false,"encoded":"a\\ud83db\\udc00\\ud83d\\ude00"}`))],
  ["wrapper_compressed_lone_surrogate_escape", smileyWith(payload(`{"encoding":"bstring","compressed":true,"encoded":${JSON.stringify(z("hi")).slice(0, -1)}\\ud800"}`))],
  ["wrapper_duplicate_encoded_last_wins", smileyWith(payload(`{"encoding":"bstring","compressed":false,"encoded":1,"encoded":"ok"}`))],
  ["wrapper_duplicate_encoded_last_not_string", smileyWith(payload(`{"encoding":"bstring","compressed":false,"encoded":"ok","encoded":null}`))],
  ["wrapper_compressed_truthy_string", smileyWith(payload(wrapper({ compressed: "no" })))],
  ["wrapper_compressed_zero", smileyWith(payload(wrapper({ compressed: 0, encoded: "raw" })))],
  ...ENCODED_VALUES.flatMap(([n, v]) => [
    [`encoded_${n}_compressed`, smileyWith(payload(JSON.stringify({ encoding: "bstring", compressed: true, encoded: v })))],
    [`encoded_${n}_uncompressed`, smileyWith(payload(JSON.stringify({ encoding: "bstring", compressed: false, encoded: v })))],
  ]),
  ["encoded_non_string_unknown_encoding", smileyWith(payload(JSON.stringify({ encoding: "x", compressed: true, encoded: null })))],
  // png-chunks-extract
  ["empty_file", new Uint8Array(0)],
  ...Array.from({ length: 8 }, (_, i) => {
    const bytes = smileyWith(payload(wrapper({})));
    bytes[i] ^= 0x20;
    return [`signature_byte_${i}`, bytes];
  }),
  ["signature_only", SIGNATURE],
  ["no_ihdr_first", png(PLTE, IHDR, IDAT, payload(wrapper({})), IEND)],
  ["iend_first", png(IEND, IHDR, payload(wrapper({})), IEND)],
  ["no_iend", png(IHDR, PLTE, IDAT, payload(wrapper({})))],
  ["crc_mismatch_idat", png(IHDR, PLTE, rawChunk("IDAT", byName("IDAT").data, 0x12345678), payload(wrapper({})), IEND)],
  ["crc_mismatch_text", smileyWith(rawChunk("tEXt", latin1(`${KEYWORD}\0${wrapper({})}`), 1))],
  ["crc_mismatch_after_text", smileyWith(payload(wrapper({})), rawChunk("zzzz", latin1("x"), 0))],
  ["crc_signed", smileyWith(tEXtOf("k", "crc with high bit"))],
  ["iend_with_data_and_bad_crc", png(IHDR, PLTE, IDAT, payload(wrapper({})), concat(u32(3), latin1("IEND"), latin1("abc"), u32(7)))],
  ["iend_without_crc", concat(png(IHDR, PLTE, IDAT, payload(wrapper({}))), u32(0), latin1("IEND"))],
  ["iend_truncated_name", concat(png(IHDR, PLTE, IDAT, payload(wrapper({}))), u32(0), latin1("IEN"))],
  ["text_after_iend_ignored", png(IHDR, PLTE, IDAT, IEND, payload(wrapper({})))],
  ["garbage_after_iend", concat(smileyWith(payload(wrapper({}))), latin1("trailing garbage"))],
  ["chunk_name_high_bytes", smileyWith(rawChunk("\u00e9\u00ff\u0080x", latin1("data")), payload(wrapper({})))],
  ["chunk_name_high_bytes_bad_crc", smileyWith(rawChunk("\u00e9\u00ff\u0080x", latin1("data"), 5))],
  ["chunk_name_nul_bad_crc", smileyWith(rawChunk("\u0000\u0000\u0000\u0000", latin1("data"), 5))],
  ["truncated_claims_more_than_left", concat(png(IHDR), u32(1000), latin1("IDAT"), latin1("short"))],
  ["truncated_in_length_field", concat(png(IHDR), new Uint8Array([0, 0]))],
  // A 0xFFFFFFFF length: png-chunks-extract allocates length + 4 bytes
  // (4 GiB) and reads zeros past the end into them before the CRC check.
  // About a minute and 4.4 GB resident per call; the port gets the same
  // CRC without allocating.
  ["huge_chunk_length", concat(png(IHDR), u32(0xffffffff), latin1("IDAT"), latin1("some data"))],
  ["zero_padded_crc_match", zeroPaddedCrcMatch(100000, 16)],
  ["zero_padded_crc_match_small", zeroPaddedCrcMatch(9, 4)],
];

// --- cases --------------------------------------------------------------------

const FIXTURES = ["test_embedded_v1.png", "smiley_embedded_v2.png", "smiley.png", "deer.png"];
const decode = [];
for (const f of FIXTURES) decode.push(await decodeCase(`fixture_${f}`, { file: `${FIXTURE_DIR}/${f}` }));
for (const [name, bytes] of HAND) decode.push(await decodeCase(name, b64(bytes)));

// Every prefix of the two embedded fixtures: png-chunks-extract reads past
// the end as zeros, so where the cut falls decides the error. Runs of
// prefix lengths with the same result are stored once, as
// `{ from, to, result }` (both ends inclusive).
const prefixes = [];
for (const f of ["test_embedded_v1.png", "smiley_embedded_v2.png"]) {
  const file = `${FIXTURE_DIR}/${f}`;
  const n = readRepo(file).length;
  const runs = [];
  for (let len = 0; len <= n; len++) {
    const bytes = materialize({ prefix: { file, len } });
    const result = await outcome(() => decodePngMetadata(blobOf(bytes)), (s) => ({ ok: stringGolden(s) }));
    const last = runs.at(-1);
    if (last && JSON.stringify(last.result) === JSON.stringify(result)) last.to = len;
    else runs.push({ from: len, to: len, result });
  }
  prefixes.push({ file, len: n, runs });
}

// Random corruptions of the embedded fixtures.
for (const f of ["test_embedded_v1.png", "smiley_embedded_v2.png"]) {
  for (let seed = 1; seed <= 150; seed++) {
    const input = { flip: { file: `${FIXTURE_DIR}/${f}`, seed: seed * 7919, count: 1 + (seed % 3) } };
    decode.push(await decodeCase(`flip_${f}_${seed}`, input, false));
  }
}

// encodePngMetadata on PNGs and scene texts; what upstream then decodes from
// its own output.
const PNG_INPUTS = [
  ["smiley", { file: `${FIXTURE_DIR}/smiley.png` }],
  ["deer", { file: `${FIXTURE_DIR}/deer.png` }],
  ["already_embedded_v1", { file: `${FIXTURE_DIR}/test_embedded_v1.png` }],
  ["already_embedded_v2", { file: `${FIXTURE_DIR}/smiley_embedded_v2.png` }],
  ["chunks_after_iend", b64(png(IHDR, PLTE, IDAT, IEND, tEXtOf("Comment", "after"), IEND))],
  ["iend_with_data", b64(png(IHDR, PLTE, IDAT, rawChunk("IEND", latin1("xyz"))))],
  ["ihdr_iend_only", b64(png(IHDR, IEND))],
  ["invalid_signature", b64(concat(latin1("GIF89a"), smileyWith()))],
  ["no_iend", b64(png(IHDR, PLTE, IDAT))],
];
const smileyScene = await decodePngMetadata(blobOf(readRepo(`${FIXTURE_DIR}/smiley_embedded_v2.png`)));
const TEXT_INPUTS = [
  ["smiley_scene", { text: smileyScene }],
  ["every_type_scene", { file: "crates/excali-core/tests/fixtures/every-type.excalidraw" }],
  ["empty_scene", { file: "crates/excali-core/tests/fixtures/empty-scene.excalidraw" }],
  ["unicode", { text: "\u{1F600} \u00fcn\u00efc\u00f6d\u00e9 \u2014 \u4e2d\u6587 \u0000\u001f\u007f \"\\ </script>" }],
  ["empty", { text: "" }],
  ["long", { repeat: "excalidraw \u00e9l\u00e9ment \u{1F3A8} ", count: 5000 }],
];
const textOf = (spec) => {
  if ("text" in spec) return spec.text;
  if ("repeat" in spec) return spec.repeat.repeat(spec.count);
  return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(readRepo(spec.file));
};

const encodeCases = [];
for (const [pngName, pngInput] of PNG_INPUTS) {
  for (const [textName, textInput] of TEXT_INPUTS) {
    if (pngName !== "smiley" && !["smiley_scene", "empty"].includes(textName)) continue;
    const metadata = textOf(textInput);
    let written;
    const result = await outcome(
      async () => (written = await bytesOfBlob(await encodePngMetadata({ blob: blobOf(materialize(pngInput)), metadata }))),
      (b) => ({ ok: bytesGolden(b) }),
    );
    const c = { name: `${pngName}__${textName}`, png: pngInput, metadata: textInput, written: result };
    if (written) {
      c.decoded = await outcome(() => decodePngMetadata(blobOf(written)), (s) => ({ ok: stringGolden(s) }));
    }
    encodeCases.push(c);
  }
}

// The tEXt payload upstream writes for a text, on its own: the chunk data is
// `keyword NUL JSON.stringify(encode({text, compress: true}))` as Latin-1.
const textChunks = TEXT_INPUTS.map(([name, input]) => ({
  name,
  metadata: input,
  json: stringGolden(JSON.stringify(encode({ text: textOf(input), compress: true }))),
}));

const out = {
  generator: "scripts/fixtures/png-goldens.sh",
  upstream_commit: upstreamCommit,
  packages: Object.fromEntries([...packages.keys()].sort().map((n) => [n, versionOf(n)])),
  keyword: KEYWORD,
  decode,
  prefixes,
  encode: encodeCases,
  text_chunks: textChunks,
};

// Guard against the stubs drifting from what the goldens assume.
if (encodeChunks(extractChunks(readRepo(`${FIXTURE_DIR}/smiley.png`))).length !== readRepo(`${FIXTURE_DIR}/smiley.png`).length) {
  throw new Error("png-chunks-encode does not reproduce smiley.png");
}

const json = JSON.stringify(out, null, 1).replace(
  /[\u007f-\uffff]/g,
  (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
);
process.stdout.write(`${json}\n`);
