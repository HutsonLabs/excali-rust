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

`ex-302`, 2026-09-28. `excali_text::text_measurements` ports `textMeasurements.ts` whole, with the provider passed explicitly instead of set globally; `CharCountTextMetrics` is upstream's test metric (since `ex-303` compiled only with the cargo feature `test-util`, which test targets turn on through their dev-dependencies, so no library build measures with it). `excali_text::font_store::FontStore` is the default provider. It loads the vendored files under `crates/excali-text/assets/fonts/` (ADR-004) (the faces in `excali_text::font_faces::FONT_FACES`, generated from upstream's `fonts/*/index.ts` by `scripts/fonts/font_faces.py`). It decodes WOFF2 with `wuff` and picks a face per character the way the browser does: the font string's family list in order, and within a family the last registered face whose `unicode-range` and `cmap` cover the character, at the best-matching weight. Each run is shaped with `rustybuzz` using its default features, kerning included, and advances are scaled by `size / unitsPerEm`. Each face is parsed once, when it is loaded, and kept with its bytes (`yoke`). Its shaping plan is built once per direction, script and language. A measurement therefore only shapes, which matters because wrapping measures every character. `font_store`'s `measuring_a_line_only_shapes` test bounds the per-character and per-line cost. The generic `sans-serif` maps to Liberation Sans and `monospace` to Cascadia; `local:` families (Helvetica, Segoe UI Emoji) have no faces and fall through.

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

`ex-303`, 2026-09-28. `excali_text::text_wrapping` ports `textWrapping.ts` whole: `parse_tokens`, `wrap_text`, `get_wrapped_text_lines` (offsets in UTF-16 code units, as upstream's) and `contains_cjk`. Widths go through the same provider and the caller's `CharWidthCache`, which stands in for upstream's global `charWidth` and keeps its key, the first UTF-16 code unit. Upstream tokenizes with one regex passed to `String.prototype.split`. The port evaluates that regex's alternatives in order at each code point: the emoji sequence first, then the zero-width break rules. It runs them in ECMA-262's split loop, after NFC normalization (`icu_normalizer`). The classes are upstream's. `\s` is JS whitespace, not Unicode `White_Space`: U+FEFF is in and U+0085 is out. The `\p{...}` properties (Script, Emoji, Emoji_Presentation, Extended_Pictographic, Emoji_Modifier, Regional_Indicator) come from ICU (`icu_properties`), which is also where V8 gets them. `tests/text_wrapping.rs` ports `textWrapping.test.ts` case for case under `CharCountTextMetrics`. `tests/text_wrapping_goldens.rs` checks the port against upstream's own `parseTokens` and `getWrappedTextLines`, run at the pin by `tools/goldens/text-wrapping.mjs`, which CI checks for staleness. The goldens cover 4668 wraps and every hard line's tokens: upstream's test inputs, edge cases and seeded random strings, under the 10 px metric and a non-additive one.

`ex-308`, 2026-09-28. `crates/excali-text/tests/text_width_corpus.rs` is the whole-corpus gate. It measures every text element in a vendored family across the ex-003 corpus: the 232 catalogue libraries and the five scene-bearing upstream fixtures (the test library and the four PNG and SVG exports with an embedded scene), 6854 texts in all, checked against `fixtures/manifest.json`. Each text is sorted into a class from its stored fields, never from its width. A `measured` text has `lineHeight`, `autoResize` not false, and a stored height that `measureText` gives (`fontSize × lineHeight × lines`, `getTextHeight`, `textMeasurements.ts:170-177`), so its width is a browser measurement of that text. A `height_mismatch` text has `lineHeight` and `autoResize` but a height `measureText` cannot give, so upstream's measurement did not write it. A `legacy` text has no `lineHeight` and predates the unitless line height. A `fixed_width` text has `autoResize: false`, and its width is a wrap width. The height rule takes out exactly the 328 Nunito texts of `datavizfairy/dashboard-charts`: heights of `fontSize × 1.4` under a stored `lineHeight` of 1.25, and widths of `0.6 × fontSize` per character. That accounts for 302 of the 416 texts ex-302 left outside 0.5 px.

The report, `crates/excali-text/tests/fixtures/text-width-corpus-report.json`, gives count, texts within 0.5 px, max and mean deviation per family and class, the upstream fixtures on their own, and the known deviations. The test rebuilds it and fails if it differs (`EXCALI_BLESS=1` rewrites it). For the `measured` class:

| fontFamily | texts | within 0.5 px | max deviation | mean deviation |
| --- | --- | --- | --- | --- |
| 1 Virgil (gated since `ex-g301`) | 1243 | 1149 | 116.74 px | 0.193 px |
| 3 Cascadia | 35 | 34 | 0.58 px | 0.054 px |
| 5 Excalifont (gated) | 198 | 181 | 2.20 px | 0.144 px |
| 6 Nunito (gated) | 20 | 20 | 0.06 px | 0.003 px |
| 7 Lilita One | 3 | 2 | 0.89 px | 0.393 px |
| 8 Comic Shanns (gated) | 39 | 38 | 0.52 px | 0.049 px |

The four upstream fixture texts are all `legacy` Virgil (`test` stored as 77 px and measured as 79.92, and an emoji that Virgil does not draw). Family 2 (Helvetica, 465 texts) has no vendored faces and is only counted.

For Excalifont, Nunito and Comic Shanns, every `measured` text must be within 0.5 px, and so must each family's mean. The exceptions are the 18 texts in `KNOWN_DEVIATIONS`: 17 Excalifont texts (1 in `childishgirl/aws-architecture-icons`, 4 in `hartmut-co-uk/kafka-streams-topology-design`, 12 in `martinberger-ch/oracle-cloud-infrastructure-icons`) and one Comic Shanns `7` in the kafka library. Each is pinned to its stored and measured width. The list may only shrink: a listed text that comes within 0.5 px fails the test until it is removed. Nothing the port measures reproduces these widths. Each text was measured in every vendored family, in its own family without kerning, and with the single-file Excalifont and Comic Shanns builds upstream shipped before `61623bbeba` (2024-10-20, `fonts/assets` at `a80cb5896a`). None comes within 0.5 px for any text longer than one character, and the older Excalifont measures every corpus text exactly as the vendored one does. All 18 are stored wider than they measure. `K` is exactly 1.000 px wider, and `A` is stored as a whole 25 px. Most neighbouring texts in the same libraries match to 0.001 px. That points to widths kept from an earlier state of the element, but it has not been shown. (`ex-g301` below found the cause of 13 of the 18.)

`ex-g301`, 2026-09-28. Virgil is gated too: `GATED_FAMILIES` is `[1, 5, 6, 8]`. The 94 Virgil `measured` texts outside 0.5 px are in `KNOWN_DEVIATIONS`, and so is the cause of every listed width. Each cause is a width that an earlier upstream wrote. `known_deviations_are_what_their_cause_writes` recomputes it from the vendored fonts and requires the stored width, to 0.001 px (the DOM widths exactly). The causes come from upstream's history (`git log -S` on `excalidraw/excalidraw`, read 2026-09-28):

| cause | upstream | what it writes | Virgil | Excalifont / Comic Shanns |
| --- | --- | --- | --- | --- |
| `dom_offset_width` | `measureText` up to `3a141ca7^` (2023-01-30), `src/element/textElement.ts:258-296` | `offsetWidth` of a `white-space: pre` div holding the text and a 1 px inline-block span after its last line: the widest line, the last 1 px wider, ceiled to a 1/64 px layout unit, then rounded | 30 | |
| `dom_offset_width_plus_one` | `3a141ca7` (2023-01-30) to `9659254f^` (2023-02-23) | the same, plus 1 ("since we are adding a span of width 1px") | 16 | |
| `scaled_dom_offset_width` | either DOM width, then resized (`resizeSingleTextElement` scales `width` and `fontSize` and does not measure) | a DOM width of N px at the font size `fontSize × N / width`, whose stored height also scales back to whole pixels (`offsetHeight`) | 6 | |
| `container_width` | bound text of `5c67329b` (2022-01-03) to `4cb6f095^` (2022-09-22) | `measureText` with `maxWidth` set `style.width`, and `handleBindTextResize` wrote `container.width - 2 × BOUND_TEXT_PADDING`, `x: container.x + 5` | 1 | |
| `ink_box` | `getLineWidth` of `62228e0b` (2024-07-25) to `e3060dfb^` (2025-02-11), in no npm release | `max(abs(actualBoundingBoxLeft) + abs(actualBoundingBoxRight), width)` from the glyph outlines | 8 | 4 |
| `ink_box_whole_pixels` | the same | the same, with the ink box being the advance box grown by the ink overhang on each side, rounded out to whole pixels | 33 | 1 |
| `ink_box_glyph_pixels` | the same | the same, with each glyph's bounds rounded out to whole pixels about its own origin (Blink's bounds without subpixel positioning) | | 8 |
| `kept_width` | none found | | | 5 |

