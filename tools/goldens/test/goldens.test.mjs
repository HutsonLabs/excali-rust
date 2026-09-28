// Content checks on the committed goldens: provenance, coverage of what the
// downstream tasks consume (ex-203/204/205/208-214), and the Excalidraw
// option mapping (research/rendering.md section 1) restated independently.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import {
  ALL_FILES,
  ARROWHEADS,
  ELEMENT_FILES,
  expectedRoughOptions,
  golden,
  GOLDENS_DIR,
  numbers,
  parkMiller,
  pinnedCommit,
  ROUGH_DEFAULTS,
  ROUGH_FILES,
  strokeLineDash,
  svgPathFromStroke,
} from "./helpers.mjs";

const SEEDS = [1, 7, 1041657908];
const ROUGHNESSES = [0, 1, 2];

test("manifest records the pin, the package versions and every file", () => {
  const manifest = golden("manifest.json");
  assert.equal(manifest.generator, "tools/goldens/generate.mjs");
  assert.equal(manifest.upstream.commit, pinnedCommit());
  assert.equal(manifest.upstream.repo, "https://github.com/excalidraw/excalidraw");
  assert.deepEqual(manifest.packages, {
    "perfect-freehand": "1.2.0",
    "points-on-curve": "1.0.1",
    roughjs: "4.6.4",
    tinycolor2: "1.6.0",
  });
  assert.deepEqual(
    manifest.files.map((f) => f.name),
    ALL_FILES,
  );
  const onDisk = readdirSync(GOLDENS_DIR)
    .filter((f) => f.endsWith(".json") && f !== "manifest.json")
    .sort();
  assert.deepEqual(onDisk, [...ALL_FILES].sort(), "no stray or missing golden files");
  for (const f of manifest.files) {
    const bytes = readFileSync(join(GOLDENS_DIR, f.name));
    assert.equal(f.sha256, createHash("sha256").update(bytes).digest("hex"), f.name);
    assert.equal(f.cases, golden(f.name).cases.length, f.name);
  }
});

test("every golden is finite JSON with unique case ids", () => {
  for (const name of ALL_FILES) {
    const g = golden(name);
    assert.ok(typeof g.description === "string" && g.description.length > 0, name);
    assert.ok(g.cases.length > 0, name);
    const ids = g.cases.map((c) => c.id);
    assert.equal(new Set(ids).size, ids.length, `${name}: duplicate ids`);
    for (const n of numbers(g)) {
      assert.ok(Number.isFinite(n), `${name}: non-finite number`);
    }
  }
});

test("random.json is roughjs Random.next (Park-Miller, 48271)", () => {
  const g = golden("random.json");
  const seeds = g.cases.map((c) => c.seed);
  for (const s of [...SEEDS, 2147483646]) assert.ok(seeds.includes(s), `seed ${s}`);
  for (const c of g.cases) {
    assert.ok(c.values.length >= 64);
    assert.deepEqual(c.values, parkMiller(c.seed, c.values.length), `seed ${c.seed}`);
  }
});

const assertDrawable = (d, where) => {
  assert.ok(typeof d.shape === "string", `${where}: shape`);
  assert.ok(Array.isArray(d.sets) && d.sets.length > 0, `${where}: sets`);
  assert.ok(!("randomizer" in d.options), `${where}: RNG state must not leak`);
  for (const set of d.sets) {
    assert.ok(["path", "fillPath", "fillSketch"].includes(set.type), `${where}: ${set.type}`);
    for (const op of set.ops) {
      assert.ok(["move", "lineTo", "bcurveTo"].includes(op.op), `${where}: ${op.op}`);
      assert.equal(op.data.length, op.op === "bcurveTo" ? 6 : 2, `${where}: ${op.op} arity`);
    }
  }
};

test("rough primitives cover every method x seed x roughness (ex-203)", () => {
  const g = golden("rough-primitives.json");
  for (const method of ["line", "rectangle", "polygon", "ellipse", "curve", "path", "linearPath", "circle", "arc"]) {
    for (const seed of SEEDS) {
      for (const roughness of ROUGHNESSES) {
        const hit = g.cases.filter(
          (c) => c.method === method && c.options.seed === seed && c.options.roughness === roughness,
        );
        assert.ok(hit.length > 0, `${method} seed ${seed} roughness ${roughness}`);
      }
    }
  }
  for (const c of g.cases) {
    assertDrawable(c.drawable, c.id);
    // resolved options = roughjs defaults overlaid with the case's options
    const resolved = { ...ROUGH_DEFAULTS, ...c.options };
    for (const [k, v] of Object.entries(resolved)) {
      assert.deepEqual(c.drawable.options[k], v, `${c.id}: option ${k}`);
    }
  }
});

