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

`excali_scene::static_scene::render_static_scene` is upstream's `renderStaticScene` (`packages/excalidraw/renderer/staticScene.ts`) as a display list, in upstream's order of work: the scroll snapped to whole device pixels (not when exporting), the device pixel ratio and the background (`bootstrapCanvas`: the view background through the dark filter, white when the canvas rejects the colour, none for `transparent`), the zoom, the grid, then every element that is not an iframe or embeddable with its bound text right after it (a label listed before its container waits for it) and its link icon, then the iframes and embeddables with their placeholder labels, then the pending flowchart nodes. An element whose drawing fails is skipped with its label and icon, as upstream's `try`/`catch` skips it. A frame's children are drawn inside the frame's clip (`clipElementToFrame`, `excali_scene::frame`): a `roundRect` of the frame's size with radius `8 / zoom` at its corner, unrotated even for a rotated frame, holding the element, its bound text or placeholder label and, for iframes and embeddables, its link icon. An element is clipped to its target frame (its own, or the highlighted frame while it is dragged over it, `getTargetFrame`) when frames render and clip and it or the frame is drawn at a render offset, or when `shouldApplyFrameClip` says so: its outline (`getElementLineSegments`: sides and flattened corners of boxes and diamonds, 90 chords of ellipses, the flattened rough.js curves or closed polygon of lines and arrows, freedraw points, text boxes) crosses the frame's, or its bounds contain the frame's, or it is grouped outside the frame and its group belongs there (by `frameId` at rest, by `isElementInFrame` while dragging, with one group cache for the scene). `excali_scene::render_element` is `renderElement`: alpha from the frame and the element (overrides first, ×0.2 while erasing, ×0.3 behind the element link selector), the render offset, frame outlines, rough.js shapes with round caps and joins, freedraw outlines filled as `new Path2D(d)` (`display::Path::from_svg_path_data`), text per line, images with their crop, flip, rounded clip and dark filter, and the image placeholders; upstream's built-in images (placeholders, link icons) are named by id with their exact data URLs (`display::builtin_image`, ids `excalidraw:…`, which the raster and Canvas 2D backends resolve themselves; `display::builtin_image_by_id`). Upstream's `fillRect` calls (the background, the placeholder box, the link icon's background) are `DisplayItem::FillRect`, which the canvas anti-aliases as a rectangle rather than a path.

`tools/goldens/static-scene.mjs` runs upstream's `renderStaticScene` under jsdom on a recording 2D context and keeps every fill, stroke, clip, text and image in order with its path, matrix, alpha and styles: 46 scenes of grids at zooms around the cutoff and at device pixel ratios 1, 1.5 and 2, backgrounds (transparent, none, rejected, dark), every element kind in the editor, exporting, dark and zoomed, link icons, opacity and overrides, the element link selector, pending flowchart nodes, and frame clipping (children of every kind crossing, containing and inside a frame, grouped children outside it, a rotated magic frame, zooms 1.5 at dpr 2 and 0.5, exporting, dark, clip off, frames off, render offsets, dragging over a highlighted frame, editing a group). `tools/goldens/frame-clip.mjs` records upstream's frame decisions and outlines on those scenes element by element (`getElementLineSegments`, `getElementBounds`, `getTargetFrame`, `isElementIntersectingFrame`, `isElementContainingFrame`, `elementsAreInFrameBounds`, `elementOverlapsWithFrame`, `isElementInFrame`, `shouldApplyFrameClip`), which `crates/excali-scene/tests/frame_clip.rs` matches. `crates/excali-scene/tests/static_scene.rs` replays the port's list and matches all of them draw for draw (matrices and numbers to 1e-9, colours as the canvas resolves them).

Four differences, by design:

- **Vectors in the editor.** Upstream draws each element once into a bitmap of its own and blits it in the editor (`generateElementWithCanvas`, `drawElementFromCanvas`); the port draws vectors, as upstream does when exporting. The difference is the bitmap's resampling and its placement on whole device pixels (at most half a device pixel). The golden generator patches upstream to its export path for the same reason, and says so.
- **Link icon canvas.** Upstream keeps each link icon's canvas while the zoom is unchanged, background included; the port draws it afresh, through the same mapping and clip.
- **Not here.** Sticky notes are ex-703's; until then the static scene leaves them out.
- **A failure inside a frame clip.** Upstream's `try` around each element holds `context.save()`, the frame clip (`clipElementToFrame`), `renderElement` for the element and its bound text or placeholder label, and `context.restore()` (`staticScene.ts:397-452` and `:455-505`). When the element or its label throws after the clip is applied, the restore is never reached: the save and the frame clip leak, and every later element in the scene is drawn inside that frame's clip. The port keeps the clip to the failing element's own items and draws the later elements as it would had the element drawn (`a_clipped_element_whose_label_cannot_draw_does_not_clip_what_follows` in `crates/excali-scene/tests/static_scene.rs`).

## Export structure

SVG output must reproduce upstream's document: `<!-- svg-source:excalidraw -->`, `<metadata>` payload, `<defs>` with a `clipPath` per frame (`rx 8`), `<style class="style-fonts">` with subsetted `@font-face` rules, background `<rect>`, then one `<g stroke-linecap="round" transform="translate(ox oy) rotate(deg cx cy)">` per element. Numbers are written with two decimals (`MAX_DECIMALS_FOR_SVG_EXPORT = 2`). The golden for this is `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap`.

The document around the elements (ex-406) is split along the display-list boundary. `excali_scene::export::svg_document` computes it from the elements as upstream's `exportToSvg` does (`scene/export.ts:293-470`). The canvas is the common bounds of the root elements and the frame name labels, plus the padding, or the exported frame alone with no padding. Bounds follow `getElementBounds`, including rough.js curve extremes for lines and arrows and arrow labels at their `labelPosition`. The embedded scene is `serializeAsJSON(…, "local")` through the payload codec. Each frame gets a clip rectangle, and the font faces come from `generateFontFaceDeclarations`. `excali_svg::export_to_svg` writes that document as the DOM nodes upstream creates, and `outerHTML` serializes them with the HTML fragment serializer. Every number is printed as JavaScript prints it; a rough.js path has two decimals (`excali_svg::path::rough_path_data`).

The elements (ex-407) follow the same split. `excali_scene::svg_scene::render_scene_to_svg` is upstream's `renderSceneToSvg` (`renderer/staticSvgScene.ts:97-933`): the elements in order with iframes and embeddables last and each container's label after it, each placed by `translate(offsetX offsetY) rotate(degrees cx cy)`, opacity as `stroke-opacity` and `fill-opacity`, a frame's children wrapped in `<g clip-path="url(#frame)">`, links as `<a href>` around what the element draws. Rectangles, diamonds, ellipses, lines, arrows and a freedraw's fill are rough.js's `RoughSVG.draw` with two-decimal path data (a `<path>` per op set: stroke, fill with `evenodd` for curves and polygons, or hachure in the fill colour at `fillWeight`). Lines and arrows get a `<mask>` after them, empty unless an arrow label cuts its padded box out. A freedraw's outline is a path filled with the stroke colour. Text is a `<text>` per line on its baseline with `text-anchor` start, middle or end. Images are a `<symbol>` per file (per crop, or per element without `reuseImages`) put first in `<defs>`, or first in the anchor of a linked image, and a `<use>` scaled for flips, masked for crops and clipped to rounded corners, with `DARK_THEME_FILTER` on SVG files in dark mode. Frames draw their `#bbb` outline, iframes and embeddables their placeholder and label and then a link or, with `renderEmbeddables`, an `<iframe>` in a `<foreignObject>` loading `getEmbedLink`'s URL (`excali_core::embeddable`). The result is `display::SvgNode` trees in `SvgDocument::symbols` and `SvgDocument::nodes`, which `excali_svg::export_to_svg` writes. What upstream throws on (a selection, a shape rough.js cannot draw) stops that element where it is, as upstream's `try`/`catch` does. With `SvgExportOptions::data_ids` every element's node carries its `data-id`, as in upstream's test mode, and the labels export makes are named `id0`, `id1`, ... as `randomId` names them there.

`tools/goldens/svg-export.mjs` runs upstream's `exportToSvg` under jsdom 22.1.0, the DOM of upstream's own test suite, in test mode. It records 48 scenes: the document before the elements, the whole document, and the element bounds, frame labels and canvas size behind it. The scenes cover upstream's export test, every element type, fill and stroke styles, every arrowhead, arrow labels, text lines, alignment, direction and fonts, images (shared, cropped, flipped, rounded, SVG in the dark theme, without `reuseImages`, missing files, linked), freedraw, links, embeddables as links and rendered, frame children clipped, unclipped and exported, and a refused selection. The port reproduces every one byte for byte (`crates/excali-svg/tests/document.rs`, `crates/excali-svg/tests/elements.rs`, `crates/excali-scene/tests/export_bounds.rs`), and upstream's `getEmbedLink` for 45 links (`crates/excali-core/tests/embed_links.rs`). `crates/excali-svg/tests/upstream_snapshot.rs` reproduces the four snapshots of `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap` after whitespace normalisation, printed as vitest's pretty-format prints them, with the test's page origin and the font faces and subsets upstream's test environment produced (below). The generator's own test checks its scenes against the same snapshot.

These differences remain:

- **Font content.** Upstream subsets each face with HarfBuzz (harfbuzzjs 0.3.6) in a worker. `excali_svg::FontFiles` subsets it with skera, fontations' port of hb-subset, given upstream's input, and encodes WOFF2 with ttf2woff2 ([ADR-010](../../decisions/adr-010-svg-font-subsetting/)). Measured on upstream's own subsets of 153 faces in 46 scenes (`tools/font-subset-eval`), every character draws with upstream's glyph, advance and shaping, and every run is pixel-identical in Chromium. The font bytes come to 97.6 % of upstream's. They are not upstream's bytes: the subsetter and the brotli encoder differ.
- **Math.** Rotations use `excali_math::js::{sin, cos}` (fdlibm through `libm`), because V8's `Math.sin`/`Math.cos` and macOS libm differ in the last bit, for example at `sin(4)`.
- **The snapshot's fonts.** Upstream's test `FontFace` gives every face the range `U+0000-00FF` (`setupTests.ts:65-86`), so its snapshot inlines every face of each family used, where a browser, and the port, inline the faces whose ranges hold the text. The snapshot test takes those faces and their HarfBuzz subsets from `tools/font-subset-eval/upstream-subsets.json` (recorded under the same `FontFace`) after checking that the port's own faces are among them, family by family in the same order.
- **Sticky notes.** Their shadow, fill, edge and footer are ex-703's; the SVG export skips a sticky note, as the static scene does. The element tests leave upstream's sticky note nodes out of its documents.
- **`border: none`.** A rendered embeddable's `<foreignObject>` and `<iframe>` set `style.border = "none"`. jsdom's cssstyle drops the declaration, so upstream's recorded documents have no border in their `style`; Chrome 153 keeps it as its four longhands (`border-width: medium; border-style: none; border-color: currentcolor; border-image: none;`), and so does the port, so the iframe has no border where the SVG is shown. The element tests insert the longhands into upstream's documents.
- **Symbol lookup.** An image's symbol is looked up with `querySelector("#" + id)`. For ids of CSS name characters that is an id lookup, and an id followed by `.class` or `#id` parts is a valid selector that matches nothing, both as upstream; any other character (a colon, a bracket, an escape, a space) is taken as an invalid selector that stops the element, where some of them are valid selectors upstream.
- **Label ids.** Outside test mode upstream names the labels export makes with `nanoid`; they are never written, and the port names them after their frame or embeddable.

### PNG export

The PNG export (ex-405) is split the same way. `excali_scene::canvas_export::export_to_canvas` is upstream's `exportToCanvas` (`scene/export.ts:180-285`) as a `display::CanvasDocument`:

- **Elements.** Exporting a frame draws the elements that overlap it and are in no other frame (`getElementsOverlappingFrame`), with frame clipping off. Otherwise each frame's name label, a Helvetica text element cut to the frame's width, goes in before the frame when frame names are on.
- **Size.** The canvas is the common bounds of the root elements (labels included), or of the exported frame, plus `exportPadding` on every side: `DEFAULT_EXPORT_PADDING` = 10, and 0 for a frame. The default `createCanvas` multiplies that by `appState.exportScale` and draws at that scale. The utils wrapper's (`utils/src/export.ts:64-105`, `CanvasSizing::Utils`) fits the larger side to `maxWidthOrHeight` when the content is larger (else it uses the caller's `exportScale`), or takes `getDimensions`, or keeps the content size at scale 1. The canvas holds what its `width` and `height` attributes do after the assignment: a WebIDL `unsigned long` (truncated, modulo 2^32), and 300 × 150 when that is past 2^31 - 1, which is what a negative padding larger than the content gives.
- **Drawing.** `renderStaticScene` with the scroll at `-min + padding`, zoom 1, no grid, `isExporting`, the export theme, and the background colour only with `exportBackground`. Images come from an image cache of the files the caller can load; a file that does not load, a binary file and a missing file draw upstream's placeholder.

