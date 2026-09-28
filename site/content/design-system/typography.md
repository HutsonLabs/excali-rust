+++
title = "Typography"
description = "The UI font and the ten canvas font families with the metrics the port must ship, plus what is known about each licence."
weight = 3
+++

## UI font

`--ui-font: Assistant, system-ui, BlinkMacSystemFont, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif` (`styles.scss:42-43`). Assistant is bundled at weights 400/500/600/700 as woff2. Buttons use `0.8333rem`; the top-level container inherits 16 px.

## Canvas fonts

| id | Family | unitsPerEm | ascender | descender | line height | Status | Licence evidence in upstream repo | Licence at source (ADR-004) |
|---|---|---|---|---|---|---|---|---|
| 1 | Virgil | 1000 | 886 | -374 | 1.25 | deprecated | none | SIL OFL 1.1 |
| 2 | Helvetica | 2048 | 1577 | -471 | 1.15 | deprecated, local system font | n/a (never bundled) | local only |
| 3 | Cascadia | 2048 | 1900 | -480 | 1.2 | deprecated | none | SIL OFL 1.1 |
| 4 | reserved | | | | | historic Assistant/Obsidian slot | | |
| 5 | Excalifont | 1000 | 886 | -374 | 1.25 | **default** | SIL OFL 1.1 (`fonts/Excalifont/index.ts:11-117`) | SIL OFL 1.1 |
| 6 | Nunito | 1000 | 1011 | -353 | 1.25 | "Normal" top pick | none | SIL OFL 1.1 |
| 7 | Lilita One | 1000 | 923 | -220 | 1.15 | | none | SIL OFL 1.1 |
| 8 | Comic Shanns | 1000 | 750 | -250 | 1.25 | "Code" top pick | MIT (`fonts/ComicShanns/index.ts:11-14,45`) | MIT |
| 9 | Liberation Sans | 2048 | 1854 | -434 | 1.15 | private | none (the file is 1.05, GPLv2 with font exception) | SIL OFL 1.1 from release 2.1.5 |
| 10 | Assistant | 2048 | 1021 | -287 | 1.25 | private | none | SIL OFL 1.1 |
| 100 | Xiaolai (CJK fallback) | 1000 | 880 | -144 | 1.25 | fallback for Excalifont | SIL OFL 1.1 (`fonts/Xiaolai/index.ts:216-232`) | SIL OFL 1.1 |

Fallback chains: Excalifont → Xiaolai → generic → Segoe UI Emoji; others → generic → emoji. Cascadia and Comic Shanns use the monospace generic; the rest sans-serif. Sizes: S 16, M 20, L 28, XL 36; default 20.

"None" in the upstream-evidence column means the upstream repository holds no licence file for that family under `packages/excalidraw/fonts`. The last column is the licence task `ex-306` confirmed on 2026-09-28 at each family's own source; the licence file, source URL and evidence for each are in [ADR-004](../../decisions/adr-004-fonts/). Every bundled family is SIL OFL 1.1 or MIT, so the port ships them all. The exception is upstream's Liberation Sans file, version 1.05 under the GPLv2-with-font-exception terms of Liberation 1.x: the port ships the OFL release 2.1.5 of the same family instead, whose vertical metrics are identical and whose advance widths match on all but two code points. `scripts/gates/fonts.py` enforces this in CI. term.hut's current viewer maps families to local stacks (`excalidrawScene.js:42-59`); the port keeps that only as the fallback for a family whose licence is not confirmed.

## Text layout rules

- Line height in px = `fontSize × lineHeight`.
- Baseline offset = `em × ascender + (lineHeightPx − em × ascender + em × descender) / 2`, `em = fontSize / unitsPerEm`.
- Width = advance width of the widest line; tabs are 8 spaces; an empty line measures as one space.
- Bound text padding 5 px; ellipse inset `(w/2)(1 − √2/2)`; diamond inset `w/4`; arrow label max width `max(0.7w, 11 × fontSize)`; sticky note padding 16.
- `autoResize: false` wraps at the stored width; `text` holds the wrapped result and `originalText` the source.

## Font picker

Three top picks: Excalifont "Hand-drawn", Nunito "Normal", Comic Shanns "Code". Deprecated families are hidden unless the scene already uses them and carry an "old" badge. Opened with Shift+F.
