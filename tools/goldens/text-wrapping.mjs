#!/usr/bin/env node
// Text wrapping goldens for excali-text (ex-303): upstream's own
// parseTokens and getWrappedTextLines (packages/element/src/textWrapping.ts)
// run from the pinned checkout under plain Node, measuring through upstream's
// setCustomTextMetricsProvider and charWidth cache
// (packages/element/src/textMeasurements.ts:106-119, 152-158, 179-210).
//
//   node tools/goldens/text-wrapping.mjs            write the fixture
//   node tools/goldens/text-wrapping.mjs --check    exit 1 if it is stale
//   node tools/goldens/text-wrapping.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-text/tests/fixtures/text-wrapping.json:
//
// - metrics: the two line-width providers, restated in the Rust test:
//   `chars10` is upstream's test metric (jest-canvas-mock's measureText
//   width is text.length, times 10 under isTestEnv); `varied` gives each
//   UTF-16 code unit u the width 3 + (u * 7) % 11 and takes 0.5 off for
//   every adjacent pair of code units whose sum is a multiple of 5, so a
//   line is not the sum of its characters (as with kerning) and the cached
//   single-character widths and whole-line measurements differ;
// - tokens: parseTokens of every hard line of every text;
// - wraps: for every text, metric and width, getWrappedTextLines as
//   [text, start, end] triples (wrapText is their texts joined by "\n").
//   The charWidth cache is cleared before each call. A width that JSON
//   cannot hold is written as the string "NaN", "Infinity" or "-Infinity".
//
// The texts are every input of upstream's textWrapping.test.ts, hand-picked
// edge cases (JS whitespace, line terminators inside a line, emoji
// sequences, NFD input, brackets and CJK punctuation), and seeded random
// strings over an alphabet of the characters the break rules single out.
//
// Only printable ASCII and visible characters (letters, numbers, punctuation,
// symbols) are written as themselves; every other code unit (controls,
// spaces other than U+0020, marks, format characters, joiners, variation
// selectors, tags) is a \uXXXX escape, so the file passes the repository's
// invisible-character gate and parses back to the same JS strings.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures");
export const FILE = "text-wrapping.json";

const ENTRY = `
export { parseTokens, getWrappedTextLines } from "./packages/element/src/textWrapping";
export { setCustomTextMetricsProvider, charWidth } from "./packages/element/src/textMeasurements";
`;

// textWrapping.test.ts:12
export const FONT = "10px Cascadia, Segoe UI Emoji";

export const METRICS = {
  chars10: (text) => text.length * 10,
  varied: (text) => {
    let width = 0;
    for (let i = 0; i < text.length; i++) {
      const u = text.charCodeAt(i);
      width += 3 + ((u * 7) % 11);
      if (i > 0 && (text.charCodeAt(i - 1) + u) % 5 === 0) width -= 0.5;
    }
    return width;
  },
};

/** Every input of packages/element/tests/textWrapping.test.ts. */
const UPSTREAM_TEXTS = [
  "Hello Excalidraw",
  "Hello😀",
  "don't wrap this number 99,100.99",
  "Hello     ",
  "  Hello  World",
  "   Hello  World            ",
  "Hello   Wo rl  d                     ",
  "😀🗺🔥👩🏽\u200d🦰👨\u200d👩\u200d👧\u200d👦🇨🇿",
  "Wikipedia is hosted by Wikimedia- Foundation, a non-profit organization that also hosts a range-of other projects",
  "Hello thereusing-now",
  "\tA) one tab\t\t- two tabs        - 8 spaces",
  "Hello World!",
  "Excalidraw",
  "A\n\nB",
  "안녕하세요こんにちは世界ｺﾝﾆﾁハ你好",
  "a醫 醫      bb  你好  world-i-😀🗺🔥",
  "HelloたWorld",
  "こんにちは〃世界",
  "Hello た。",
  "Hello「たWorld」",
  "「Helloた」World",
  "中国你好！这是一个测试。\n我们来看看：人民币¥1234「很贵」\n（括号）、逗号，句号。空格 换行\u3000全角符号…—",
  "日本こんにちは！これはテストです。\n  見てみましょう：円￥1234「高い」\n  （括弧）、読点、句点。\n  空白 改行\u3000全角記号…ー",
  "한국 안녕하세요! 이것은 테스트입니다.\n우리 보자: 원화₩1234「비싸다」\n(괄호), 쉼표, 마침표.\n공백 줄바꿈\u3000전각기호…—",
  "  \t   Hello world",
  "Hello whats up     ",
  "Hippopotomonstrosesquippedaliophobia        ??????",
  "Hello whats up",
  "Hello\n  whats up",
  "hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg break it now",
  "Excalidraw is a virtual collaborative whiteboard",
  "99,100.99",
  "😬🌍🗺🔥☂\ufe0f👩🏽\u200d🦰👨\u200d👩\u200d👧\u200d👦👩🏾\u200d🔬🏳\ufe0f\u200d🌈🧔\u200d♀\ufe0f🧑\u200d🤝\u200d🧑🙅🏽\u200d♂\ufe0f✅0\ufe0f\u20e3🇨🇿🦅",
  "😬a🌍b🗺c🔥d☂\ufe0f《👩🏽\u200d🦰》👨\u200d👩\u200d👧\u200d👦德👩🏾\u200d🔬こ🏳\ufe0f\u200d🌈안🧔\u200d♀\ufe0fg🧑\u200d🤝\u200d🧑h🙅🏽\u200d♂\ufe0fe✅f0\ufe0f\u20e3g🇨🇿10🦅#hash",
  "c\u030cて\u3099a\u0308ひ\u309aε\u0301다и\u0306한",
  "《道德經》醫-醫こんにちは世界！안녕하세요세계；요』,다.다...원/달(((다)))[[1]]〚({((한))>)〛(「た」)た…[Hello] \t\u3000World？ニューヨーク・￥3700.55す。090-1234-5678￥1,000〜＄5,000「素晴らしい！」〔重要〕＃１：Taro君30％は、（たなばた）〰￥110±￥570で20℃〜9:30〜10:00【一番】",
];

