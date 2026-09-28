+++
title = "ADR-010: SVG export subsets fonts with skera and ttf2woff2"
description = "Upstream inlines each font face subset to the scene's characters. The port does the same in Rust: skera subsets with upstream's HarfBuzz input and ttf2woff2 writes WOFF2. Chosen by size and fidelity numbers against upstream's own subsets."
weight = 10
+++

**Status.** Accepted. Decided by task `ex-408` (2026-09-28) from the numbers in `tools/font-subset-eval`. `excali_svg::FontFiles` subsets every face it inlines (`excali_svg::subset_woff2`).

## Question

Upstream's SVG export inlines every font face a scene uses as a `data:font/woff2;base64,…` URL, subset to the characters of that font family in the scene. Until this decision the port inlined the whole vendored file. Should the port subset too, and with what: `allsorts`, HarfBuzz's `hb-subset` through Rust bindings, another Rust subsetter, or no subsetting at all?

## Evidence

- **Upstream's pipeline.** `Fonts.generateFontFaceDeclarations` (`packages/excalidraw/fonts/Fonts.ts:182-217`) calls `ExcalidrawFontFace.toCSS(characters)` for every face whose unicode range holds one of the family's characters. `toCSS` then calls `getContent` (`fonts/ExcalidrawFontFace.ts:37-86`), which fetches the face and calls `subsetWoff2GlyphsByCodepoints`. That runs in a worker when one exists and on the main thread otherwise (`subset/subset-main.ts`).
- **The subset itself.** `subsetToBinary` (`subset/subset-shared.chunk.ts:44-57`) decompresses the woff2, subsets it with harfbuzzjs 0.3.6's `hb_subset_or_fail` and compresses the result back to woff2. The subset input is HarfBuzz's default with the requested unicodes and every layout feature kept, "the equivalent of --font-features=*" (`subset/harfbuzz/harfbuzz-bindings.ts:74-81, 116-120`). On any error `subsetToBase64` inlines the whole file (`:25-39`).
- **Upstream's own wasm.** harfbuzzjs and woff2 are inlined as base64 in `subset/harfbuzz/harfbuzz-wasm.ts` and `subset/woff2/woff2-wasm.ts`. They decode to 590,665 bytes (218,649 gzipped at level 9) and 727,190 bytes (332,912 gzipped) (`upstream-subsets.json` `wasm`).
- **Upstream's subsets.** `tools/goldens/font-subset.mjs` runs upstream's code from the pinned checkout. It replaces only `fetchFont`, reading the file from disk as upstream's `setupTests.ts:101-125` does, and records every `@font-face` rule of 46 scenes in `tools/font-subset-eval/upstream-subsets.json`: the face's file, the code points and upstream's woff2. The scenes are upstream's export test, the 30 font asset scenes of `font-assets.mjs` (every family, range and fallback case), and drawings of ordinary size: diagram labels, printable ASCII in every family upstream inlines, Latin-1, and a paragraph of Chinese, Japanese and Korean. For the export test the generator uses the FontFace of upstream's test setup (every face `U+0000-00FF`, `setupTests.ts:65-86`), and its 32 rules equal `tests/scene/__snapshots__/export.test.ts.snap` byte for byte (`tools/goldens/test/font-subset.test.mjs`). Assistant (id 10) is not in `Fonts.init`'s registry, so upstream never inlines it.
- **Where excali-svg must build.** `excali-svg` targets native and wasm32 ([architecture overview](../../architecture/overview/)); `scripts/gates/wasm-build.sh` builds it for `wasm32-unknown-unknown` in CI.
- **Crates, from crates.io on 2026-09-28.**
  - `allsorts` 0.17.0 (YesLogic, Apache-2.0). Its `subset_ttf` writes cmap, cvt, fpgm, hhea, hmtx, maxp, name, post, prep, OS/2, head, glyf and loca, and no GSUB, GPOS or GDEF.
  - `hb-subset` 0.3.0 (MIT), bindings to a bundled HarfBuzz 8.2.2 built with `cc` and `bindgen`.
  - `skera` 0.7.0 (fontations, MIT OR Apache-2.0), formerly klippa. It is Google's port of hb-subset to Rust and takes the same plan: unicodes, glyphs, drop tables, scripts, features, name ids and languages.
  - `subsetter` 0.2.6 (typst). Its documentation says the subsets are "most likely unusable in any other contexts than PDF writing" and "we remove the `cmap` table".
  - WOFF2 encoders: `woofwoof` 1.0.2 (Google's woff2 C++ over the Rust brotli crate) and `ttf2woff2` 0.13.3 (Rust).

## Options

1. Ship whole files (what `FontFiles` did).
2. `hb-subset`: the library upstream runs, from Rust.
3. `skera`: hb-subset ported to Rust.
4. `allsorts`.
5. `subsetter`.

Each subsetter is paired with each encoder.

## Rule

Set before the numbers were read, and applied by `tools/font-subset-eval/src/decision.rs`. The port subsets with a candidate when it meets all three conditions:

1. **Faithful on every face of `upstream-subsets.json`.** Nothing fails. Every code point the original face maps is kept, with the original's outline and advance. unitsPerEm and the hhea, OS/2 typo and win metrics equal the original's. Every run of the scene's text in the face shapes, under rustybuzz, to the same drawn glyphs (outline, advance and offsets, in order).
2. **Faithful in Chromium.** The subset loads wherever upstream's own subset loads (the CSS Font Loading API runs Chromium's font sanitizer, as an SVG's `@font-face` does), and every run drawn at 64 px is pixel for pixel the original's.
3. **Builds for `wasm32-unknown-unknown`**, with its subsetter and its encoder each and together.

Of the candidates that pass, the one that inlines the fewest bytes wins. If none passes, whole files stay.

## Result

**Decision: `skera+ttf2woff2`**

`tools/font-subset-eval` (a stand-alone package, not a workspace member) makes every rule of `upstream-subsets.json` again from the port's vendored face with each candidate. It calls each one with upstream's input (the rule's code points, every layout feature, HarfBuzz's defaults otherwise; allsorts and subsetter take the cmap's glyphs instead, and allsorts writes a Unicode cmap). Encoding is at brotli quality 11 with the glyf/loca transform. The run writes `report.json`. `browser/render.mjs` writes `browser.json` from Chromium 153. `wasm.sh` writes `wasm.json`. The tests fail if `report.json` is not a fresh run, or if this page does not quote the tables below and the decision the rule gives.

Upstream is measured the same way against its own original face. Its 153 subsets are faithful by every measure, so the measure asks for exactly what upstream delivers. "bytes" is the font bytes inlined over all 46 scenes. A failed subset counts the whole file, as upstream would inline it.

| candidate | bytes | % of upstream | faces faithful | code points kept | glyphs equal | runs shaped alike | Chromium: loaded, runs pixel-equal | wasm32 bytes (raw / gzip) |
|---|---|---|---|---|---|---|---|---|
| upstream (harfbuzzjs 0.3.6) | 329,896 | 100.0 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | 118/121 loaded, 279/279 | (its own wasm, below) |
| `whole` | 6,824,208 | 2068.6 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | (the original) | nothing to build |
| `hb-subset+woofwoof` | 322,760 | 97.8 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | not run | does not build (hb-subset, woofwoof) |
| `hb-subset+ttf2woff2` | 322,580 | 97.8 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | 118/121 loaded, 279/279 | does not build (hb-subset) |
| `skera+woofwoof` | 321,828 | 97.6 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | not run | does not build (woofwoof) |
| `skera+ttf2woff2` | 321,964 | 97.6 | 153/153 | 1417/1417 | 1417/1417 | 324/324 | 118/121 loaded, 279/279 | 2,374,997 / 788,843 |
| `allsorts+woofwoof` | 302,140 | 91.6 | 131/153 | 1417/1417 | 1417/1417 | 292/324 | not run | does not build (woofwoof) |
| `allsorts+ttf2woff2` | 302,104 | 91.6 | 131/153 | 1417/1417 | 1417/1417 | 292/324 | 118/121 loaded, 251/279 | 1,993,537 / 861,398 |
| `subsetter+woofwoof` | 281,780 | 85.4 | 0/153 | 0/1417 | 0/1417 | 0/324 | not run | does not build (woofwoof) |
| `subsetter+ttf2woff2` | 281,756 | 85.4 | 0/153 | 0/1417 | 0/1417 | 0/324 | 0/121 loaded, 0/0 | 2,516,526 / 842,860 |

The Chromium check covers the 121 faces of the 44 browser scenes, with each subsetter paired with `ttf2woff2`. Both encoders decode to the same font, and `woofwoof` cannot be used in wasm anyway. Upstream's subset and every candidate that keeps the cmap are rejected by Chromium's sanitizer for the same 3 faces. Each of those faces was asked only for characters it does not draw, and its subset holds just `.notdef` and the space (`browser.json` `rejected`).

Per scene, in bytes:

| scene | faces | upstream | whole files | `skera+ttf2woff2` |
|---|---|---|---|---|
| `fixture-default` | 2 | 3,632 | 41,432 | 3,672 |
| `labels-excalifont` | 1 | 8,484 | 24,956 | 8,508 |
| `ascii-excalifont` | 1 | 15,324 | 24,956 | 15,348 |
| `ascii-virgil` | 1 | 14,304 | 56,156 | 14,304 |
| `ascii-cascadia` | 1 | 23,660 | 65,732 | 23,624 |
| `ascii-nunito` | 1 | 8,820 | 16,476 | 8,800 |
| `ascii-lilita` | 1 | 6,108 | 10,676 | 6,068 |
| `ascii-comic-shanns` | 1 | 12,580 | 17,488 | 12,588 |
| `ascii-liberation` | 1 | 21,292 | 410,712 | 13,924 |
| `latin1-excalifont` | 1 | 9,732 | 24,956 | 9,740 |
| `cjk-paragraph` | 32 | 50,980 | 1,760,216 | 50,916 |

What the numbers say:

- **Whole files** draw exactly what upstream draws, but in 20.7 times the bytes. A drawing with any Liberation Sans text carries the 410,712-byte TrueType file: 547,637 characters of data URL, where upstream inlines 21,292 bytes. Liberation's row compares different sources, the port's 2.1.5 file against upstream's 1.05 ([ADR-004](../adr-004-fonts/)). A paragraph of CJK text carries 1.76 MB of Xiaolai range files.
- **hb-subset** is upstream's own engine. It is as faithful as upstream, and after decoding, 106 of its 153 subsets are the very sfnt upstream inlines. But the bundled HarfBuzz is C++, and `wasm32-unknown-unknown` has no C++ standard library (`'cassert' file not found`). `woofwoof` fails the same way (`'cstdint' file not found`). Neither can go into `excali-svg`.
- **skera** is as faithful as upstream on every measure: 1417 of 1417 code points, 324 of 324 shaped runs, and 279 of 279 runs pixel-identical in Chromium. It is 97.6 % of upstream's bytes (2.4 % smaller; within 1.1 % on every quoted scene but Liberation, whose source differs). It builds for wasm32. Its subsets are not upstream's bytes: 7 of 153 decode to upstream's sfnt, and `ttf2woff2` never writes upstream's woff2 bytes (`woff2Identical` 0 for every subsetter). So the port's SVG draws what upstream's draws, but its font data is not byte-for-byte upstream's.
- **allsorts** writes no GSUB, GPOS or GDEF. Every glyph is kept, but kerning and contextual forms are lost. 32 of 324 runs shape differently, and 28 of 279 runs draw differently in Chromium: for example "API Gateway" in Excalifont, and "Web app" and "Cache (Redis)" in Nunito. With its default cmap target it writes only a Mac Roman cmap when every glyph fits, which "browsers reject" (allsorts' own `CmapTarget` documentation). The evaluation asks for a Unicode cmap.
- **subsetter** removes the cmap, as its documentation says. Chromium rejects all 121 of its subsets.
- **wasm size.** `skera` and `ttf2woff2` together add 2,355,500 bytes (781,164 gzipped) over the probe alone (19,497 / 7,679), against upstream's 1,317,855 (551,561 gzipped) of harfbuzzjs and woff2 wasm. For scale, `excali-wasm` today is 1,432,165 bytes in release. It does not depend on `excali-svg` yet, so nothing it ships grows now.

## Decision

The port subsets like upstream. `excali_svg::subset_woff2` decodes the face (wuff, as excali-text does), subsets it with `skera` 0.7.0 using upstream's input (the characters' code points, every layout feature, and hb-subset's default drop tables, name ids 0 to 6 in English, and all scripts), and encodes WOFF2 with `ttf2woff2` 0.13.3 at brotli quality 11. `excali_svg::FontFiles` inlines that as `data:font/woff2;base64,…`. If subsetting fails it inlines the whole file with its own type (`font/woff2`, or `font/ttf` for Liberation Sans), as `subsetToBase64` does. If the file cannot be read it writes upstream's asset URL, as before. `subset_woff2` returns an error rather than letting skera panic on a font with no `maxp`, or with `loca` but no `glyf`.

`crates/excali-svg/tests/writer.rs` checks the behaviour against upstream's own output: for upstream's export test faces, labels, and the Comic Shanns, Cascadia, Lilita and Virgil scenes, the port's inlined face draws every code point with the glyph (advance and outline) of upstream's inlined face.

## Consequences

- An exported SVG is about the size of upstream's, not 20 times it, and every character draws as in upstream's export. SVG font data is the one part of the document that is not byte for byte upstream's. The [rendering fidelity page](../../architecture/rendering-fidelity/) records it.
- When `excali-wasm` gains SVG export, the module grows by about 2.4 MB (0.8 MB gzipped), 1.8 times upstream's lazily loaded subsetting wasm. Loading that part lazily is a packaging question for that task, not a reason to ship whole files.

## What would reverse it

- A wasm32 C++ toolchain (a WASI sysroot) in the build, which would let upstream's own HarfBuzz in and bring the sfnt bytes closer to upstream's.
- A skera release that changes what is drawn. The evaluation pins skera `=0.7.0`. Upgrading means running `tools/font-subset-eval` again, and its tests fail until this page quotes the new numbers.
