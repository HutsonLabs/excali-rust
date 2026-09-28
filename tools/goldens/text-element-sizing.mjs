#!/usr/bin/env node
// Text element sizing goldens for excali-text (ex-305): upstream's own
// newTextElement, refreshTextDimensions and getTextAnchorRatios
// (packages/element/src/newElement.ts:297-581), getBoundTextMaxWidth
// (packages/element/src/textElement.ts:540-569) and the autoResize action
// (packages/excalidraw/actions/actionTextAutoResize.ts) run from the pinned
// checkout under plain Node, measuring through upstream's
// setCustomTextMetricsProvider (packages/element/src/textMeasurements.ts).
//
//   node tools/goldens/text-element-sizing.mjs            write the fixture
//   node tools/goldens/text-element-sizing.mjs --check    exit 1 if it is stale
//   node tools/goldens/text-element-sizing.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-text/tests/fixtures/text-element-sizing.json:
//
// - metrics: the two line-width providers, restated in the Rust test:
//   `chars10` is upstream's test metric (text.length * 10); `scaled` gives
//   each UTF-16 code unit u the width 3 + (u * 7) % 11, takes 0.5 off for
//   every adjacent pair whose sum is a multiple of 5 and scales the sum by
//   the font size over 20, so the font size reaches the width;
// - templates: one text element and one container of each type as
//   upstream's constructors build them. A case lists the text's fields that
//   differ; the Rust test lays them over the template;
// - anchors: getTextAnchorRatios for every alignment;
// - boundTextMaxWidth: getBoundTextMaxWidth for each container type, width
//   and font size;
// - create: newTextElement for texts, fonts, alignments and positions;
// - refresh: refreshTextDimensions(text, container, elementsMap, nextText,
//   maxWidth) for texts built by newTextElement then edited as the editor
//   and restore edit them: every alignment, angles, fixed and growing
//   widths, the view's maximum width, every container type, a container id
//   naming no element and a deleted text. For a label bound to an arrow,
//   `labelOrigin` is getElementAbsoluteCoords' top-left for the label (the
//   arrow's label position, LinearElementEditor.getBoundTextElementPosition),
//   which the Rust test supplies through excali_text's ArrowLabelGeometry;
// - sessions: typing a text one keystroke at a time as App.updateElement
//   does (packages/excalidraw/components/App.tsx: originalText set to the
//   editor's value and refreshTextDimensions with the view's maximum width
//   assigned over the text), every intermediate element;
// - autoResize: actionTextAutoResize.perform on wrapped fixed-width texts.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { escapeInvisible } from "./text-wrapping.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures");
export const FILE = "text-element-sizing.json";

const ENTRY = `
export {
  newTextElement,
  refreshTextDimensions,
  getTextAnchorRatios,
  newElement,
  newArrowElement,
  newStickyNoteElement,
  getElementAbsoluteCoords,
  getBoundTextMaxWidth,
} from "@excalidraw/element";
export { setCustomTextMetricsProvider, charWidth } from "./packages/element/src/textMeasurements";
export { actionTextAutoResize } from "./packages/excalidraw/actions/actionTextAutoResize";
export { Scene } from "./packages/element/src/Scene";
`;

const varied = (text) => {
  let width = 0;
  for (let i = 0; i < text.length; i++) {
    const u = text.charCodeAt(i);
    width += 3 + ((u * 7) % 11);
    if (i > 0 && (text.charCodeAt(i - 1) + u) % 5 === 0) width -= 0.5;
  }
  return width;
};

export const METRICS = {
  chars10: (text) => text.length * 10,
  scaled: (text, font) => (varied(text) * parseFloat(font)) / 20,
};

const TEXT_ALIGNS = ["left", "center", "right"];
const VERTICAL_ALIGNS = ["top", "middle", "bottom"];

/** The fields of a text element a case states (the rest is the template's). */
const TEXT_KEYS = [
  "x",
  "y",
  "width",
  "height",
  "angle",
  "text",
  "originalText",
  "fontSize",
  "fontFamily",
  "lineHeight",
  "textAlign",
  "verticalAlign",
  "autoResize",
  "containerId",
  "isDeleted",
];