test("rough fills cover every fill style at Excalidraw's weights (ex-204)", () => {
  const g = golden("rough-fills.json");
  const styles = ["hachure", "cross-hatch", "zigzag", "solid", "dots", "dashed", "zigzag-line"];
  for (const style of styles) {
    for (const sw of [1, 2, 4]) {
      const hit = g.cases.filter(
        (c) =>
          c.options.fillStyle === style &&
          c.options.fillWeight === sw / 2 &&
          c.options.hachureGap === sw * 4,
      );
      const methods = new Set(hit.map((c) => c.method));
      for (const m of ["rectangle", "polygon", "ellipse", "path"]) {
        assert.ok(methods.has(m), `${style} sw ${sw} on ${m}`);
      }
    }
  }
  for (const c of g.cases) {
    assertDrawable(c.drawable, c.id);
    const types = c.drawable.sets.map((s) => s.type);
    assert.ok(
      types.includes(c.options.fillStyle === "solid" ? "fillPath" : "fillSketch"),
      `${c.id}: fill set present`,
    );
  }
});

test("rough options goldens vary each option Excalidraw relies on (ex-205)", () => {
  const g = golden("rough-options.json");
  const varied = (key, values) => {
    for (const v of values) {
      assert.ok(
        g.cases.some((c) => JSON.stringify(c.options[key]) === JSON.stringify(v)),
        `${key} = ${JSON.stringify(v)}`,
      );
    }
  };
  varied("disableMultiStroke", [true, false]);
  varied("disableMultiStrokeFill", [true, false]);
  varied("preserveVertices", [true, false]);
  varied("curveFitting", [0.95, 1]);
  varied("strokeWidth", [1.5, 2.5, 4.5]);
  varied("bowing", [0, 1, 5]);
  varied("curveTightness", [0, 0.5]);
  varied("curveStepCount", [9, 20]);
  varied("maxRandomnessOffset", [2, 6]);
  varied("hachureAngle", [-41, 0, 60]);
  varied("fillShapeRoughnessGain", [0.8, 0]);
  varied("strokeLineDash", [[8, 9], [1.5, 7]]);
  for (const c of g.cases) assertDrawable(c.drawable, c.id);
});

/** Which shapes upstream builds with continuousPath = true. */
const continuousPath = (el) =>
  (["rectangle", "iframe", "embeddable", "diamond"].includes(el.type) && !!el.roundness) ||
  (el.type === "arrow" && el.elbowed === true);

test("element goldens follow generateRoughOptions (shape.ts:195-260)", () => {
  let checked = 0;
  for (const name of ELEMENT_FILES) {
    for (const c of golden(name).cases) {
      const el = c.element;
      assert.ok(el && typeof el.type === "string", `${name}/${c.id}: element`);
      assert.ok(c.renderConfig, `${name}/${c.id}: renderConfig`);
      if (!c.shapes.length || c.shapes[0].type !== "rough") continue;
      c.shapes.forEach((s, i) => s.type === "rough" && assertDrawable(s.drawable, `${c.id}[${i}]`));
      // The body shape is shapes[0]. Iframe-like elements rewritten by
      // modifyIframeLikeForRoughOptions are checked in their own test.
      if (["iframe", "embeddable"].includes(el.type)) continue;
      const opts = c.shapes[0].drawable.options;
      const expected = expectedRoughOptions(el, continuousPath(el));
      if (el.type === "freedraw") {
        // the freedraw fill is generator.curve(..., { ...options, stroke: "none" })
        assert.equal(opts.stroke, "none", `${c.id}: freedraw fill stroke`);
      }
      for (const [k, v] of Object.entries(expected)) {
        assert.deepEqual(opts[k], v, `${name}/${c.id}: ${k}`);
      }
      assert.deepEqual(opts.strokeLineDash, strokeLineDash(el), `${c.id}: strokeLineDash`);
      const fillable = ["rectangle", "diamond", "ellipse"].includes(el.type);
      if (fillable && el.backgroundColor === "transparent") {
        assert.equal(opts.fill, undefined, `${c.id}: transparent means no fill`);
      }
      if (fillable && el.backgroundColor !== "transparent") {
        assert.equal(opts.fillStyle, el.fillStyle, `${c.id}: fillStyle`);
        assert.equal(typeof opts.fill, "string", `${c.id}: fill`);
      }
      if (el.type === "arrow") assert.equal(opts.fill, undefined, `${c.id}: arrows never fill`);
      checked++;
    }
  }
  assert.ok(checked > 150, `checked ${checked} element cases`);
});