`export_canvas_png` is the editor's `exportCanvas("png")` (`data/index.ts:98-192`): it refuses an empty scene and, with `exportEmbedScene`, embeds `serializeAsJSON(elements, appState, files, "local")` as `encodePngMetadata` does, compressed in the payload wrapper, in a `tEXt` chunk keyed `application/vnd.excalidraw+json` (`png_payload`). `excali_raster::export_png` paints the list on a transparent pixmap of the canvas size and encodes it as 8-bit RGBA with the `tEXt` chunk before `IEND`; a canvas with no pixels has no blob, as `toBlob` gives none (upstream's `CANVAS_POSSIBLY_TOO_BIG`).

`tools/goldens/png-export.mjs` runs upstream's `exportToCanvas` and the utils `exportToCanvas` on the recording context of the static scene. It records 26 scenes: padding 0, 10, 25.5 and -100, export scales 1, 1.5, 2 and 3, background on, off, transparent, coloured and dark, frame labels (truncated, dark, off, frames off), an exported frame, loaded, broken, binary and missing images, embeddables, and `maxWidthOrHeight` and `getDimensions`. For each it keeps the canvas's width and height, every draw, and with `exportEmbedScene` the scene text upstream embeds. `crates/excali-scene/tests/canvas_export.rs` matches every size and every draw, and the embedded text byte for byte. `crates/excali-raster/tests/png_export.rs` checks the encoded PNGs: the IHDR size, the pixels against the rendered canvas, the background pixel, and the `tEXt` chunk before `IEND` decoding to upstream's text. In CI, the `png_export` example writes the port's PNG of every scene, and upstream's own `decodePngMetadata` and `loadFromBlob` read them back (`png-export.mjs --reimport`). Each PNG must have upstream's canvas size and give back the embedded text, and each embedded scene must restore to what upstream restores from its own export.

