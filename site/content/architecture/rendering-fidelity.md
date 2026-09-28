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
| Grid | bold `#dddddd`, regular `#e5e5e5` (dark: filtered); regular lines hidden when `gridSize × zoom < 10`; regular lines dashed `[w × 3, 1/zoom + (w + 1/zoom)]`; bold lines up to 4 CSS px, regular 1, snapped to whole device pixels once a device pixel wide |
| Opacity | `frameOpacity × elementOpacity / 10000`; erase preview ×0.2 |
| Bitmap cache padding | freedraw `sw × 12`; text `fontSize / 2`; arrow 40 (20 without head); else 20; caps area 16,777,216 and side 32,767 |
| Dark mode | colours through `invert(93%) hue-rotate(180deg)` computed numerically |

## Static scene

`excali_scene::static_scene::render_static_scene` is upstream's `renderStaticScene` (`packages/excalidraw/renderer/staticScene.ts`) as a display list, in upstream's order of work: the scroll snapped to whole device pixels (not when exporting), the device pixel ratio and the background (`bootstrapCanvas`: the view background through the dark filter, white when the canvas rejects the colour, none for `transparent`), the zoom, the grid, then every element that is not an iframe or embeddable with its bound text right after it (a label listed before its container waits for it) and its link icon, then the iframes and embeddables with their placeholder labels, then the pending flowchart nodes. An element whose drawing fails is skipped with its label and icon, as upstream's `try`/`catch` skips it. `excali_scene::render_element` is `renderElement`: alpha from the frame and the element (overrides first, ×0.2 while erasing, ×0.3 behind the element link selector), the render offset, frame outlines, rough.js shapes with round caps and joins, freedraw outlines filled as `new Path2D(d)` (`display::Path::from_svg_path_data`), text per line, images with their crop, flip, rounded clip and dark filter, and the image placeholders; upstream's built-in images (placeholders, link icons) are named by id with their exact data URLs (`render_element::builtin_image`).

`tools/goldens/static-scene.mjs` runs upstream's `renderStaticScene` under jsdom on a recording 2D context and keeps every fill, stroke, clip, text and image in order with its path, matrix, alpha and styles: 34 scenes of grids at zooms around the cutoff and at device pixel ratios 1, 1.5 and 2, backgrounds (transparent, none, rejected, dark), every element kind in the editor, exporting, dark and zoomed, link icons, opacity and overrides, the element link selector and pending flowchart nodes. `crates/excali-scene/tests/static_scene.rs` replays the port's list and matches all of them draw for draw (matrices and numbers to 1e-9, colours as the canvas resolves them).

Three differences, by design:

- **Vectors in the editor.** Upstream draws each element once into a bitmap of its own and blits it in the editor (`generateElementWithCanvas`, `drawElementFromCanvas`); the port draws vectors, as upstream does when exporting. The difference is the bitmap's resampling and its placement on whole device pixels (at most half a device pixel). The golden generator patches upstream to its export path for the same reason, and says so.
- **Link icon canvas.** Upstream keeps each link icon's canvas while the zoom is unchanged, background included; the port draws it afresh, through the same mapping and clip.
- **Not here.** Sticky notes are ex-703's and the frame clip of frame children (`clipElementToFrame`) is ex-403's; until then the static scene leaves both out.

## Export structure

SVG output must reproduce upstream's document: `<!-- svg-source:excalidraw -->`, `<metadata>` payload, `<defs>` with a `clipPath` per frame (`rx 8`), `<style class="style-fonts">` with subsetted `@font-face` rules, background `<rect>`, then one `<g stroke-linecap="round" transform="translate(ox oy) rotate(deg cx cy)">` per element. Numbers are written with two decimals (`MAX_DECIMALS_FOR_SVG_EXPORT = 2`). The golden for this is `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap`.

The document around the elements (ex-406) is split along the display-list boundary. `excali_scene::export::svg_document` computes it from the elements as upstream's `exportToSvg` does (`scene/export.ts:293-470`). The canvas is the common bounds of the root elements and the frame name labels, plus the padding, or the exported frame alone with no padding. Bounds follow `getElementBounds`, including rough.js curve extremes for lines and arrows and arrow labels at their `labelPosition`. The embedded scene is `serializeAsJSON(…, "local")` through the payload codec. Each frame gets a clip rectangle, and the font faces come from `generateFontFaceDeclarations`. `excali_svg::export_to_svg` writes that document as the DOM nodes upstream creates, and `outerHTML` serializes them with the HTML fragment serializer. Every number is printed as JavaScript prints it; a rough.js path has two decimals (`excali_svg::path::rough_path_data`).

`tools/goldens/svg-export.mjs` runs upstream's `exportToSvg` under jsdom 22.1.0, the DOM of upstream's own test suite. It records 29 scenes: the document before the elements, and the element bounds, frame labels and canvas size behind it. The port reproduces all of them byte for byte (`crates/excali-svg/tests/document.rs`, `crates/excali-scene/tests/export_bounds.rs`). The generator's own test checks its scenes against upstream's vitest snapshot: the same root, comment, metadata, style block and font family order, and the same embedded scene apart from the export source.

Two differences remain:

- **Font content.** Upstream subsets each face with HarfBuzz in a worker. `excali_svg::FontFiles` inlines the vendored file whole. That draws the same glyphs in a larger document; ex-408 decides whether to subset.
- **Math.** Rotations use `excali_math::js::{sin, cos}` (fdlibm through `libm`), because V8's `Math.sin`/`Math.cos` and macOS libm differ in the last bit, for example at `sin(4)`.

PNG export: canvas = common bounds + 2 × 10 padding, times `exportScale`; no grid; background optional; scene payload in a `tEXt` chunk.

## Text: trust the file, then measure the same way

Text elements store `width`, `height` and the wrapped `text` computed by the browser that wrote them. On load the port trusts those values. When the user edits, or when a container resizes, the port measures with the same font files and the same rules (tab = 8 spaces, empty line = one space, advance width of the widest line). The [risks page](../../plan/risks/) tracks this as the main fidelity risk; task `ex-308` measures it across the corpus and gates Excalifont, Nunito and Comic Shanns at 0.5 px ([ADR-007](../../decisions/adr-007-text-metrics/)).

## Golden tests

Task `ex-004` generates goldens by running upstream's own shape builders under Node with `roughjs@4.6.4` and `perfect-freehand@1.2.0` on the fixture elements; the Rust crates must match op by op at two decimals. Task `ex-206` ran the same goldens against the `roughr` crate so the decision to port was made on numbers: roughr 0.14.0 matches 7.5 % of them, and 37.2 % even with rough.js's random generator patched in, so rough.js 4.6.4 is ported into `excali-rough` ([ADR-003](../../decisions/adr-003-sketch-renderer/), `tools/roughr-eval`).
