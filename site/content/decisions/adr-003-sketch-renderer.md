+++
title = "ADR-003: Sketch renderer, goldens before porting"
description = "Rough.js fidelity is decided by numbers: evaluate roughr against goldens generated from upstream, then adopt, fork or port."
weight = 3
+++

**Status.** Proposed; decided by task `ex-206`.

## Question

Should the port depend on the existing `roughr` crate, fork it, or port rough.js 4.6.4 itself?

## Evidence

- Excalidraw stores a per-element `seed` so "the roughjs shape doesn't differ across renders" (`types.ts:56-58`), and term.hut's viewer relies on the same property ("the same seed with different options is still a different drawing", `excalidrawScene.js`).
- rough.js's generator is a Park–Miller LCG: `((2^31 − 1) & (seed = imul(48271, seed))) / 2^31` (`rough-stuff/rough` `src/math.ts`, verified on master 4.6.6; upstream pins 4.6.4).
- `roughr` 0.14.0 (updated 2026-09-24) is "a Rust port of the Rough.js npm package" with `OptionsBuilder` fields including `seed`, `roughness`, `bowing`, `hachure_angle`, `hachure_gap`; adapters exist for tiny-skia, vello, iced and piet (`orhanbalci/rough-rs` README). The README does not state which rough.js version it tracks or claim output equality.
- Upstream's SVG export writes numbers with two decimals, so "equal" has a precise meaning.

## Options

1. Adopt `roughr` as is.
2. Fork `roughr` and fix divergences.
3. Port rough.js 4.6.4 into `excali-rough`.

## Decision

Generate goldens first (`ex-004`), run `roughr` against them (`ex-206`), and choose by the match rate: adopt at 100 %, fork at a small, fixable divergence, port otherwise. Whatever is chosen sits behind the `excali-rough` interface so the rest of the stack does not care.

## Consequences

The port is never judged by eye. The goldens become a permanent CI gate (`ex-217`).