/** Cases upstream's code decides that its tests do not pin. */
const EDGE_TEXTS = [
  "",
  "\n",
  "\n\n",
  " ",
  "     ",
  "\t\t\t",
  "ab     ",
  "a\u00a0b\u00a0c d",
  "x\ufeffy z\ufeff",
  "x\u0085y z\u0085\u0085",
  "one\rtwo three\r\r",
  "one\u2028two\u2029three  \u2028 ",
  "trail \r  ",
  "a\u3000b\u3000\u3000c",
  "a--b---c -d- e-",
  "-leading and trailing-",
  "(a)(b)[c]{d}<e>",
  "foo.(bar)baz!?[qux]",
  "Hello(한글)world",
  "価格￥100円、＄5と￦7",
  "「」『』【】〔〕《》",
  "た。」』た、、た",
  "（（た））",
  "🇨🇿🇩🇪🇫",
  "a🇨b",
  "👍🏽👍🏻🏻",
  "\u200d\u200d😀\u200d",
  "😀\ufe0f\ufe0f",
  "1\ufe0f\u20e3#\ufe0f\u20e3*\ufe0f\u20e3",
  "🏴\udb40\udc67\udb40\udc62\udb40\udc73\udb40\udc63\udb40\udc74\udb40\udc7f flag",
  "🏴\udb40\udc67\udb40\udc62",
  "e\u0301\u0302 o\u0308\u0304 A\u030a",
  "각가",
  "مرحبا بالعالم",
  "สว\u0e31สด\u0e35ชาวโลก",
  "नमस\u094dत\u0947 द\u0941न\u093fय\u093e",
  "𠀀𠀁𠀂𠀃 𠀄𠀅",
  "𝒜𝒷𝒸𝒹𝑒𝒻𝑔𝒽",
  "👨\u200d👩\u200d👧\u200d👦👨\u200d👩\u200d👧\u200d👦👨\u200d👩\u200d👧\u200d👦",
  "ｺﾝﾆﾁﾊ ﾜｰﾙﾄﾞ",
  "ー〜ー〜",
  "〃〃〃abc",
  "abc＃＆＊def",
  "Word…/next",
  "x>)]}.,:;!?…/(y",
  "Supercalifragilisticexpialidocious",
  "Hi \n  there   \n\n   friend  ",
];