const cases = (name) => golden(name).cases;

test("rectangle/diamond/ellipse goldens span seeds, roughness, fills, strokes, roundness", () => {
  for (const name of ["elements-rectangle.json", "elements-diamond.json", "elements-ellipse.json"]) {
    const cs = cases(name);
    for (const seed of SEEDS) {
      for (const r of ROUGHNESSES) {
        assert.ok(cs.some((c) => c.element.seed === seed && c.element.roughness === r), `${name} seed ${seed} r ${r}`);
      }
    }
    for (const fill of ["hachure", "cross-hatch", "zigzag", "solid"]) {
      for (const sw of [1, 2, 4]) {
        assert.ok(
          cs.some((c) => c.element.fillStyle === fill && c.element.strokeWidth === sw && c.element.backgroundColor !== "transparent"),
          `${name} ${fill} sw ${sw}`,
        );
      }
    }
    for (const style of ["dashed", "dotted"]) {
      assert.ok(cs.some((c) => c.element.strokeStyle === style), `${name} ${style}`);
    }
    assert.ok(cs.some((c) => c.renderConfig.theme === "dark"), `${name} dark theme`);
    // adjustRoughness: both reduction branches
    assert.ok(cs.some((c) => Math.max(c.element.width, c.element.height) < 10), `${name} tiny`);
    assert.ok(
      cs.some((c) => { const m = Math.max(c.element.width, c.element.height); return m >= 10 && m < 50; }),
      `${name} small`,
    );
  }
  for (const name of ["elements-rectangle.json", "elements-diamond.json"]) {
    const cs = cases(name);
    for (const type of [1, 2, 3]) {
      assert.ok(cs.some((c) => c.element.roundness?.type === type), `${name} roundness ${type}`);
    }
    assert.ok(cs.some((c) => c.element.roundness?.type === 3 && Math.min(c.element.width, c.element.height) > 128), `${name} adaptive fixed radius`);
    assert.ok(cs.some((c) => c.element.roundness?.type === 3 && c.element.roundness.value !== undefined), `${name} adaptive custom value`);
  }
});

test("line and arrow goldens cover linearPath, polygon and curve variants (ex-209)", () => {
  const lines = cases("elements-line.json");
  const shapeOf = (c) => c.shapes[0].drawable.shape;
  assert.ok(lines.some((c) => shapeOf(c) === "linearPath"));
  assert.ok(lines.some((c) => shapeOf(c) === "polygon"), "closed filled sharp line");
  assert.ok(lines.some((c) => shapeOf(c) === "curve"));
  assert.ok(lines.some((c) => shapeOf(c) === "curve" && c.shapes[0].drawable.options.fill), "closed filled curve");
  assert.ok(lines.some((c) => c.element.polygon === true));
  const arrows = cases("elements-arrow.json");
  assert.ok(arrows.some((c) => shapeOf(c) === "linearPath"));
  assert.ok(arrows.some((c) => shapeOf(c) === "curve"));
  for (const c of arrows) assert.notEqual(shapeOf(c), "polygon", `${c.id}: arrows never fill`);
});

test("arrowhead goldens: all fourteen heads at stroke widths 1, 2, 4 (ex-212)", () => {
  const cs = cases("elements-arrowheads.json");
  for (const head of ARROWHEADS) {
    for (const sw of [1, 2, 4]) {
      for (const pos of ["startArrowhead", "endArrowhead"]) {
        assert.ok(
          cs.some((c) => c.element[pos] === head && c.element.strokeWidth === sw),
          `${head} ${pos} sw ${sw}`,
        );
      }
    }
    assert.ok(cs.some((c) => c.element.endArrowhead === head && c.element.roundness), `${head} on a curve`);
    assert.ok(cs.some((c) => c.element.endArrowhead === head && c.element.strokeStyle === "dotted"), `${head} dotted`);
  }
  for (const c of cs) {
    assert.ok(c.shapes.length >= 2, `${c.id}: body plus head shapes`);
  }
  // outline heads fill with the canvas background (shape.ts:385-389, 401)
  const outline = cs.find(
    (c) => c.element.endArrowhead === "circle_outline" && c.element.startArrowhead === null && c.renderConfig.theme === "light",
  );
  const head = outline.shapes[outline.shapes.length - 1].drawable.options;
  assert.equal(head.fill, outline.renderConfig.canvasBackgroundColor);
});

