+++
title = "Rendering fidelity"
description = "What makes a port look identical to Excalidraw, and the exact constants the port must reproduce."
weight = 3
+++

## The three sources of the look

1. **Rough.js** turns a shape into sketchy strokes. Given the same options and the same `seed`, its output is deterministic. Excalidraw stores `seed` per element for exactly this reason (`types.ts:56-58`).
2. **perfect-freehand** turns pointer samples into a pressure-shaped outline for the pencil tool.
3. **Fonts** with particular metrics: the hand-drawn look of text is Excalifont (default) or Virgil, and text boxes are sized from those metrics.

term.hut's viewer proved the first two can be reproduced outside React (its file header notes that the same seed renders "identically on every open, and the same as it does in Excalidraw proper"). The port extends that to the full editor.

## Rough.js contract

The generator's random source, from `rough-stuff/rough` `src/math.ts` (verified on `master`, package version 4.6.6; upstream pins 4.6.4):

```ts
next(): number {
  if (this.seed) {
    return ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31;
  } else {
    return Math.random();
  }
}
```

The port must consume random numbers in the same order as rough.js for every primitive, or every shape differs. This is why `excali-rough` is verified by goldens rather than by eye.

Default options (`src/generator.ts`): `maxRandomnessOffset 2, roughness 1, bowing 1, strokeWidth 1, curveTightness 0, curveFitting 0.95, curveStepCount 9, fillStyle hachure, fillWeight -1, hachureAngle -41, hachureGap -1, dashOffset -1, dashGap -1, zigzagOffset -1, seed 0, disableMultiStroke false, disableMultiStrokeFill false, preserveVertices false, fillShapeRoughnessGain 0.8`.

## Excalidraw's option mapping

From `packages/element/src/shape.ts:195-260` (research: rendering section 1):

| option | value |
|---|---|
| `seed` | `element.seed` |
| `strokeLineDash` | dashed `[8, 8 + sw]`; dotted `[1.5, 6 + sw]`; solid none |
| `disableMultiStroke` | `strokeStyle !== "solid"` |
| `strokeWidth` | `sw + 0.5` when not solid, else `sw` |
| `fillWeight` | `sw / 2` |
| `hachureGap` | `sw * 4` |
| `roughness` | `adjustRoughness(element)` |
| `stroke` | stroke colour through the dark filter when in dark mode |
| `preserveVertices` | `continuousPath || roughness < 2` |
| `fill`, `fillStyle` | for rectangle, iframe, embeddable, diamond, ellipse when background is not transparent; for line/freedraw only when the path is a loop; never for arrow |
| `curveFitting` | `1` for ellipse |

`adjustRoughness` leaves roughness alone when (min side ≥ 20 and max side ≥ 50), or (rounded and min side ≥ 15), or (linear and max side ≥ 50); otherwise `min(roughness / (maxSize < 10 ? 3 : 2), 2.5)`.

## Shape constants

| Shape | Constants |
|---|---|
| Corner radius | proportional `0.25 × min(w,h)`; adaptive `32` px until `min(w,h) ≤ 32/0.25 = 128`, then proportional |
| Diamond | `topX = floor(w/2)+1`, `rightY = floor(h/2)+1`; rounded corners as cubic `C` segments |
| Ellipse | `generator.ellipse(w/2, h/2, w, h)` with `curveFitting 1` |
| Elbow arrow | path corner radius 16; skipped if any coordinate magnitude > 1e6 |
| Arrowheads | sizes: arrow 25, diamond 12, crowfoot 15, cardinality marker 20, default 15; angles: bar 90°, arrow 20°, default 25°; direction from the last bezier at t = 0.3; `minSize = min(size, lastSegment × (diamond ? 0.25 : 0.5))`; circle diameter `hypot + sw − 2` |
| Freedraw (variable) | perfect-freehand `size = sw × 4.25`, `thinning 0.6`, `smoothing 0.5`, `streamline` from the element (default 0.5), easing `sin(t·π/2)`, `last true` |
| Freedraw (constant) | laser-pointer `size = sw × 1.4`, `simplify 0`, pressure 1 |
| Freedraw path | `M p0 Q p_i mid(p_i,p_i+1) … L p0 Z`, numbers trimmed to 2 decimals |
| Text | `lineHeightPx = fontSize × lineHeight`; `verticalOffset = em×ascender + (lineHeightPx − em×ascender + em×descender)/2` with `em = fontSize/unitsPerEm` |
| Frame | stroke `#bbb`, width `2/zoom`, radius `8/zoom`; name 14 px Helvetica, colour `#999999` light / `#7a7a7a` dark |
| Grid | bold `#dddddd`, regular `#e5e5e5` (dark: filtered); regular lines hidden when `gridSize × zoom < 10` |
| Opacity | `frameOpacity × elementOpacity / 10000`; erase preview ×0.2 |
| Bitmap cache padding | freedraw `sw × 12`; text `fontSize / 2`; arrow 40 (20 without head); else 20; caps area 16,777,216 and side 32,767 |
| Dark mode | colours through `invert(93%) hue-rotate(180deg)` computed numerically |

## Export structure

SVG output must reproduce upstream's document: `<!-- svg-source:excalidraw -->`, `<metadata>` payload, `<defs>` with a `clipPath` per frame (`rx 8`), `<style class="style-fonts">` with subsetted `@font-face` rules, background `<rect>`, then one `<g stroke-linecap="round" transform="translate(ox oy) rotate(deg cx cy)">` per element. Numbers are written with two decimals (`MAX_DECIMALS_FOR_SVG_EXPORT = 2`). The golden for this is `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap`.

PNG export: canvas = common bounds + 2 × 10 padding, times `exportScale`; no grid; background optional; scene payload in a `tEXt` chunk.

## Text: trust the file, then measure the same way

Text elements store `width`, `height` and the wrapped `text` computed by the browser that wrote them. On load the port trusts those values. When the user edits, or when a container resizes, the port measures with the same font files and the same rules (tab = 8 spaces, empty line = one space, advance width of the widest line). The [risks page](../../plan/risks/) tracks this as the main fidelity risk; task `ex-308` measures it across the corpus.

## Golden tests

Task `ex-004` generates goldens by running upstream's own shape builders under Node with `roughjs@4.6.4` and `perfect-freehand@1.2.0` on the fixture elements; the Rust crates must match op by op at two decimals. Task `ex-206` runs the same goldens against the `roughr` crate before any port is written, so the decision to port is made on numbers.
