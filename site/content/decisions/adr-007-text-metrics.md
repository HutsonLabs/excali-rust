+++
title = "ADR-007: Trust stored text metrics, re-measure with the same fonts"
description = "How the port avoids re-flowing every text element differently from the browser that wrote the file."
weight = 7
+++

**Status.** Accepted, 2026-09-28.

## Evidence

- Text `width`, `height` and the wrapped `text` are saved values computed by `canvas.measureText` in the writing browser; height is `fontSize × lineHeight × lines` (`textMeasurements.ts:91-96, 121-168`).
- Upstream exposes `setCustomTextMetricsProvider` and uses a 10 px-per-character metric in its own tests (`textMeasurements.ts:113-119, 144-146`).
- Font metrics (unitsPerEm, ascender, descender, lineHeight) are tabulated per family in `font-metadata.ts`.

## Decision

On load, the port keeps stored `width`, `height` and `text` untouched. It measures only when the user edits text, resizes a container, or explicitly asks to refresh, and it measures with the same woff2 files through `ttf-parser` advance widths (shaping via `rustybuzz` where ligatures or CJK make it matter). A pluggable metrics provider mirrors upstream's, so the test metric can be reproduced and a host can supply the browser's `measureText` if it prefers.

## Implementation

`ex-302`, 2026-09-28. `excali_text::text_measurements` ports `textMeasurements.ts` whole, with the provider passed explicitly instead of set globally; `CharCountTextMetrics` is upstream's test metric. `excali_text::font_store::FontStore` is the default provider. It loads the vendored files under `fonts/` (the faces in `excali_text::font_faces::FONT_FACES`, generated from upstream's `fonts/*/index.ts` by `scripts/fonts/font_faces.py`). It decodes WOFF2 with `wuff` and picks a face per character the way the browser does: the font string's family list in order, and within a family the last registered face whose `unicode-range` and `cmap` cover the character, at the best-matching weight. Each run is shaped with `rustybuzz` using its default features, kerning included, and advances are scaled by `size / unitsPerEm`. The generic `sans-serif` maps to Liberation Sans and `monospace` to Cascadia; `local:` families (Helvetica, Segoe UI Emoji) have no faces and fall through.

Kerning matters. For Excalifont, 135 of 198 corpus texts measure within 0.5 px of the stored width from summed glyph advances alone, and 181 with shaping. Most of the texts that match do so to within 0.001 px. The acceptance fixture (`crates/excali-text/tests/fixtures/text-widths.json`, 991 texts in six families from the library corpus) is measured within 0.5 px, heights included. The largest width deviation per family is 0.29 px for Virgil, 0.07 for Cascadia, 0.36 for Excalifont, 0.06 for Nunito, 0.003 for Lilita One and 0.02 for Comic Shanns.

Over the whole corpus, some texts deviate by more, and none of them points at the measurement. Some were widened by hand after measuring, by exactly 1, 2 or 3 px. Some came from older font builds or other browsers. One library was generated and uses Nunito widths the browser never produced. `ex-308` reports these per family.

## Consequences

Files open pixel-identical. Edits may differ from the browser by sub-pixel amounts; `ex-308` measures that across the corpus and the threshold (0.5 px) is a CI gate for the three top-pick families.
