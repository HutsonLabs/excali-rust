+++
title = "Build log"
description = "What landed on main, newest first."
weight = 5
+++

Each entry records a task that merged to main: the date, the task id and title, what now works, and the pull request that landed it. The newest entries are at the top. The [progress page](@/plan/progress.md) has the full status of the task graph.

## 2026-09-28 · ex-507 · Hit testing (collision.ts): thresholds, inside/outline rules, per-shape intersections

The editor now does hit testing the way upstream's `collision.ts` and `distance.ts` do. That covers the zoom-scaled hit threshold, the cached `hitElementItself` with its inside-or-outline rule, point-in-element, and element/segment intersections with offsets. It also covers the binding hit tests and `distanceToElement` for every element type. The port matches upstream on 2,481 probes plus the intersection, binding and containment scenes, and every describe block of `collision.test.tsx` is ported. PR: [#80](https://github.com/HutsonLabs/excali-rust/pull/80).

## 2026-09-28 · ex-407 · SVG elements: rough paths, freedraw, text per line, images as symbol/use, arrow-label masks

SVG export now draws the elements as upstream's `renderSceneToSvg` does: rough.js paths, freedraw outlines, one `<text>` per line, images as shared `<symbol>` and `<use>` with crop masks and clips, arrow-label masks, frame clips, embeddables as links or iframes, and link anchors. The port reproduces the four snapshots of upstream's `export.test.ts` and 48 whole `exportToSvg` documents byte for byte. PR: [#79](https://github.com/HutsonLabs/excali-rust/pull/79).

## 2026-09-28 · ex-409 · excali-cli: validate, render (png), export (svg), lib (list/merge), with exit codes

The `excali` command line now validates scenes and libraries (including scenes embedded in PNG and SVG), renders PNG with text from the vendored fonts, exports SVG with subset fonts, and lists and merges libraries as upstream's `mergeLibraryItems` does, with exit codes 0-4. A canvas beyond the browser limits is refused with "Error: Canvas too big" instead of crashing. PR: [#78](https://github.com/HutsonLabs/excali-rust/pull/78).

## 2026-09-28 · ex-506 · Editor state machine: tools registry (keys, fillable, toggle), active tool, tool lock, pen mode

The editor now has upstream's tool registry: the TOOLS table with its letter and number keys, a caps-lock insensitive `find_shape_by_key`, and the fillable and toggle flags. `ToolState` holds the active tool, tool lock and pen mode, applies forced tools, and resets an unsupported tool to selection while interaction is off, in the order upstream's componentDidUpdate uses. PR: [#77](https://github.com/HutsonLabs/excali-rust/pull/77).

## 2026-09-28 · ex-501 · WASM build pipeline: wasm-bindgen --target web, wasm-opt, size check in CI

`scripts/web/build.sh` now builds the editor for the browser: the `web-release` profile, `wasm-bindgen --target web`, then `wasm-opt -Oz` from binaryen 133, pinned and SHA-256 verified. It writes `dist/excali_editor.js` and `dist/excali_editor_bg.wasm`. CI runs the build on every pull request and fails when a gzip size goes over its budget in phases.md. The module is 213,439 bytes gzip against a 1,500,000 budget. PR: [#76](https://github.com/HutsonLabs/excali-rust/pull/76).

## 2026-09-28 · ex-403 · Frames: clipping with radius 8/zoom, stroke #bbb, names as Helvetica text on export

The static scene now clips each frame's children to the frame. The clip is a rounded rectangle of radius 8 / zoom at the frame's corner, as upstream's `frameClip` and `clipElementToFrame` draw it. Each child's bound text, placeholder label and link icon are clipped with it. The `frame.ts` predicates that decide the target frame are ported and held to upstream's results, including while dragging over a highlighted frame and while editing a group. PNG export carries these through as well. PR: [#75](https://github.com/HutsonLabs/excali-rust/pull/75).

## 2026-09-28 · ex-m3 · Milestone check: M3 reached

The second M3 check ran on `main` at eec2b7b, with ex-301 to ex-308 and the gap tasks ex-g301 to ex-g303 merged. `cargo test -p excali-text --locked` passes 217 tests, including upstream's wrapping tests case for case, and the text goldens are up to date with upstream. The corpus width test now holds all six vendored families to 0.5 px. The 114 older stored widths outside that are pinned, and for 113 of them the test recomputes the stored width exactly from the older upstream measurement that wrote it. The remaining one is a Lilita One width kept from an earlier state of the element. Upstream's own code, run in Chromium, reproduces those causes. No gap tasks. PR: [#74](https://github.com/HutsonLabs/excali-rust/pull/74).

## 2026-09-28 · ex-405 · PNG export: padding 10, scale, background, embedded payload

Scenes now export to PNG the way upstream's `exportToCanvas` draws them: padding 10 (0 for an exported frame), canvas size times `exportScale`, the background drawn only when `exportBackground` is set, and the utils wrapper's `maxWidthOrHeight` and `getDimensions`. The PNG is 8-bit RGBA and holds the scene in `encodePngMetadata`'s tEXt chunk. Upstream's `loadFromBlob` reads the port's PNGs back into the same scenes. PR: [#73](https://github.com/HutsonLabs/excali-rust/pull/73).

## 2026-09-28 · ex-g303 · Show the cause of the 18 pinned Excalifont and Comic Shanns width deviations against upstream at the pin

Each of the 18 Excalifont and Comic Shanns texts whose stored width differs from the port now names what wrote it: an ink box of upstream's older `getLineWidth`, measured per glyph or at whole pixels. Five are reproduced exactly by upstream's code in Chrome, and Chrome shows restore keeps all 18 stored widths. A new macOS gate re-runs the measurement and checks it against the committed fixture within 0.001 px. PR: [#72](https://github.com/HutsonLabs/excali-rust/pull/72).

## 2026-09-28 · ex-404 · Image elements: decode data URLs (image crate), crop, scale flip, rounded clip, placeholder

Image elements now draw as they do in upstream's `renderElement`. This covers crops, flips, rounded clips, the dark-theme filter on SVG files, and the light, dark and error placeholders. The PNG backend decodes raster and SVG data URLs, and the browser canvas backend draws the same images. Built-in placeholders and link icons use one set of `excalidraw:` ids, and the static scene fills rectangles with `fillRect` wherever upstream does. PNG output matches Chrome 153 within each fixture's tolerance. PR: [#71](https://github.com/HutsonLabs/excali-rust/pull/71).

## 2026-09-28 · ex-408 · Spike: font subsetting for SVG export (allsorts, hb-subset, or ship full woff2)

SVG export now inlines each font face subset to the characters in the scene, as upstream's hb-subset path does, and falls back to the whole file only if subsetting fails. ADR-010 chose skera 0.7.0 with ttf2woff2 0.13.3 after measuring four subsetters against upstream's own subsets. skera keeps 1417/1417 code points with the same glyphs, its output is 97.6 % of upstream's bytes, 279/279 runs render pixel-identical in Chromium, and it builds for wasm32. PR: [#70](https://github.com/HutsonLabs/excali-rust/pull/70).

## 2026-09-28 · ex-g302 · Gate Cascadia and Lilita One in the whole-corpus width test

The corpus width test now also holds Cascadia and Lilita One to 0.5 px. Every Cascadia and Lilita One text whose stored width is a measurement passes, except one in each family, and both are pinned with a measured explanation. The Cascadia `{}` stores its width at `Math.round(fontSize)`, as every Cascadia text in its library does, and the test recomputes that exactly. The Lilita One `Metrics` stores 3.3345 em, where the face measures 3.304 em at every size. PR: [#69](https://github.com/HutsonLabs/excali-rust/pull/69).

## 2026-09-28 · ex-g301 · Gate Virgil in the whole-corpus width test: explain or fix the 94 measured Virgil texts outside 0.5 px

The corpus width test now gates Virgil along with Excalifont, Nunito and Comic Shanns. Each of the 94 Virgil texts outside 0.5 px is pinned to the older upstream measurement that wrote its stored width: DOM offsetWidth before and after the +1 fix, a width scaled from an earlier font size, a padded container width, or the 2024 ink-box getLineWidth. The test recomputes each of these from the vendored fonts and requires it to equal the stored width. The same models explain 13 of the 18 older Excalifont and Comic Shanns exceptions. PR: [#68](https://github.com/HutsonLabs/excali-rust/pull/68).

## 2026-09-28 · ex-402 · Static scene assembly: background, grid, element order, bound text after container, iframes last

`excali_scene::static_scene::render_static_scene` builds the static canvas the way upstream's `renderStaticScene` does, as a display list: the background with its white fallback, the device pixel ratio and zoom, the grid with its colours, dashes and 10 px zoom cutoff, elements with bound text after their container and link icons, iframes and embeddables last, and pending flowchart nodes. `render_element` draws every element kind except sticky notes. The display list matches upstream draw for draw across 34 scenes and 1,776 draws. PR: [#67](https://github.com/HutsonLabs/excali-rust/pull/67).

## 2026-09-28 · ex-m3 · Milestone check: M3 not yet reached

The M3 check ran on `main` at c720406, with ex-301 to ex-308 merged. `cargo test -p excali-text --locked` passes 205 tests. That covers `textWrapping.test.ts` ported case for case, the bound-text cases of `textElement.test.ts`, and the text goldens, which are up to date with upstream. The corpus gate passes, but it holds only Excalifont, Nunito and Comic Shanns to 0.5 px, and it needs 18 pinned exceptions to do so. Among the corpus texts whose stored width is a browser measurement, 94 of 1243 Virgil texts, 1 of 35 Cascadia texts and 1 of 3 Lilita One texts miss by more than 0.5 px, so M3's corpus criterion is not met. Three gap tasks are open: ex-g301 gates Virgil, ex-g302 gates Cascadia and Lilita One, and ex-g303 shows the cause of the pinned deviations. PR: [#66](https://github.com/HutsonLabs/excali-rust/pull/66).

## 2026-09-28 · ex-308 · Corpus test: stored vs measured text widths across fixtures and libraries

`excali-text` now measures every text of a vendored family across the whole corpus (232 libraries and the scene-bearing upstream fixtures, 6854 texts) and compares it with the width the browser stored. A per-family report of counts, max and mean deviation is committed and kept current by `cargo test`, and Excalifont, Nunito and Comic Shanns are gated at 0.5 px per text and per family mean, with 18 recorded deviations that may only shrink. PR: [#65](https://github.com/HutsonLabs/excali-rust/pull/65).

## 2026-09-28 · ex-305 · Text element sizing on edit: autoResize, originalText/text, anchor growth by align

`excali-text` sizes text while it is created and edited the way upstream's `newElement.ts` does. `new_text_element` positions a new box from its anchor for each alignment. `RefreshedText::apply` applies a `refreshTextDimensions` result as the user types, so the text grows and wraps like it does in the editor. `text_auto_resize` unwraps text around its anchor as `actionTextAutoResize` does. More than 2400 upstream cases match. PR: [#64](https://github.com/HutsonLabs/excali-rust/pull/64).

## 2026-09-28 · ex-304 · Bound-text sizing: padding 5, ellipse and diamond insets, arrow label width

`excali-text` sizes text bound to a container the way upstream's textElement.ts does. That covers the padding of 5, the insets for ellipses and diamonds, the maximum width and height of the text, the container size that fits a given text, and the text's position inside its container. `refreshTextDimensions` and `getAdjustedDimensions` are ported, so restoring a scene through `TextEnv` refits container text as upstream does. Arrow labels get their box through `ArrowLabelGeometry`, which excali-editor will implement under ex-511. PR: [#63](https://github.com/HutsonLabs/excali-rust/pull/63).

## 2026-09-28 · ex-406 · SVG writer: document structure (source comment, metadata, defs clipPaths, font style block, background rect)

`excali_svg::export_to_svg` writes the SVG document that upstream's `exportToSvg` builds before the elements. That covers the source comment, the embedded scene payload, a clip path per frame, frame name labels, the `@font-face` style block and the dark-mode background. The output is serialized the way `outerHTML` does, and path data uses two decimals. It matches upstream's output under jsdom for 29 scenes. When a font file is not available, a face falls back to upstream's esm.sh asset url under `dist/prod/fonts/`. PR: [#62](https://github.com/HutsonLabs/excali-rust/pull/62).

## 2026-09-28 · ex-m2 · Milestone check: M2 reached

The second M2 check ran on `main` at 0eb233c, after ex-g201 and ex-g202 merged. 1093 workspace tests pass with the goldens feature, the 105 golden generator tests pass and all sixteen goldens `--check` steps are current. The committed matrix `goldens/elements-matrix.json` covers 19 element variants at every fill style, roughness 0, 1 and 2 and seeds 1, 7 and 1041657908 (684 cases), the port matches all of them, looped freedraw fills included, and the `goldens` CI job fails on any missing cell. M2 is closed. PR: [#61](https://github.com/HutsonLabs/excali-rust/pull/61).

## 2026-09-28 · ex-401 · excali-raster: display list to tiny-skia (paths, fills, dashes, opacity, clips)

`excali-raster` draws display lists with tiny-skia and matches Chrome's canvas. The 19 fixture display lists are compared with references drawn by headless Chrome 153, each within its own tolerance; they cover fills, curves, arcs, dashes, opacity, clips, transforms, images and the device pixel ratio. Where tiny-skia's pixels differ from Chrome's, the crate uses ports of Skia m153 instead: analytic anti-aliasing, the stroker and dasher, and the hairline walk. Huge or non-finite coordinates draw what Chrome draws, and a new CI job checks the port against the runner's Chrome. PR: [#60](https://github.com/HutsonLabs/excali-rust/pull/60).

## 2026-09-28 · ex-g202 · M2 golden matrix: every element type x fill style x roughness 0/1/2 x seeds 1/7/1041657908 in the goldens and CI

`goldens/elements-matrix.json` records upstream's `generateElementShape` for 684 cases: 19 element variants, from sharp and rounded boxes to curved lines, arrows with a filled head, looped freedraw and the shapeless types, at hachure, cross-hatch, zigzag and solid fills, roughness 0, 1 and 2 and seeds 1, 7 and 1041657908. The port matches every case, and CI now asserts the matrix has no missing cell on both the Rust and node sides. PR: [#59](https://github.com/HutsonLabs/excali-rust/pull/59).

## 2026-09-28 · ex-303 · Wrapping port (textWrapping.ts) with upstream tests

`excali_text::text_wrapping` ports `textWrapping.ts`: `parse_tokens` splits lines with upstream's break regex evaluated in order (CJK, emoji, whitespace, hyphens), and `wrap_text` wraps them to a width with `wrap_line`, `wrap_word` and `trim_line`, using UTF-16 offsets like the browser. `textWrapping.test.ts` is ported case for case under a 10 px per character test metric, and the output matches upstream's `parseTokens`/`getWrappedTextLines` goldens. PR: [#58](https://github.com/HutsonLabs/excali-rust/pull/58).

## 2026-09-28 · ex-g201 · Freedraw background fill: port getFreedrawFillCurvePoints and the loop fill curve of generateElementShape

`excali_scene::generate_freedraw_shapes` follows upstream's freedraw case: when a stroke closes into a loop, it first draws a rough.js curve over the points simplified to 0.75 (`get_freedraw_fill_curve_points`) with the element's fill style and `stroke: "none"`, then the stroke path. All 48 freedraw goldens match upstream, including the 14 looped fills. PR: [#57](https://github.com/HutsonLabs/excali-rust/pull/57).

## 2026-09-28 · ex-302 · Advance-width measurement from font files (ttf-parser, rustybuzz where shaping matters)

`excali_text::text_measurements` ports `textMeasurements.ts`, and `FontStore` measures each line from the vendored font files. It picks a face per character through the fallback list the way the browser does and shapes each run with rustybuzz, kerning included. Each face is parsed once and its shaping plans are cached. 991 corpus texts measure within 0.5 px of their stored width. PR: [#56](https://github.com/HutsonLabs/excali-rust/pull/56).

## 2026-09-28 · ex-307 · Font asset pipeline: range-split woff2 manifest and lazy loading

Every upstream font file is vendored with its licence under `crates/excali-text/assets/fonts`, and `manifest.json` maps each family and unicode range to a file. `excali_text::font_assets` ports upstream's registry, unicode-range test, `containsCJK` and scene font selection. In the browser, `registerFonts` adds the faces without fetching, and `loadSceneFonts` fetches only the range files a scene's text needs, checked in Chromium on all 30 golden scenes. PR: [#55](https://github.com/HutsonLabs/excali-rust/pull/55).

## 2026-09-28 · ex-m2 · Milestone check: M2 not yet reached

The M2 check ran on `main` at f43521d, with every agent-doable phase 2 task merged. 887 workspace tests pass with the goldens feature, the 93 golden generator tests pass and all fourteen goldens `--check` steps are current. An uncommitted run of upstream's `generateElementShape` matched the port on all 432 cases of rectangle, diamond, ellipse, iframe, embeddable, line and arrow at every fill style, roughness 0, 1, 2 and seeds 1, 7, 1041657908. M2 is still not met. The committed goldens test fill styles only at roughness 1 and the fixture seed, so CI does not prove the full matrix. The check added ex-g202 to commit it with a coverage assertion. The rough.js background fill under a looped freedraw is not ported either, and the check added ex-g201 for it. PR: [#54](https://github.com/HutsonLabs/excali-rust/pull/54).

## 2026-09-28 · ex-211 · Elbow arrow routing: A* over the non-uniform grid

`excali-editor` now routes elbow arrows as upstream does. Its `updateElbowArrowPoints` runs A* over the non-uniform grid, with bend penalties, BASE_PADDING 40 and fixed segments. It matches upstream's routes on 574 fixture scenes generated from the pin, including the scenes where upstream throws. `RoutingEnv` answers restoreElements' re-route of an unbound elbow arrow. PR: [#53](https://github.com/HutsonLabs/excali-rust/pull/53).

## 2026-09-28 · ex-117 · Typed element model: keep field values of a type the model has no form for (string strokeWidth) as upstream's restore does

The typed element model now keeps a restored field value of a JSON type it has no form for, such as `strokeWidth: "3"`, and writes it back unchanged until the field is edited. Upstream's restore does the same. The 24 `aarondiel/logic-gates` elements that were dropped now round-trip. The typed view of such a value is not what upstream draws. The [file format](@/architecture/file-format.md) page records this, so a renderer must read the raw value. PR: [#52](https://github.com/HutsonLabs/excali-rust/pull/52).

## 2026-09-28 · ex-306 · Verify and record the licence of every font family before vendoring

ADR-004 records the licence of every upstream font family, checked at its source. Nine bundled families are under OFL 1.1 or MIT. Cascadia Code's name ID 13 is quoted in full. Upstream's Liberation Sans 1.05 file is recorded as a licence gap and mapped to the OFL 2.1.5 build, whose sha256 is pinned in a new Vendored builds table. `scripts/gates/fonts.py` enforces this in CI. Every vendored font must be in one of upstream's family directories and have a licence file next to it. Liberation files must match a recorded build. PR: [#51](https://github.com/HutsonLabs/excali-rust/pull/51).

## 2026-09-28 · ex-301 · Font metadata table and vertical offset formula

`excali_text::font_metadata` now has upstream's `FONT_METADATA` table, `get_vertical_offset`, `get_line_height`, `get_line_height_in_px`, `GOOGLE_FONTS_RANGES`, the fallback font names and the family fallback and font string helpers. Every font's metrics, line height and vertical offset match upstream bit for bit, checked against a golden fixture generated from the pinned upstream checkout. PR: [#50](https://github.com/HutsonLabs/excali-rust/pull/50).

## 2026-09-28 · ex-m1 · Milestone check (second run): M1 not yet reached

The M1 check ran again on `main` at 46d9d4e, after ex-g101 merged. 851 workspace tests pass, the fixture corpus verifies, and all twelve goldens checks are current, including the D1 document round trip. Every scene-bearing upstream fixture and all 232 catalogue libraries round-trip byte for byte against upstream. D1 is still not met because two known losses remain. The 24 logic-gates lines with a string `strokeWidth` are dropped (ex-117). 1245 legacy arrow bindings are cleared instead of migrated (ex-116, blocked by ex-507 and ex-510). The check found no other gap, so it added no new tasks. PR: [#49](https://github.com/HutsonLabs/excali-rust/pull/49).

## 2026-09-28 · ex-212 · Arrowheads: all fourteen kinds with sizes, angles and roughness rules

`excali_scene::bounds` now has `get_arrowhead_size`, `get_arrowhead_angle` and `get_arrowhead_points`, and `excali_scene::shape` has `get_arrowhead_shapes` for all fourteen kinds. `generate_linear_element_shapes` draws the arrow body and then its start and end heads. Line heads cap roughness at 1 and are drawn solid unless the arrow is dotted. Circle heads cap roughness at 0.5. Outline heads fill with the canvas background, dark-filtered on a dark canvas. A missing `endArrowhead` key still defaults to an arrow. All 102 shapes in `elements-arrow.json` and all 442 in `elements-arrowheads.json` match upstream. PR: [#48](https://github.com/HutsonLabs/excali-rust/pull/48).

## 2026-09-28 · ex-218 · Dark-mode colour filter maths (invert 93% hue-rotate 180deg) and reverse

`excali_core::color` now has `remove_dark_mode_filter`, a port of upstream's `removeDarkModeFilter`, next to the existing `apply_dark_mode_filter`. It also has a public `rgb_to_hex` that follows upstream's int32 arithmetic and optional alpha, and `COLOR_PALETTE` in upstream key order. `DARK_THEME_FILTER` is in `excali_core::constants`. For all 63 palette colours, apply, remove and the apply-remove-apply round trip all match upstream exactly. So do every CSS colour notation tested and all 256 greys. PR: [#47](https://github.com/HutsonLabs/excali-rust/pull/47).

## 2026-09-28 · ex-210 · Elbow arrow path from fixed points (radius 16) and validation

`excali_scene::shape` now draws an elbow arrow the way upstream's `_generateElementShape` does. It calls `generator.path` on `generateElbowArrowShape(points, 16)` with continuous options, and corners shrink on short segments. Empty points become `[0, 0]`, and nothing is drawn past the 1e6 coordinate guard. The `heading` helpers and `validate_elbow_points` (DEDUP_TRESHOLD 1, strict) are ported too, and all 9 elbow golden bodies match upstream op by op. PR: [#46](https://github.com/HutsonLabs/excali-rust/pull/46).

## 2026-09-28 · ex-g101 · D1 conformance: upstream diagramFixture document round-trips through Document and restore against an upstream golden

`excali_core::document::load_scene_json` and `LoadedScene::to_document` port upstream's `loadFromBlob` and `serializeAsJSON`. Every scene-bearing upstream test fixture now round-trips to upstream's bytes on the first write and on the reload. That covers `diagramFixture`, `elementFixture`, the `.excalidrawlib` files and the embedded-scene PNG and SVG files. Unknown keys survive, and `files` values that are not objects are indexed the way JS indexes them. One test fails if a new scene-bearing fixture has no case, and `document-fixtures.mjs --check` keeps the golden in step with upstream in CI. PR: [#45](https://github.com/HutsonLabs/excali-rust/pull/45).

## 2026-09-28 · ex-214 · excali-freehand: laser-pointer constant-width variant

`excali-freehand` now has upstream's vendored laser pointer (`LaserPointer`, `douglas_peucker`, `run_length`) and `constant_width_outline` (size strokeWidth * 1.4, simplify 0, pressure 1). All 61 `laser-pointer.json` goldens match. Constant-width freedraw elements go through it in `excali_scene::freedraw`, so all 48 `elements-freedraw.json` elements, both variable and constant width, now give upstream's SVG path byte for byte. PR: [#44](https://github.com/HutsonLabs/excali-rust/pull/44).

## 2026-09-28 · ex-215 · Outline to path string with quadratic midpoints and 2-decimal trimming

`excali_scene::freedraw` turns a freedraw outline into upstream's SVG path: `M p0 Q p_i mid(p_i, p_i+1) ... L p0 Z` with JS number formatting and the TO_FIXED_PRECISION trimming. All 48 recorded outlines in `elements-freedraw.json` give upstream's path byte for byte. `get_freedraw_outline_points` and `get_free_draw_svg_path` switch on stroke variability: the 32 variable-width elements match end to end, and constant width returns a typed error until ex-214 ports the laser-pointer outline. PR: [#43](https://github.com/HutsonLabs/excali-rust/pull/43).

## 2026-09-28 · ex-209 · Shape construction: line and arrow (sharp, curved, polygon), loop fill

`excali_scene::shape::generate_linear_shape` builds the body of a line or non-elbow arrow the way upstream's `_generateElementShape` does. A sharp element gives `linearPath`, or `polygon` when a closed loop is filled. A round element gives `curve`, and empty points become `[0, 0]`. Elbow arrows and non-linear elements return typed errors. The line goldens and arrow bodies match upstream op by op. PR: [#42](https://github.com/HutsonLabs/excali-rust/pull/42).

## 2026-09-28 · ex-m1 · Milestone check: M1 not yet reached

The M1 check ran on `main` at 1a79b96. All phase 1 tasks ex-101 to ex-115 are merged. 754 workspace tests pass, the fixture corpus verifies and every goldens check is current. All 232 catalogue libraries round-trip byte for byte against upstream, and the `restore.test.ts` cases are ported. D1 is not met because the round trip loses data that upstream keeps: 24 elements with a string `strokeWidth` (ex-117) and 1245 legacy arrow bindings (ex-116, blocked by ex-507 and ex-510). The check added ex-g101, a document-level round trip of upstream's `diagramFixture` plus one test that covers every upstream fixture. PR: [#41](https://github.com/HutsonLabs/excali-rust/pull/41).

## 2026-09-28 · ex-206 · Spike: evaluate the roughr crate (0.14.0) against the goldens

`tools/roughr-eval` runs roughr 0.14.0 against all 1346 rough.js goldens. It matches 101 of them at two decimals (7.5 %), or 501 (37.2 %) with rough.js's Park-Miller generator patched in. Each mismatch is attributed to one of ten named divergences, so ADR-003 is accepted as a port. A CI job keeps both reports and the ADR tables current. PR: [#40](https://github.com/HutsonLabs/excali-rust/pull/40).

## 2026-09-28 · ex-109 · Library import model: URL allow-list and #addLibrary token parsing

`excali_core::library_url` ports validateLibraryUrl and #addLibrary token parsing, including the legacy query form and the import steps. `excali_core::link` ports normalizeLink and toValidURL. Both parse through `excali_core::whatwg_url`, which reads URLs the way Node 26's `new URL` does. For hosts outside ASCII it uses a port of ada 4.0.0's IDNA that runs over ada's own tables. It matches Node on 127 host cases, on every code point and on 20,000 seeded random URLs, and a CI job checks the fixtures. PR: [#39](https://github.com/HutsonLabs/excali-rust/pull/39).

## 2026-09-28 · ex-205 · excali-rough: dashes, multi-stroke, curve fitting, preserveVertices

Excalidraw's stroke styles go through the rough.js port as upstream does: dashed [8, 8+sw] and dotted [1.5, 6+sw] dashes are carried for the renderer, disableMultiStroke keeps a single pass, and preserveVertices and curveFitting follow rough.js. `goldens/rough-strokes.json` checks 630 cases (solid, dashed and dotted strokes x width x roughness x seed x generator call) against upstream output, and each element type yields the same ops as upstream. PR: [#38](https://github.com/HutsonLabs/excali-rust/pull/38).

## 2026-09-28 · ex-216 · Display list type: renderer-independent paths, fills, dashes, images, text runs, clips, opacity

`excali_scene::display` describes a scene as a list of canvas draws (fills, strokes, images, text runs and groups with transform, opacity and clip), with canvas path, dash, colour and font rules and one `Painter` replay for every backend. RoughCanvas.draw now emits display items. `excali-raster` renders the list with tiny-skia, and `excali-canvas2d` paints it through web-sys. Tests check that neither backend knows about elements, and colour parsing matches Chrome 153 case by case. PR: [#37](https://github.com/HutsonLabs/excali-rust/pull/37).

## 2026-09-28 · ex-115 · JSON Schema generation for .excalidraw and .excalidrawlib (schemars)

`excali_core::schema` generates draft 2020-12 JSON Schemas for `.excalidraw` and `.excalidrawlib` from the Rust model. The site publishes them at `/schema/excalidraw.schema.json` and `/schema/excalidrawlib.schema.json`, and the file-format page links both. Required, optional and nullable keys follow upstream's `types.ts`. On 593 elements from upstream's restore output, the schema accepts exactly the elements the codec reads. All 232 catalogue libraries are valid, and a CI step fails if the published files drift from the model. PR: [#36](https://github.com/HutsonLabs/excali-rust/pull/36).

## 2026-09-28 · ex-114 · Corpus test: 232 catalogue libraries round-trip

All 232 catalogue libraries in the manifest are parsed, written and parsed again, and both writes match upstream's output and reload digests byte for byte. Legacy-binding libraries are compared on their without-geometry digests until ex-116 lands, and aarondiel/logic-gates waits on ex-117. A third load must leave the file unchanged. A checked-in loss report lists every legacy key dropped, binding cleared, id replaced and polygon added, and ties each one to its upstream rule. PR: [#35](https://github.com/HutsonLabs/excali-rust/pull/35).

## 2026-09-28 · ex-202 · excali-math: cubic curves, Catmull-Rom approximation, length (Legendre-Gauss N=24), closest point

`excali_math::curve` ports upstream's `curve.ts`: bezier evaluation, Newton line-segment intersection, closest parameter, point and distance, tangents, Catmull-Rom quadratic and cubic approximation, offset points, and Legendre-Gauss N=24 length, length at parameter and point at length. `curve.test.ts` is ported and 1013 upstream golden cases match at 1e-10, with the curveLength fixtures exact to the bit. PR: [#34](https://github.com/HutsonLabs/excali-rust/pull/34).

## 2026-09-28 · ex-217 · Golden harness in CI (cargo test feature `goldens`)

`excali_rough::goldens` compares drawables and element shapes against the upstream goldens op by op; a failing golden prints the case id, element id, shape, set, op index and kind, expected and actual numbers, the difference in ulps and the tolerance. It also checks every goldens file against the manifest's sha256 and case count, and a new CI job runs the goldens on arm64, the architecture they were generated on. PR: [#33](https://github.com/HutsonLabs/excali-rust/pull/33).

## 2026-09-28 · ex-108 · Library formats: v1 `library` and v2 `libraryItems`, restoreLibraryItems, merge

`excali_core::library` now ports upstream's library data handling: `parseLibraryJSON` for v1 `library` arrays and v2 `libraryItems`, `restoreLibraryItems` with per-item element restore, and a typed `LibraryItem` codec that keeps unknown keys. `serializeLibraryAsJSON`, `mergeLibraryItems` and `getLibraryItemsHash` are ported too, and all 232 catalogue libraries match upstream goldens. Upstream keeps a string `strokeWidth` that the typed model drops; that gap is tracked as ex-117. PR: [#32](https://github.com/HutsonLabs/excali-rust/pull/32).

## 2026-09-28 · ex-105 · Restore: scene-level repairs (ids, indices, frames, bound text, bindings, sticky notes)

`excali_core::restore::restore_elements` and `bump_element_versions` now port upstream's `restoreElements` and `bumpElementVersions`: invisible and unsupported elements dropped, duplicate ids replaced, fractional indices repaired, and with `repairBindings` the frame, bound-text, container, binding, sticky-note and elbow-arrow passes in upstream's order. All 117 scenes of the upstream-generated fixture match byte for byte; routing, text measurement and sticky label fit go through new `RestoreEnv` hooks. PR: [#31](https://github.com/HutsonLabs/excali-rust/pull/31).

## 2026-09-28 · ex-208 · Shape construction: rectangle (adaptive radius path), diamond (rounded C corners), ellipse (curveFitting 1), iframe defaults

`excali-scene` now builds the rough.js drawables for rectangles, iframes, embeddables, diamonds and ellipses as upstream's `_generateElementShape` does: the adaptive-radius Q path for rounded rectangles, rounded diamond corners as C curves from `getDiamondPoints`, ellipses with curveFitting 1, and the `modifyIframeLikeForRoughOptions` defaults. `getCornerRadius` matches upstream's table (proportional 0.25, adaptive 32 with cutoff), and the element goldens for all five types pass. PR: [#30](https://github.com/HutsonLabs/excali-rust/pull/30).

## 2026-09-28 · ex-213 · excali-freehand: perfect-freehand 1.2.0 port (variable width)

`excali-freehand` now ports perfect-freehand 1.2.0 statement by statement: stroke points, outline points, `getStroke` and the stroke radius, plus upstream's `getVariableWidthFreedrawOutline` preset (size strokeWidth*4.25, thinning 0.6, smoothing 0.5, sine ease-out). Stroke points match all 51 freehand goldens exactly, including 22 new edge cases for tapers, caps, pressures and reversals, and the outlines match all 36 variable-width freedraw elements. PR: [#29](https://github.com/HutsonLabs/excali-rust/pull/29).

## 2026-09-28 · ex-204 · excali-rough: fill styles hachure, cross-hatch, zigzag, solid

`excali-rough` now draws the rough.js fill styles: solid, hachure, cross-hatch, zigzag, dashed, zigzag-line and dots. Every generator shape uses the hachure-fill 0.5.2 scan lines. Upstream rotates each polygon in place, so a repeated vertex turns once for each time it appears, and the port does the same. Solid, hachure, cross-hatch and zigzag fills match the upstream goldens bit for bit, with fillWeight = strokeWidth/2 and hachureGap = strokeWidth*4. PR: [#28](https://github.com/HutsonLabs/excali-rust/pull/28).

## 2026-09-28 · ex-207 · Option mapping: generateRoughOptions and adjustRoughness with all constants

`excali-scene` now maps an element to rough.js options as upstream's `generateRoughOptions` and `adjustRoughness` do. That covers seeds, stroke dash and dot patterns, fill styles, the roughness reduction for small shapes and the keep-roughness cases, `isPathALoop` fills for closed lines, and dark-mode colours through `applyDarkModeFilter` in `excali-core`. The port matches goldens generated from upstream's own functions, and CI checks those goldens. PR: [#27](https://github.com/HutsonLabs/excali-rust/pull/27).

## 2026-09-28 · ex-104 · Restore: per-type rules (text, freedraw, image, line/draw, arrow, stickynote, frame)

`excali-core` now restores each element type as upstream's `restoreElement` does. That covers the legacy font string and line height detection, freedraw points and stroke options, image defaults, draw to line, arrowhead renames, point re-basing, the 75000 px cap, binding repair, the fixedSegments rule, sticky notes and frame names. The port matches a fixture table generated from upstream's own `restoreElement` and the `restore.test.ts` cases. The legacy binding migration is tracked separately as ex-116. PR: [#26](https://github.com/HutsonLabs/excali-rust/pull/26).

## 2026-09-28 · ex-113 · Clipboard JSON format (excalidraw/clipboard) parse and emit

`excali-core` now writes and reads upstream's clipboard JSON (`serializeAsClipboardJSON`, `parseClipboard`): compact output with only the copied images' files, orphaned frame children detached through a version bump with `shape` and `canvas` dropped, and paste of the `excalidraw`, `excalidraw/clipboard` and `excalidraw-api/clipboard` types with everything else returned as text. Number literals beyond the f64 range read as `null`, as `JSON.stringify(JSON.parse(...))` gives. All 26 copy cases match upstream byte for byte and all 78 paste cases match too. PR: [#25](https://github.com/HutsonLabs/excali-rust/pull/25).

## 2026-09-28 · ex-111 · PNG tEXt scene payload read/write

`excali-core` now writes and reads the scene payload that upstream embeds in a PNG `tEXt` chunk (`encodePngMetadata`, `decodePngMetadata`, `getTEXtChunk`), with ports of png-chunks-extract, png-chunks-encode and png-chunk-text. `test_embedded_v1.png` and `smiley_embedded_v2.png` decode to their elements, written bytes equal upstream's byte for byte, truncated and corrupted chunks fail as upstream does, and upstream decodes every PNG the port writes. PR: [#24](https://github.com/HutsonLabs/excali-rust/pull/24).

## 2026-09-28 · ex-112 · SVG metadata scene payload read/write

`excali-core` now writes and reads the scene payload that upstream embeds in exported SVG metadata (`encodeSvgBase64Payload`, `decodeSvgBase64Payload`), with byte-exact `btoa` and forgiving-base64 `atob`. The v1 and v2 fixture SVGs decode to their scenes, and re-encoding the payloads in upstream's export snapshots reproduces them byte for byte, pako-compressed base64 included. INVALID and FAILED cases match upstream's blob loader. PR: [#23](https://github.com/HutsonLabs/excali-rust/pull/23).

## 2026-09-28 · ex-106 · AppState: exported keys, defaults and restoreAppState legacy handling

`excali-core` now builds upstream's default AppState and cleans it for browser, export and server storage the way `APP_STATE_STORAGE_CONF` does. Only gridSize, gridStep, gridModeEnabled, viewBackgroundColor and lockedMultiSelections are exported. `restoreAppState` migrates a numeric `zoom` and a string `openSidebar`, and tinycolor2's parser backs `colorToHex` and `isTransparent`. All of it matches goldens generated from the pinned upstream. Restore now also throws where upstream's `getNormalizedDimensions` does, for sizes that are objects with their own `toString` key. PR: [#22](https://github.com/HutsonLabs/excali-rust/pull/22).

## 2026-09-28 · ex-203 · excali-rough: Park-Miller RNG and core generator (line, rectangle, polygon, ellipse, curve, path)

`excali-rough` now ports rough.js 4.6.4: the Park-Miller `Random`, the generator strokes, the renderer primitives, path-data-parser, points-on-curve and points-on-path. Random draws happen in the same order as in rough.js. OpSets equal goldens generated from the pinned roughjs at seeds 1, 7 and 1041657908 at roughness 0, 1 and 2 (228 generator cases). Bezier flattening and simplify never overflow the stack. PR: [#21](https://github.com/HutsonLabs/excali-rust/pull/21).

## 2026-09-28 · ex-103 · Restore: base normalisation rules

`excali-core` now restores the base fields of any element the way upstream's `restoreElementWithProperties` does: JS `||` and `??` defaults, legacy `strokeSharpness` to `roundness` by type, `boundElementIds` to `boundElements`, links sanitised through a port of `@braintree/sanitize-url` 6.0.2, negative sizes flipped, and upstream's object-spread key order. All 311 cases generated from the pinned upstream match byte for byte (`tools/goldens/restore-fixtures.mjs --check`). PR: [#20](https://github.com/HutsonLabs/excali-rust/pull/20).

## 2026-09-28 · ex-107 · Fractional indexing port (base-62 keys, generateNKeysBetween, syncInvalidIndices)

`excali-core` now generates and validates fractional index keys the way upstream's vendored fractional-indexing does, and ports `fractionalIndex.ts`: `syncInvalidIndices`, `syncMovedIndices`, `syncInvalidIndicesImmutable` (returning upstream's id map, duplicate ids included), `validateFractionalIndices` and `orderByFractionalIndex`, which sorts with the V8 TimSort port so unindexed elements and duplicate ids land in upstream's order. All of it is checked against goldens generated from the pinned upstream. PR: [#19](https://github.com/HutsonLabs/excali-rust/pull/19).

## 2026-09-28 · ex-110 · Payload codec: byte-string encoding and zlib compression (encode/decode)

`excali-core` now encodes and decodes the `{ version, encoding, compressed, encoded }` payload wrapper the way upstream `data/encode.ts` does. pako 2.0.3's deflate and inflate are ported in full, so compressed output is byte-identical to pako and every corrupt stream fails with pako's own message (or succeeds with pako's output), checked against goldens generated from upstream and a 1,600-case corrupted-stream differential. PR: [#18](https://github.com/HutsonLabs/excali-rust/pull/18).

## 2026-09-28 · ex-102 · Serde with unknown-field preservation and 2-space JSON output

`excali-core` now reads and writes `.excalidraw` scenes the way upstream does. Unknown keys at every level (the top level, elements, appState, files and customData) survive a round trip in the order they were read, and unchanged raw values are kept byte for byte, including lone-surrogate escapes. Output matches `JSON.stringify(data, null, 2)`: 2-space indentation, JS property order with array-index keys first, and upstream's constructor key order for elements built in Rust. This is checked against node-generated fixtures (`tools/goldens/scene-fixtures.mjs --check`). PR: [#17](https://github.com/HutsonLabs/excali-rust/pull/17).

## 2026-09-28 · ex-201 · excali-math: points, vectors, segments, angles, ranges, rectangles, polygons

The `excali-math` crate now covers `packages/math/src` function for function, except `curve.ts` and `pca.ts` (ex-202): points, vectors, segments, lines, angles, ranges, rectangles, triangles, ellipses and polygons, with the upstream math tests ported and parity against `goldens/math.json`. JS number semantics are kept, including a line-by-line port of V8's TimSort, so `convexHull` on points with NaN or infinite coordinates matches upstream bit for bit (checked against `goldens/js-sort.json`, generated from real V8). PR: [#16](https://github.com/HutsonLabs/excali-rust/pull/16).

## 2026-09-28 · ex-m0 · Milestone check: M0 reached

The M0 acceptance check was rerun end to end on deddc2d and all four criteria pass. **Pages:** the pages workflow for deddc2d built, deployed and passed its post-deploy smoke ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405351)), and the site returns HTTP 200. **Bootstrap:** `scripts/bootstrap.sh` under `/bin/bash` 3.2 in a fresh clone on macOS arm64 completed, and the `bootstrap-and-site` job passed on ubuntu and macOS. **Attribution:** the `attribution` job of the gates run for deddc2d ran the 11 planted-violation cases of `scripts/gates/test_attribution.py`, and every one was rejected with exit 1 while the clean control passed ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405346)). **Workspace:** fmt, clippy `-D warnings` and `cargo test --workspace --locked` pass (50 tests, 0 failed, 0 ignored), and so do the workspace, version and wasm32 gates ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405357)). The Playwright smoke suite (140), the corpus checks and the goldens `--check` are green. No gap tasks. PR: see the ex-m0 milestone PR.

## 2026-09-28 · ex-g001 · Attribution gate self-test in CI: a planted attribution line must fail the gate

`scripts/gates/test_attribution.py` plants violations one at a time in a scratch git repository outside this one (a tool-attribution line, a Co-Authored-By trailer, zero-width characters in a file and in a message, a non-human author and committer, and a violation below a clean tip) and asserts the attribution gate exits 1 with the matching rule id, while a clean control exits 0. The `attribution` job in the gates workflow runs it on every PR and on `main`; the first run was green ([gates run](https://github.com/HutsonLabs/excali-rust/actions/runs/36387970984)). PR: [#14](https://github.com/HutsonLabs/excali-rust/pull/14).

## 2026-09-28 · ex-m0 · Milestone check: M0 not yet reached

The M0 acceptance check ran end to end on 25602c2. **Pages:** the pages workflow for 25602c2 succeeded, and the github-pages deployment for that SHA serves the home, phases, progress and architecture pages with HTTP 200. **Bootstrap:** `scripts/bootstrap.sh` under `/bin/bash` 3.2 in a fresh clone (macOS arm64) installed hooks, seeded the tracker, downloaded Zola with its SHA-256 verified, checked out upstream at 438d898 and passed the gate self-check. **Workspace:** fmt, clippy `-D warnings` and `cargo test --workspace --locked` pass (50 tests, 0 failed, 0 ignored), and so do the workspace, version and wasm32 gates. The Playwright smoke suite (140), the plan and script tests, the corpus check, `verify-upstream` and the goldens `--check` are also green, and every CI workflow on main is green. **Attribution:** `attribution.py` rejects a planted line locally. On a tracked file it exits 1 (R2, R4), and on a commit with a Co-Authored-By trailer it exits 1 (R1, R3, R4). The commit-msg hook strips such a trailer. But no CI job plants a violation, so the requirement that CI rejects a planted line is not shown. A partial pass is a fail. Gap task `ex-g001` adds a gate self-test to the gates workflow; M0 closes once it is green. PR: see the ex-m0 milestone PR.

## 2026-09-28 · ex-008 · Record the owner decisions of 2026-09-27 (calendar versioning, agent-closed milestones, fonts, strictly a port)

The workspace is versioned 26.9.1 under ADR-009 (calendar YY.M.BUILD; GitHub releases only for now), and a version gate in CI checks the format, workspace inheritance, publish = false and Cargo.lock. Agents now close milestones on green CI with evidence posted and merge term.hut PRs. ADR-004 maps font families without a confirmed licence to licensed fallbacks. The upstream checkout's push URL is disabled because this is strictly a port. Phase 8 now ends with v26.9.1: ex-801 is deferred and ex-804 tags the GitHub release. PR: [#12](https://github.com/HutsonLabs/excali-rust/pull/12).

## 2026-09-28 · ex-101 · Element model: enums, structs and shared base fields

`excali-core` now has the full Excalidraw element model: `ElementBase` with every `_ExcalidrawElementBase` field under upstream's JSON names, and `ElementKind` tagged on `type` for all 14 element types with their per-type fields. Supporting enums (fill and stroke styles, roundness, font families, alignment, arrowheads with legacy names, bindings, fixed segments, crop), type groupings, arrow subtypes and upstream's construction defaults and constants are in place, pinned by 29 tests. PR: [#11](https://github.com/HutsonLabs/excali-rust/pull/11).

## 2026-09-28 · ex-004 · Golden generator: node script producing rough.js 4.6.4 path output for fixture elements

`tools/goldens/generate.mjs` runs upstream's own `ShapeCache.generateElementShape` from the pinned checkout under plain Node, with roughjs 4.6.4 and perfect-freehand 1.2.0 pinned to upstream's `yarn.lock` hashes. It writes 16 byte-stable files to `goldens/` (465 element shapes, raw rough.js primitives and fills, `Random.next` sequences, freehand strokes) with a sha256 manifest, and the goldens reproduce upstream's export snapshot paths exactly. The new `goldens` CI job runs the 28 tests and `generate.mjs --check` on every PR. PR: [#10](https://github.com/HutsonLabs/excali-rust/pull/10).

## 2026-09-28 · ex-006 · Playwright smoke test for the site and mockups

Every content page, every mockup and the 404 page now load in Chromium at 1440x900, 1024x768 and 390x844 from a local build served like GitHub Pages. Any console error, uncaught exception, failed request or HTTP error fails the run, and inventory checks keep the tested pages in step with the sitemap and the mockups index. The new `site-smoke` CI job runs the unit tests and the 137-test smoke suite on every PR (`cd tests/site && npm ci && npm run test:smoke` locally). PR: [#8](https://github.com/HutsonLabs/excali-rust/pull/8).

## 2026-09-28 · ex-003 · Fixture corpus with manifest (upstream test fixtures + 232 public libraries)

`fixtures/` now holds 249 pinned files. The 15 upstream files are byte-exact copies at 438d898: the test fixtures, the restore and reconcile tests, the restore snapshot, and the upstream LICENSE. The other 234 come from excalidraw-libraries at 297a349: `libraries.json`, its LICENSE, and all 232 catalogue libraries, stored gzipped. `fixtures/manifest.json` records each file's sha256, size and origin URL. `scripts/fixtures/corpus.py check` confirms the manifest matches disk, and the new `fixtures` CI job also checks the copies against the pinned upstream checkout and the live origin URLs. PR: [#7](https://github.com/HutsonLabs/excali-rust/pull/7).

## 2026-09-28 · ex-001 · Cargo workspace skeleton and CI (fmt, clippy -D warnings, test)

The Cargo workspace now holds the 14 crates named in the architecture overview, with a pinned stable toolchain and a recorded MSRV. `excali-core` writes JSON exactly as `JSON.stringify` does (numbers, lone surrogates, key order), and a round-trip test covers an empty scene. The new `rust` CI workflow runs fmt, clippy with `-D warnings`, the tests, the MSRV check, a wasm32 build, and a crate-graph gate. The gate checks the workspace against the overview page and rejects `std::fs` in the wasm-pure crates. PR: [#6](https://github.com/HutsonLabs/excali-rust/pull/6).

## 2026-09-28 · ex-007 · scripts/site/zola.sh works on macOS (bash 3.2, shasum) with the aarch64-apple-darwin digest pinned

`scripts/site/zola.sh` now runs under the `/bin/bash` 3.2 that ships with macOS and checks downloads with `sha256sum` or `shasum -a 256`. The v0.22.0 digests for all four unix targets are pinned. A download with no pinned digest, or with a digest that does not match, is refused and nothing is installed. The new `bootstrap-and-site` CI job runs the offline tests, a real SHA-verified download, `bootstrap.sh` and the site build under `/bin/bash` on both ubuntu and macos. PR: [#5](https://github.com/HutsonLabs/excali-rust/pull/5).

## 2026-09-27 · ex-002 · Upstream pin script: check out excalidraw at the pinned commit into .tools/upstream

`scripts/upstream/checkout.sh` checks out excalidraw at the commit pinned in `site/config.toml` (438d898). The checkout goes into the main clone's `.tools/upstream`, which every worktree shares. It verifies HEAD and is idempotent. It refuses dirty or foreign checkouts, and it will not use any other commit unless `PIN` is set explicitly. `bootstrap.sh` runs it, and the `upstream-pin` CI job runs both the offline test suite and a live checkout. PR: [#4](https://github.com/HutsonLabs/excali-rust/pull/4).

## 2026-09-27 · ex-005 · Enable GitHub Pages source = GitHub Actions and confirm first deploy

GitHub Pages now deploys from GitHub Actions, and the site is live at its configured `base_url`. A new `smoke` job runs after each deploy to main. It fails the workflow unless the home page returns HTTP 200 with the expected title. `scripts/site/zola.sh` now also works with the bash 3.2 that ships with macOS. PR: [#3](https://github.com/HutsonLabs/excali-rust/pull/3).
