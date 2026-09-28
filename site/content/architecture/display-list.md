+++
title = "Display list"
description = "The renderer-independent drawing vocabulary excali-scene produces and every backend paints: items, canvas semantics, and the Painter contract."
weight = 6
+++

Upstream renders by calling `CanvasRenderingContext2D` directly (`packages/element/src/renderElement.ts`, `packages/excalidraw/renderer/staticScene.ts`). The port splits that at one type, `excali_scene::display::DisplayList` ([ADR-008](../../decisions/adr-008-crate-boundaries/)): `excali-scene` turns elements into a list of draws, and the backends turn the list into pixels or markup without knowing what an element is.

| Backend | Crate | Consumes the list through |
|---|---|---|
| Browser canvas | `excali-canvas2d` | `paint(list, ctx)` over the `Context2d` trait; `WebCanvas` forwards to `web-sys` |
| Native pixels (PNG, CLI) | `excali-raster` | `render(list, pixmap, images, text)` into a tiny-skia `Pixmap` |
| SVG | `excali-svg` | the same list (ex-406) |

Both backends have a `no_element_knowledge` test: their only workspace dependency is `excali-scene`, and their sources name nothing from the element model or the shape generators.

## Items

`DisplayItem` has one variant per kind of canvas draw upstream makes:

| Item | Canvas calls it stands for |
|---|---|
| `Fill { path, color, rule }` | `fillStyle`, the path, `fill(rule)` |
| `Stroke { path, stroke }` | `strokeStyle`, `lineWidth`, `lineCap`, `lineJoin`, `miterLimit`, `setLineDash`, `lineDashOffset`, the path, `stroke()` |
| `Image(ImageItem)` | `imageSmoothingEnabled`, `filter`, `drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)` |
| `Text(TextRun)` | `font`, `fillStyle`, `textAlign`, `direction`, `fillText(text, x, y)` |
| `Group { transform, opacity, clip, items }` | `save()`, `transform(…)`, `globalAlpha *= opacity`, the clip path and `clip(rule)`, the children, `restore()` |

A `Path` is the list of `moveTo`, `lineTo`, `quadraticCurveTo`, `bezierCurveTo`, `arc` and `closePath` calls, with `Path::rect` and `Path::round_rect` building what the canvas methods of those names build. Images are named by id (upstream's `fileId`) and each backend resolves ids in its own store. Colours are CSS strings kept verbatim, so the SVG writer can print them as the element stores them.

`excali_scene::rough_canvas::draw` turns a roughjs `Drawable` into items exactly as roughjs 4.6.4's `RoughCanvas.draw` (`bin/canvas.js`) paints it: `path` sets become strokes in the stroke colour, `fillPath` sets fills (`evenodd` for `curve`, `polygon` and `path` shapes), `fillSketch` sets strokes in the fill colour at `fillWeight`, with the stroke and fill dashes and `fixedDecimalPlaceDigits` rounding.

## Semantics

The list means what the same calls mean on a canvas, so the Canvas 2D backend transliterates it and the others reproduce it.

- **Order.** Items paint in list order, source-over.
- **Transforms** compose as `ctx.transform`: a group's matrix applies to its children before the parent's. A matrix with an infinite or NaN entry is ignored.
- **Opacity** is `globalAlpha`: multiplied through nested groups and applied to each draw on its own, so overlapping draws in one element compound (upstream's multi-stroke rough lines darken where they cross). A product outside `0..=1` is ignored.
- **Clips** are in the group's space after its transform, intersect the enclosing clips and end with the group. An empty clip path hides everything.
- **Colours** resolve with upstream's parser (tinycolor 1.6.0, `excali_core::color`); a draw whose colour does not parse is not painted.
- **Canvas value rules**: an odd dash list repeats, a dash list with a negative or non-finite entry or a zero total draws solid, a line width or miter limit of 0 or less draws the default (1, 10), a path command with a non-finite argument is dropped, a drawing command with no subpath starts one, and an arc adds a line from the current point to its start.
- **Images**: the source rectangle defaults to the whole bitmap, is clipped to it with the destination clipped in proportion, and the dark-theme filter is `invert(93%) hue-rotate(180deg)` (upstream's numeric version for backends without CSS filters).
- **Text**: one run per line, `y` on the alphabetic baseline, `x` placed by `textAlign`.

## The Painter contract

`DisplayList::replay(&mut impl Painter)` walks the tree once for every backend. A `Painter` receives each draw with its absolute matrix and alpha (`PaintState`) and its colour resolved to `Rgba`, and receives clips as a `push_clip`/`pop_clip` stack. `replay_from` starts from a base state, such as the device-pixel-ratio scale upstream's `bootstrapCanvas` applies (`renderer/helpers.ts:73-127`).

The Canvas 2D backend isolates each draw in `save()`/`restore()` with `setTransform` and `globalAlpha` set first, and pushes a clip as `save()`, `setTransform`, the path, `clip(rule)`. The raster backend fills and strokes anti-aliased through tiny-skia, keeps clips as intersected coverage masks, draws images through a pattern shader (bilinear or nearest from the smoothing flag), and hands text to a `TextRasterizer` the caller provides from the font pipeline.