The upstream code fixes each formula. The rounding conventions (1/64 px layout units, how each browser reports the ink box) are inferred from the data, not taken from a browser. Every other `measured` text of these libraries is within 0.5 px of today's advance width. Element `updated` timestamps do not date a measurement. Moving, grouping, pasting and library export change them without measuring. For example, the kafka library's texts are stamped 2025-04-06, after `e3060dfb`.

The DOM causes cover `stojanovic/aws-serverless-icons-v2` (14), `pratheeshpm/basic-system-design` (12 and the bound `API\nGateway`, stored 203 px in a 213 px rectangle, the 116.74 px maximum), `infamousjoeg/cyberark` (13 and one scaled), `erlina/data-processing` and `gabrielamacakova/halloween-elements` (scaled, the four halloween texts from one font size of 85.708 px), and 7 texts of `childishgirl/aws-architecture-icons`. The ink-box causes cover the other 33 childishgirl texts and the 8 kafka texts. Five texts have no cause: the Excalifont `A` (stored as a whole 25 px), `Alarms` and `Oracle\nAutonomous\nDatabase` (each exactly 1 px wider than the per-glyph ink box), `VCN\n(Region Identifier)`, and the Comic Shanns `7`. They stay `kept_width`, pinned by stored and measured width alone. Restore keeps a stored width unless it is called with `refreshDimensions` (`packages/excalidraw/data/restore.ts:1033-1046` at the pin), and the port keeps it on load. On edit the port measures today's advance width, as upstream at the pin does.

## Consequences

Files open pixel-identical. Edits may differ from the browser by sub-pixel amounts; `ex-308` measures that across the corpus, and the 0.5 px threshold is a CI gate (`cargo test`) for Virgil and the three top-pick families, per text and per family mean.