test("elbow arrow goldens are continuous paths (ex-210)", () => {
  const cs = cases("elements-elbow-arrow.json");
  assert.ok(cs.length >= 4);
  for (const c of cs) {
    assert.equal(c.element.elbowed, true);
    const body = c.shapes[0].drawable;
    assert.equal(body.shape, "path");
    assert.equal(body.options.preserveVertices, true);
  }
  assert.ok(cs.some((c) => c.shapes.length === 0), "extreme coordinates are not rendered");
});

test("freedraw goldens: outline, svg path and loop fill (ex-213/214)", () => {
  const cs = cases("elements-freedraw.json");
  const variable = cs.filter((c) => c.element.strokeOptions?.variability !== "constant");
  const constant = cs.filter((c) => c.element.strokeOptions?.variability === "constant");
  assert.ok(variable.length >= 6 && constant.length >= 3);
  assert.ok(variable.some((c) => c.element.simulatePressure === true));
  assert.ok(variable.some((c) => c.element.simulatePressure === false));
  for (const sw of [1, 2, 4]) assert.ok(cs.some((c) => c.element.strokeWidth === sw), `sw ${sw}`);
  assert.ok(cs.some((c) => c.shapes.length === 2 && c.shapes[0].type === "rough"), "closed loop fill");
  for (const c of cs) {
    const svg = c.shapes[c.shapes.length - 1];
    assert.equal(svg.type, "svgPath", c.id);
    assert.ok(Array.isArray(c.outline) && c.outline.length > 0, `${c.id}: outline`);
    assert.equal(svg.d, svgPathFromStroke(c.outline), `${c.id}: svg path = getSvgPathFromStroke(outline)`);
  }
});

test("iframe-like goldens follow modifyIframeLikeForRoughOptions (shape.ts:262-293)", () => {
  const cs = cases("elements-iframe-like.json");
  const opts = (c) => c.shapes[0].drawable.options;
  const placeholder = cs.filter(
    (c) =>
      c.element.backgroundColor === "transparent" &&
      c.element.strokeColor === "transparent" &&
      (c.renderConfig.isExporting ||
        (c.element.type === "embeddable" && !c.renderConfig.validatedEmbeds.includes(c.element.id))),
  );
  assert.ok(placeholder.length >= 2);
  for (const c of placeholder) {
    assert.equal(opts(c).roughness, 0, c.id);
    assert.equal(opts(c).fill, "#d3d3d3", c.id);
    assert.equal(opts(c).fillStyle, "solid", c.id);
  }
  const iframe = cs.filter((c) => c.element.type === "iframe" && !placeholder.includes(c));
  assert.ok(iframe.length >= 1);
  for (const c of iframe) {
    assert.equal(opts(c).stroke, "#000000", c.id);
    assert.equal(opts(c).fill, "#f4f4f6", c.id);
  }
  assert.ok(
    cs.some((c) => c.element.type === "embeddable" && c.renderConfig.validatedEmbeds.includes(c.element.id)),
    "validated embeddable",
  );
});

test("freehand.json: perfect-freehand getStroke with Excalidraw's options (ex-213)", () => {
  const cs = cases("freehand.json");
  for (const sw of [1, 2, 4]) {
    assert.ok(
      cs.some(
        (c) =>
          c.options.size === sw * 4.25 &&
          c.options.thinning === 0.6 &&
          c.options.smoothing === 0.5 &&
          c.options.easing === "easeOutSine" &&
          c.options.last === true,
      ),
      `excalidraw options sw ${sw}`,
    );
  }
  assert.ok(cs.some((c) => c.options.streamline === 0.5));
  assert.ok(cs.some((c) => c.options.streamline !== 0.5));
  assert.ok(cs.some((c) => c.options.simulatePressure === false && c.points[0].length === 3));
  assert.ok(cs.some((c) => c.points.length === 1), "single point");
  for (const c of cs) {
    assert.ok(c.outline.length > 0, c.id);
    assert.ok(c.strokePoints.length > 0, c.id);
    for (const p of c.strokePoints) {
      for (const k of ["point", "pressure", "vector", "distance", "runningLength"]) {
        assert.ok(k in p, `${c.id}: strokePoint.${k}`);
      }
    }
  }
});

test("freedraw element outlines equal the direct perfect-freehand goldens they share inputs with", () => {
  const direct = new Map(cases("freehand.json").map((c) => [c.id, c]));
  let matched = 0;
  for (const c of cases("elements-freedraw.json")) {
    if (!c.freehandCase) continue;
    const d = direct.get(c.freehandCase);
    assert.ok(d, `${c.id} -> ${c.freehandCase}`);
    assert.deepEqual(c.outline, d.outline, c.id);
    matched++;
  }
  assert.ok(matched >= 3);
});
