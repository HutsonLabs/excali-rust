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

Both backends have a `no_element_knowledge` test: `cargo metadata --format-version 1 --no-deps` must show `excali-scene` as their only workspace dependency (normal, dev, build and target-specific, by package name, so a renamed dependency such as `ec = { package = "excali-core", .. }` counts as `excali-core`; `the_dependency_check_sees_renamed_packages` pins that on a throwaway workspace), it may reach that crate only through `excali_scene::display`, and no identifier in their code may contain `Element` (comments aside), with `HtmlImageElement`, the browser image type `WebCanvas` draws from, the one allowed name. `excali-scene`'s `display_module_does_not_know_elements` holds the `display` module to the same identifier rule and to `excali_core::color` and `excali_core::json` as its only paths into `excali-core`, with no path out of `display` into the rest of the crate: a `super` inside a use group (`super::{super::x}`), a `super`, `self` or `crate` after `::` and a renamed `self`, `super` or `crate` are rejected along with `super::super` and `crate::` anything but `display`.

## Items

`DisplayItem` has one variant per kind of canvas draw upstream makes:

| Item | Canvas calls it stands for |
|---|---|
| `Fill { path, color, rule }` | `fillStyle`, the path, `fill(rule)` |
| `Stroke { path, stroke }` | `strokeStyle`, `lineWidth`, `lineCap`, `lineJoin`, `miterLimit`, `setLineDash`, `lineDashOffset`, the path, `stroke()` |
| `Image(ImageItem)` | `imageSmoothingEnabled`, `filter`, `drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)` |
| `Text(TextRun)` | `font`, `fillStyle`, `textAlign`, `direction`, `fillText(text, x, y)` |
| `Group { transform, opacity, clip, items }` | `save()`, `transform(…)`, `globalAlpha *= opacity`, the clip path and `clip(rule)`, the children, `restore()` |

A `Path` is the list of `moveTo`, `lineTo`, `quadraticCurveTo`, `bezierCurveTo`, `arc` and `closePath` calls, with `Path::rect` and `Path::round_rect` building what the canvas methods of those names build. Images are named by id (upstream's `fileId`) and each backend resolves ids in its own store. Colours are CSS strings kept verbatim: upstream assigns `element.strokeColor` and `backgroundColor` to `fillStyle`/`strokeStyle` as stored (`applyDarkModeFilter` returns them unchanged unless the theme is dark), so the Canvas 2D backend assigns the same string and the SVG writer prints it as the element stores it.

`excali_scene::rough_canvas::draw` turns a roughjs `Drawable` into items exactly as roughjs 4.6.4's `RoughCanvas.draw` (`bin/canvas.js`) paints it: `path` sets become strokes in the stroke colour, `fillPath` sets fills (`evenodd` for `curve`, `polygon` and `path` shapes), `fillSketch` sets strokes in the fill colour at `fillWeight`, with the stroke and fill dashes and `fixedDecimalPlaceDigits` rounding.

## Semantics

The list means what the same calls mean on a canvas, so the Canvas 2D backend transliterates it and the others reproduce it.

- **Order.** Items paint in list order, source-over.
- **Transforms** compose as `ctx.transform`: a group's matrix applies to its children before the parent's. A matrix with an infinite or NaN entry is ignored as a whole. The canvas ignores only the one call with the bad argument, and upstream positions an element with two calls, `translate(cx, cy)` then `rotate(angle)`, so an element with a NaN angle draws translated and unrotated. A single group of `translate × rotate(NaN)` would lose the translation as well, so producers emit one group per canvas call (the translation's group holding the rotation's) or leave out only the non-finite factor.
- **Opacity** is `globalAlpha`: multiplied through nested groups and applied to each draw on its own, so overlapping draws in one element compound (upstream's multi-stroke rough lines darken where they cross). A product outside `0..=1` is ignored.
- **Clips** are in the group's space after its transform, intersect the enclosing clips and end with the group. An empty clip path hides everything.
- **Colours** are parsed as the canvas parses a `fillStyle` assignment. The Canvas 2D backend leaves that to the browser; for the others `Color::rgba` is a CSS Color 4 parser (named, system and hex colours, `rgb`/`rgba`/`hsl`/`hsla` in both syntaxes, `hwb`, `lab`, `lch`, `oklab`, `oklch`, `color()` in the predefined spaces clipped to sRGB, `none`, `calc()`, `min()`, `max()` and `clamp()`), checked case by case against headless Chrome (`scripts/fixtures/css-color-goldens.sh`, `crates/excali-scene/tests/fixtures/css-colors.json`). So `rgb(255 0 0 / 50%)` is half-transparent red, and `ff0000` or `hsv(0,100%,100%)` are not colours. Not covered: CSS Color 5's `color-mix()` and relative colours, and math functions other than `calc()`, `min()`, `max()` and `clamp()` (`round()`, `mod()`, ...). An assignment the canvas ignores (`""`, `none`, `blue-ish`, which restore keeps as stored) leaves the current style, so the draw paints in it: black on a fresh context (upstream draws each element on a fresh element canvas inside `save()`/`restore()`), or the base state's `fill_style`/`stroke_style`.
- **Canvas value rules**: an odd dash list repeats, a dash list with a negative or non-finite entry or a zero total draws solid, a line width or miter limit of 0 or less draws the default (1, 10), a path command with a non-finite argument is dropped, a drawing command with no subpath starts one, and an arc adds a line from the current point to its start.
- **Images**: the source rectangle defaults to the whole bitmap, is clipped to it with the destination clipped in proportion, and the dark-theme filter is `invert(93%) hue-rotate(180deg)` (upstream's numeric version for backends without CSS filters).
- **Text**: one run per line, `y` on the alphabetic baseline, `x` placed by `textAlign`.

## The Painter contract

`DisplayList::replay(&mut impl Painter)` walks the tree once for every backend. A `Painter` receives each draw with its absolute matrix, alpha and current styles (`PaintState`), the item's colour string and that colour resolved to `Rgba` (the current style when the canvas would ignore it), and receives clips as a `push_clip`/`pop_clip` stack. `replay_from` starts from a base state, such as the device-pixel-ratio scale upstream's `bootstrapCanvas` applies (`renderer/helpers.ts:73-127`). That scale is all `bootstrapCanvas` contributes: it paints the background inside `save()`/`restore()`, so no `fillStyle` survives it, and its `COLOR_WHITE` seed (an invalid background paints white) covers only the background rectangle, which the producer emits. A base `fill_style`/`stroke_style` is for a caller whose context already holds styles.

The Canvas 2D backend isolates each draw in `save()`/`restore()` with `setTransform` and `globalAlpha` set first, assigns the colour string as given so the browser parses it, and pushes a clip as `save()`, `setTransform`, the path, `clip(rule)`. `paint` and `paint_scaled` expect a fresh context; `paint_from` first sets the base state's styles. The raster backend fills and strokes anti-aliased through tiny-skia, keeps clips as intersected coverage masks, draws images through a pattern shader (bilinear or nearest from the smoothing flag), and hands text to a `TextRasterizer` the caller provides from the font pipeline.