/** What newTextElement decides. */
const CREATE_KEYS = [...TEXT_KEYS.filter((k) => !["angle", "isDeleted"].includes(k)), "baseFontSize"];

const pick = (element, keys) => Object.fromEntries(keys.map((k) => [k, element[k]]));

/** Keys of a template that describe one element rather than its kind. */
const templateOf = (element) => {
  // JSON drops the keys upstream leaves undefined (customData)
  const out = JSON.parse(JSON.stringify(element));
  out.id = `${element.type}-template`;
  out.seed = 1;
  out.versionNonce = 0;
  out.updated = 1;
  return out;
};

const build = async (upstream) => {
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": JSON.stringify("test") },
  });
  let current = METRICS.chars10;
  const fonts = new Set();
  up.setCustomTextMetricsProvider({
    getLineWidth: (text, font) => {
      fonts.add(font);
      return current(text, font);
    },
  });
  // wrapping caches each character's width per font string: widths cached
  // under the other metric must not leak into this one
  const useMetric = (metric) => {
    current = METRICS[metric];
    for (const font of fonts) up.charWidth.clearCache(font);
  };

  // Containers, one per type getBoundTextMaxWidth distinguishes.
  const containers = {
    rectangle: up.newElement({ type: "rectangle", x: 40, y: 30, width: 180, height: 120 }),
    ellipse: up.newElement({ type: "ellipse", x: -60, y: 10, width: 230, height: 140 }),
    diamond: up.newElement({ type: "diamond", x: 5, y: -45, width: 260, height: 160 }),
    stickynote: up.newStickyNoteElement({ type: "stickynote", x: 100, y: 100, width: 250, height: 250 }),
    arrow: up.newArrowElement({
      type: "arrow",
      x: 20,
      y: 60,
      width: 300,
      height: 40,
      points: [
        [0, 0],
        [300, 40],
      ],
    }),
  };
  for (const [name, c] of Object.entries(containers)) {
    containers[name] = { ...c, id: name };
  }

  const template = templateOf(up.newTextElement({ text: "template", x: 0, y: 0 }));
  /** A text's case fields: its geometry and text, and what else differs from the template. */
  const textFields = (element) =>
    Object.fromEntries(
      TEXT_KEYS.filter(
        (k) => ["x", "y", "width", "height", "text"].includes(k) || element[k] !== template[k],
      ).map((k) => [k, element[k]]),
    );

  const anchors = [];
  for (const textAlign of TEXT_ALIGNS) {
    for (const verticalAlign of VERTICAL_ALIGNS) {
      anchors.push({ textAlign, verticalAlign, ratios: up.getTextAnchorRatios({ textAlign, verticalAlign }) });
    }
  }

  const boundTextMaxWidth = [];
  for (const type of ["rectangle", "ellipse", "diamond", "stickynote", "arrow", "embeddable"]) {
    for (const width of [0, 7, 33.3, 100, 180, 257.5, 1000]) {
      for (const fontSize of [null, 8, 20, 36]) {
        const container = { ...containers[type === "embeddable" ? "rectangle" : type], type, width };
        const text = fontSize === null ? null : { ...template, fontSize };
        boundTextMaxWidth.push({ type, width, fontSize, maxWidth: up.getBoundTextMaxWidth(container, text) });
      }
    }
  }

  // newTextElement
  const CREATE_TEXTS = ["", "hello\nworld!", "a\r\nb\tc\rd", "\n\nx\n", "😀 wide 漢字  "];
  const CREATE_FONTS = [
    { fontSize: 36, fontFamily: 3 },
    { fontSize: 16, fontFamily: 1, lineHeight: 1.5 },
    { fontSize: 0, fontFamily: 0, lineHeight: 0 },
  ];
  const create = [];
  for (const metric of Object.keys(METRICS)) {
    useMetric(metric);
    for (const text of CREATE_TEXTS) {
      for (const font of CREATE_FONTS) {
        for (const textAlign of [undefined, ...TEXT_ALIGNS]) {
          for (const verticalAlign of [undefined, ...VERTICAL_ALIGNS]) {
            const opts = { text, x: 10.5, y: -20.25, ...font };
            if (textAlign) opts.textAlign = textAlign;
            if (verticalAlign) opts.verticalAlign = verticalAlign;
            const el = up.newTextElement(opts);
            create.push({
              metric,
              opts,
              element: pick(el, CREATE_KEYS),
            });
          }
        }
      }
    }
    // originalText, autoResize and containerId pass through
    for (const extra of [
      { originalText: "raw source", autoResize: false },
      { originalText: "", containerId: "rectangle", baseFontSize: 28 },
    ]) {
      const opts = { text: "shown\ntext", x: 3, y: 4, textAlign: "center", verticalAlign: "bottom", ...extra };
      const el = up.newTextElement(opts);
      create.push({ metric, opts, element: pick(el, CREATE_KEYS) });
    }
  }

  // refreshTextDimensions
  const EDITS = [
    ["hello world, this is long", "hi"],
    ["a\nb", "a\nb\nccc dd eee"],
    ["", "x"],
    ["one two three four five six", "one two three four five six seven"],
  ];
  const ANGLES = [0, 0.7, 4.5];
  const MODES = [
    "auto",
    "fixed",
    "autoMax",
    "autoMaxWider",
    "fixedMax",
    "rectangle",
    "ellipse",
    "diamond",
    "stickynote",
    "arrow",
    "dangling",
    "deleted",
    "fixedNoText",
  ];
  const refresh = [];
  for (const metric of Object.keys(METRICS)) {
    useMetric(metric);
    for (const mode of MODES) {
      // a deleted text is left alone whatever it holds
      const once = mode === "deleted";
      const edits = once ? EDITS.slice(0, 1) : EDITS;
      const aligns = once ? [["left", "top"]] : TEXT_ALIGNS.flatMap((t) => VERTICAL_ALIGNS.map((v) => [t, v]));
      // an arrow's label is never rotated
      const angles =
        once || mode === "arrow"
          ? [0]
          : ["rectangle", "ellipse", "diamond", "stickynote"].includes(mode)
            ? [0, 0.7]
            : ANGLES;
      for (const [prev, next] of edits) {
        for (const angle of angles) {
          for (const [textAlign, verticalAlign] of aligns) {
            {
              const fontSize = mode === "autoMax" || mode === "arrow" ? 16 : 20;
              let el = up.newTextElement({ text: prev, x: 30.5, y: 70.25, textAlign, verticalAlign, fontSize });
              el = { ...el, angle };
              let container = null;
              let maxWidth;
              let nextText = next;
              const elements = [];
              if (mode === "fixed" || mode === "fixedMax" || mode === "fixedNoText") {
                el = { ...el, autoResize: false, width: 60 };
                if (mode === "fixedMax") maxWidth = 40;
                if (mode === "fixedNoText") nextText = undefined;
              } else if (mode === "autoMax") {
                maxWidth = 90;
              } else if (mode === "autoMaxWider") {
                maxWidth = 40;
              } else if (mode === "dangling") {
                el = { ...el, containerId: "missing" };
              } else if (mode === "deleted") {
                el = { ...el, isDeleted: true };
              } else if (containers[mode]) {
                container = { ...containers[mode], boundElements: [{ type: "text", id: el.id }] };
                el = { ...el, containerId: container.id };
                elements.push(container);
              }
              elements.push(el);
              const map = new Map(elements.map((e) => [e.id, e]));
              const result = up.refreshTextDimensions(el, container, map, nextText, maxWidth);
              const entry = { metric, mode, text: textFields(el) };
              if (container) entry.container = container.id;
              if (mode === "arrow") {
                const [x1, y1] = up.getElementAbsoluteCoords(el, map);
                entry.labelOrigin = [x1, y1];
              }
              if (nextText !== undefined) entry.nextText = nextText;
              if (maxWidth !== undefined) entry.maxWidth = maxWidth;
              entry.result = result === undefined ? null : result;
              refresh.push(entry);
            }
          }
        }
      }
    }
  }

  // Typing sessions: App.updateElement on every keystroke.
  const SESSION_TEXT = "Excalidraw is a whiteboard\nfor sketching diagrams!";
  const sessions = [];
  for (const metric of Object.keys(METRICS)) {
    useMetric(metric);
    for (const maxWidth of [undefined, 150]) {
      for (const angle of maxWidth === undefined ? [0] : [0, 1.1]) {
        for (const textAlign of TEXT_ALIGNS) {
          for (const verticalAlign of VERTICAL_ALIGNS) {
            let el = up.newTextElement({ text: "", x: 200, y: 100, textAlign, verticalAlign });
            el = { ...el, angle };
            const start = textFields(el);
            const steps = [];
            for (let i = 1; i <= SESSION_TEXT.length; i++) {
              const typed = SESSION_TEXT.slice(0, i);
              const map = new Map([[el.id, el]]);
              el = {
                ...el,
                originalText: typed,
                ...up.refreshTextDimensions(el, null, map, typed, maxWidth),
              };
              steps.push(pick(el, ["text", "autoResize", "x", "y", "width", "height"]));
            }
            const session = { metric, text: start, typed: SESSION_TEXT };
            if (maxWidth !== undefined) session.maxWidth = maxWidth;
            session.steps = steps;
            sessions.push(session);
          }
        }
      }
    }
  }

  // actionTextAutoResize
  const autoResize = [];
  const scene = new up.Scene();
  for (const metric of Object.keys(METRICS)) {
    useMetric(metric);
    for (const [shown, original] of [
      ["this is it my friends\nald aksdl askdlasdk", "this is it my friends ald aksdl askdlasdk"],
      ["a\nb", "a\nb"],
      ["wrapped\nline\nhere", "wrapped line here and more"],
    ]) {
      for (const textAlign of TEXT_ALIGNS) {
        for (const verticalAlign of VERTICAL_ALIGNS) {
          for (const fontSize of [20, 28]) {
            const el = {
              ...up.newTextElement({ text: shown, x: 100, y: 100, textAlign, verticalAlign, fontSize }),
              width: 300,
              height: 50,
              originalText: original,
              autoResize: false,
            };
            const appState = { selectedElementIds: { [el.id]: true } };
            const out = up.actionTextAutoResize.perform([el], appState, el, { scene });
            autoResize.push({
              metric,
              text: textFields(el),
              result: pick(out.elements[0], ["autoResize", "width", "height", "x", "y", "text"]),
            });
          }
        }
      }
    }
  }

  return escapeInvisible(
    format({
      description:
        "Upstream newTextElement, refreshTextDimensions and getTextAnchorRatios (element/src/newElement.ts:297-581), getBoundTextMaxWidth (element/src/textElement.ts:540-569) and actionTextAutoResize (excalidraw/actions/actionTextAutoResize.ts) measured through setCustomTextMetricsProvider at the pinned commit (tools/goldens/text-element-sizing.mjs).",
      upstream: upstream.commit,
      metrics: {
        chars10: "text.length * 10 (UTF-16 code units)",
        scaled:
          "(sum over UTF-16 code units u of 3 + (u * 7) % 11, minus 0.5 for each adjacent pair (a, b) with (a + b) % 5 == 0) * parseFloat(font) / 20",
      },
      templates: { text: template, ...Object.fromEntries(Object.entries(containers).map(([k, c]) => [k, templateOf(c)])) },
      anchors,
      boundTextMaxWidth,
      create,
      refresh,
      sessions,
      autoResize,
    }),
  );
};

const usage = () => {
  process.stderr.write("usage: text-element-sizing.mjs [--check] [--out DIR]\n");
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
    process.stderr.write(`text-element-sizing: ${error.message}\n`);
    process.exit(1);
  }
  const text = await build(upstream);
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(
        `stale: ${where}\ntext-element-sizing goldens are out of date: run node tools/goldens/text-element-sizing.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`text-element-sizing goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