Text pixels come from the caller's `TextRasterizer`, as for every raster draw. The [command line](../cli/)'s `excali render` fills the glyph outlines of each run shaped from the vendored font files (`FontStore::shape_line`), with tiny-skia's coverage rather than Chrome's.

## Text: trust the file, then measure the same way

Text elements store `width`, `height` and the wrapped `text` computed by the browser that wrote them. On load the port trusts those values. When the user edits, or when a container resizes, the port measures with the same font files and the same rules (tab = 8 spaces, empty line = one space, advance width of the widest line). The [risks page](../../plan/risks/) tracks this as the main fidelity risk; task `ex-308` measures it across the corpus and gates Virgil, Cascadia, Excalifont, Nunito, Lilita One and Comic Shanns at 0.5 px (Virgil since `ex-g301`, Cascadia and Lilita One since `ex-g302`) ([ADR-007](../../decisions/adr-007-text-metrics/)).

## Golden tests

Task `ex-004` generates goldens by running upstream's own shape builders under Node with `roughjs@4.6.4` and `perfect-freehand@1.2.0` on the fixture elements; the Rust crates must match op by op at two decimals. Task `ex-206` ran the same goldens against the `roughr` crate so the decision to port was made on numbers: roughr 0.14.0 matches 7.5 % of them, and 37.2 % even with rough.js's random generator patched in, so rough.js 4.6.4 is ported into `excali-rough` ([ADR-003](../../decisions/adr-003-sketch-renderer/), `tools/roughr-eval`).