// mulberry32 (upstream's own PRNG in common/src/random.ts, seeded here).
const mulberry32 = (seed) => () => {
  let t = (seed += 0x6d2b79f5);
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

/** Atoms the random strings are drawn from: every class the rules name. */
const ALPHABET = [
  "a", "b", "c", "x", "y", "Z", "0", "7", "#", "*",
  " ", " ", " ", "\t", "\u00a0", "\u3000", "\ufeff", "\u0085", "\r", "\u2028", "\n",
  "-", "<", "(", "[", "{", ">", ")", "]", "}", ".", ",", ":", ";", "!", "?", "…", "/",
  "漢", "字", "か", "な", "カ", "ナ", "ｶ", "한", "글", "ー", "〃", "〰", "＃", "／",
  "（", "「", "『", "【", "《", "〝", "）", "」", "』", "】", "》", "。", "、", "・", "〜",
  "￥", "＄", "₩",
  "😀", "🗺", "✅", "☂", "\ufe0f", "\u200d", "🏽", "\u20e3", "🇨", "🇿",
  "👩🏽\u200d🦰", "🏳\ufe0f\u200d🌈",
  "e\u0301", "가", "𠀀", "𠀁", "𝒜",
];

const randomTexts = () => {
  const random = mulberry32(303);
  const texts = [];
  for (let i = 0; i < 60; i++) {
    const length = 1 + Math.floor(random() * 32);
    let text = "";
    for (let j = 0; j < length; j++) {
      text += ALPHABET[Math.floor(random() * ALPHABET.length)];
    }
    texts.push(text);
  }
  return texts;
};

export const WIDTHS = [0, 5, 10, 15, 20, 30, 40, 50, 60, 80, 100, 120, 150, 300];
const INVALID_WIDTHS = [-1, NaN, Infinity, -Infinity];

const widthJson = (w) => (Number.isFinite(w) ? w : String(w));

const build = async (upstream) => {
  // MODE "test", as under upstream's vitest: satisfiesWordInvariant
  // (textWrapping.ts:731-740) then throws if a word handed to wrapWord
  // contains whitespace, so no golden comes from a broken invariant.
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": JSON.stringify("test") },
  });
  let current = METRICS.chars10;
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => current(text) });

  const texts = [...new Set([...UPSTREAM_TEXTS, ...EDGE_TEXTS, ...randomTexts()])];

  const hardLines = [...new Set(texts.flatMap((t) => t.split("\n")))];
  const tokens = hardLines.map((line) => ({ line, tokens: up.parseTokens(line) }));

  const wraps = [];
  for (const text of texts) {
    const widths = UPSTREAM_TEXTS.includes(text) || EDGE_TEXTS.includes(text)
      ? [...WIDTHS, ...INVALID_WIDTHS]
      : WIDTHS;
    for (const metric of Object.keys(METRICS)) {
      current = METRICS[metric];
      const cases = widths.map((maxWidth) => {
        up.charWidth.clearCache(FONT);
        const lines = up.getWrappedTextLines(text, FONT, maxWidth);
        return { maxWidth: widthJson(maxWidth), lines: lines.map((l) => [l.text, l.start, l.end]) };
      });
      wraps.push({ metric, text, cases });
    }
  }

  return escapeInvisible(
    format({
      description:
        "Upstream parseTokens and getWrappedTextLines (element/src/textWrapping.ts) measured through setCustomTextMetricsProvider and the charWidth cache (element/src/textMeasurements.ts:106-119, 152-158, 179-210) at the pinned commit, for every input of element/tests/textWrapping.test.ts, edge cases and seeded random strings (tools/goldens/text-wrapping.mjs).",
      upstream: upstream.commit,
      font: FONT,
      metrics: {
        chars10: "text.length * 10 (UTF-16 code units)",
        varied:
          "sum over UTF-16 code units u of 3 + (u * 7) % 11, minus 0.5 for each adjacent pair (a, b) with (a + b) % 5 == 0",
      },
      tokens,
      wraps,
    }),
  );
};

const VISIBLE = /^[\p{L}\p{N}\p{P}\p{S}]$/u;
// The authorship gate's invisible ranges that are letters (Hangul fillers).
const GATED = /^[\u115f\u1160\u3164\uffa0]$/u;

/** Writes every non-visible code point of the JSON as \uXXXX escapes. */
export const escapeInvisible = (json) => {
  let out = "";
  for (const ch of json) {
    const cp = ch.codePointAt(0);
    const plain = (cp >= 0x20 && cp < 0x7f) || cp === 0x0a || (cp >= 0x80 && VISIBLE.test(ch) && !GATED.test(ch));
    if (plain) {
      out += ch;
    } else {
      for (let i = 0; i < ch.length; i++) {
        out += `\\u${ch.charCodeAt(i).toString(16).padStart(4, "0")}`;
      }
    }
  }
  return out;
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating text-wrapping goldens");
  };
  try {
    return await fn();
  } finally {
    Math.random = random;
  }
};

const usage = () => {
  process.stderr.write("usage: text-wrapping.mjs [--check] [--out DIR]\n");
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

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`text-wrapping: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(
        `stale: ${where}\ntext-wrapping goldens are out of date: run node tools/goldens/text-wrapping.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`text-wrapping goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
