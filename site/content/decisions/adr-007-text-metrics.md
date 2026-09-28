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

## Consequences

Files open pixel-identical. Edits may differ from the browser by sub-pixel amounts; `ex-308` measures that across the corpus and the threshold (0.5 px) is a CI gate for the three top-pick families.
