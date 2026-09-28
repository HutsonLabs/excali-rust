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

`ex-302`, 2026-09-28. `excali_text::text_measurements` ports `textMeasurements.ts` whole, with the provider passed explicitly instead of set globally; `CharCountTextMetrics` is upstream's test metric. `excali_text::font_store::FontStore` is the default provider. It loads the vendored files under `fonts/` (the faces in `excali_text::font_faces::FONT_FACES`, generated from upstream's `fonts/*/index.ts` by `scripts/fonts/font_faces.py`). It decodes WOFF2 with `wuff` and picks a face per character the way the browser does: the font string's family list in order, and within a family the last registered face whose `unicode-range` and `cmap` cover the character, at the best-matching weight. Each run is shaped with `rustybuzz` using its default features, kerning included, and advances are scaled by `size / unitsPerEm`. Each face is parsed once, when it is loaded, and kept with its bytes (`yoke`). Its shaping plan is built once per direction, script and language. A measurement therefore only shapes, which matters because wrapping measures every character. `font_store`'s `measuring_a_line_only_shapes` test bounds the per-character and per-line cost. The generic `sans-serif` maps to Liberation Sans and `monospace` to Cascadia; `local:` families (Helvetica, Segoe UI Emoji) have no faces and fall through.

Kerning matters. For Excalifont, 135 of 198 corpus texts measure within 0.5 px of the stored width from summed glyph advances alone, and 181 with shaping. Most of the texts that match do so to within 0.001 px.

The acceptance fixture is `crates/excali-text/tests/fixtures/text-widths.json`: 991 texts in six families from the library corpus. It was selected on the outcome. A (library, fontFamily) pair went into the fixture only if every one of its eligible texts measured within 0.5 px with this port. The fixture therefore shows that the port reproduces those texts, not the corpus. Nothing independent shows that those texts were measured with the font builds the port vendors. Within the fixture, the largest width deviation per family is 0.29 px for Virgil, 0.07 for Cascadia, 0.36 for Excalifont, 0.06 for Nunito, 0.003 for Lilita One and 0.02 for Comic Shanns, and heights match.

The whole-corpus baseline was measured on 2026-09-28 with the same eligibility rule (`scripts/fixtures/text_widths.py`). It covers 1866 texts in 51 pairs:

| fontFamily | within 0.5 px | max deviation |
| --- | --- | --- |
| 1 Virgil | 1149 / 1243 | 116.7 px |
| 3 Cascadia | 34 / 35 | 0.58 px |
| 5 Excalifont | 181 / 198 | 2.20 px |
| 6 Nunito | 46 / 348 | 47.5 px |
| 7 Lilita One | 2 / 3 | 0.89 px |
| 8 Comic Shanns | 38 / 39 | 0.52 px |

37 of the 51 pairs pass in full. Those 37 are the fixture. The other 14 were excluded, with these texts within 0.5 px:

| library | fontFamily | within 0.5 px | max deviation |
| --- | --- | --- | --- |
| childishgirl/aws-architecture-icons | 1 | 222 / 262 | 3.00 px |
| erlina/data-processing | 1 | 8 / 9 | 2.59 px |
| gabrielamacakova/halloween-elements | 1 | 2 / 6 | 0.73 px |
| hartmut-co-uk/kafka-streams-topology-design | 1 | 57 / 65 | 2.13 px |
| infamousjoeg/cyberark | 1 | 11 / 25 | 2.30 px |
| pratheeshpm/basic-system-design | 1 | 34 / 47 | 116.74 px |
| stojanovic/aws-serverless-icons-v2 | 1 | 10 / 24 | 1.46 px |
| childishgirl/aws-architecture-icons | 3 | 6 / 7 | 0.58 px |
| childishgirl/aws-architecture-icons | 5 | 0 / 1 | 1.00 px |
| hartmut-co-uk/kafka-streams-topology-design | 5 | 27 / 31 | 1.02 px |
| martinberger-ch/oracle-cloud-infrastructure-icons | 5 | 31 / 43 | 2.20 px |
| datavizfairy/dashboard-charts | 6 | 38 / 340 | 47.51 px |
| devdaejungyoon/github-actions | 7 | 1 / 2 | 0.89 px |
| hartmut-co-uk/kafka-streams-topology-design | 8 | 12 / 13 | 0.52 px |

416 texts fall outside 0.5 px. 363 of them measure narrower than their stored width, and 39 of those differ by exactly 1, 2, 3 or 5 px. That fits widths edited by hand or kept from an older font build better than a measuring error, but it has not been shown. The other 53 measure wider, all Nunito texts of `datavizfairy/dashboard-charts`, a generated library. The exclusions are recorded in `EXCLUDED` in `scripts/fixtures/text_widths.py`, and its `check` fails if the fixture and the exclusions stop covering the corpus. `ex-308` owns the whole-corpus gate and starts from these numbers.

## Consequences

Files open pixel-identical. Edits may differ from the browser by sub-pixel amounts; `ex-308` measures that across the corpus and the threshold (0.5 px) is a CI gate for the three top-pick families.
