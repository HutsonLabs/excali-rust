+++
title = "ADR-003: Sketch renderer, goldens before porting"
description = "Rough.js fidelity is decided by numbers: evaluate roughr against goldens generated from upstream, then adopt, fork or port."
weight = 3
+++

**Status.** Accepted. Decided by task `ex-206` (2026-09-28): port rough.js 4.6.4 into `excali-rough`.

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

Generate goldens first (`ex-004`), run `roughr` against them (`ex-206`), and choose by the match rate: adopt at 100 %, fork at a small, fixable divergence (at least 90 % of the goldens), port otherwise. Whatever is chosen sits behind the `excali-rough` interface so the rest of the stack does not care.

## Result

`tools/roughr-eval` (a stand-alone package, not a workspace member) replays every rough.js golden through `roughr` 0.14.0 from crates.io, pinned `=0.14.0`:

- `goldens/random.json`: `Random.next()` sequences;
- `rough-primitives.json`, `rough-generator.json`, `rough-fills.json`, `rough-options.json` and `rough-strokes.json`: `new RoughGenerator()[method](...args, options)` from roughjs 4.6.4. Each is replayed as the same `roughr::generator::Generator` call. `rough-strokes.json` (ex-205) is Excalidraw's solid, dashed and dotted strokes with the options upstream's `generateRoughOptions` sets for them (`strokeLineDash`, `disableMultiStroke`, `strokeWidth + 0.5`, `preserveVertices`).

Each call gets the options rough.js resolved for that case (the golden drawable's `options`), which is what an adapter would pass. That includes rough.js's `bowing: 1`, since roughr's own default is 2. `stroke`/`fill` `"none"` and `"transparent"` map to no colour.

A case matches when every set type, op kind and number equals the golden after `+x.toFixed(2)`. That is the precision of upstream's SVG export (`fixedDecimalPlaceDigits` 2, `packages/excalidraw/renderer/staticSvgScene.ts:71`), and the loosest "equal" a user can see. "Exact" means within the relative 1e-10 that the port's own gate allows.

The numbers are in `tools/roughr-eval/report.json`. `tools/roughr-eval/tests/eval.rs` fails if that file differs from a fresh run, or if this page does not quote it.

**Match rate.** roughr 0.14.0 matches 101 of 1346 goldens (7.5 %). 27 of them draw nothing: `stroke: "none"` without fill, empty paths, and point lists too short to draw. The rest are roughness-0 ellipses, circles, arcs and curves, and zero-length lines.

At roughness 0 a rectangle still differs, because rough.js's line diverge point `0.2 + random() * 0.2` is not scaled by roughness. So no Excalidraw rectangle, diamond, line or arrow matches at any roughness.

**Counterfactual fork.** `tools/roughr-eval/park-miller.patch` swaps rough.js's `Random` in for roughr's `StdRng` and changes nothing else. `python3 tools/roughr-eval/fork.py` applies it to the resolved crate sources under `target/` and writes `report-fork.json`. With rough.js's generator patched in, roughr matches 501 of 1346 goldens (37.2 %).

| golden file | cases | exact | two decimals | two decimals, generator patched |
|---|---|---|---|---|
| `random.json` | 7 | 0 | 0 | 7 |
| `rough-primitives.json` | 117 | 3 | 12 | 75 |
| `rough-generator.json` | 228 | 37 | 43 | 110 |
| `rough-fills.json` | 224 | 0 | 1 | 28 |
| `rough-options.json` | 140 | 0 | 0 | 7 |
| `rough-strokes.json` | 630 | 30 | 45 | 274 |
| total | 1346 | 70 | 101 | 501 |

**Divergences.** Every one of the 1245 mismatches is attributed to one divergence, found by reading both sources. A mismatch that the patched build matches is `rng`. Any other mismatch is attributed to the divergence behind the first difference the patched build still has, so each count below is "cases whose first remaining cause is this". The same attribution is in `report.json`, case by case.

| divergence | cases |
|---|---|
| `pattern-fill` | 501 |
| `rng` | 400 |
| `svg-path` | 168 |
| `curve-reseed` | 72 |
| `solid-fill-shape` | 54 |
| `f32` | 28 |
| `path-simplification` | 12 |
| `seed-range` | 7 |
| `path-draw-order` | 2 |
| `fill-sentinel` | 1 |

What each divergence is:

- **`rng`**
  - roughr draws from rand's `StdRng` (ChaCha12, `seed_from_u64`, `core.rs` `Options::random`) and narrows every draw to f32. rough.js draws from its Park–Miller `Random` (`bin/math.js`).
  - The seed is the whole point of Excalidraw's shape stability.
- **`pattern-fill`**
  - rough.js 4.6.4 builds hachure, cross-hatch, zigzag, dashed and zigzag-line fills on `hachure-fill` 0.5.2. At roughness ≥ 1 it also takes one draw to pick a skip offset.
  - roughr keeps the older scan-line hachure, so every fill line starts and ends elsewhere. For example, the first line of a 100×60 rectangle starts at 10, 10, where rough.js starts at 9.99, 10.01. The dashed and zigzag-line fillers produce different op counts.
- **`svg-path`**
  - roughr's `svg_path` emits an extra `move` for every `M`, and draws a move for a path that is only a move.
  - It parses with `svgtypes` and `svg_path_ops`, where rough.js uses `path-data-parser` 0.1.0. As a result, a path without a leading `M` draws nothing, and `- 20` ends the path; rough.js rewrites it to `-20`.
  - This affects every path stroke: the rounded rectangles, diamonds and curved arrows Excalidraw draws with `generator.path`.
- **`curve-reseed`**
  - rough.js's `cloneOptionsAlterSeed` drops the randomizer and restarts the second stroke of a curve at `seed + 1`.
  - roughr's `clone_options_alter_seed` copies the live randomizer, so the second stroke continues the first stroke's stream.
- **`path-simplification`**
  - With `simplification < 1`, roughr's `points_on_path` samples differently from `points-on-path` 0.2.1.
  - roughr also simplifies at `simplification: 0`, which rough.js treats as unset.
- **`solid-fill-shape`**
  - For solid fills of curves and single-subpath paths, rough.js 4.6 draws a single-stroke sketch at `roughness + fillShapeRoughnessGain` and merges it (`_mergedShape`).
  - roughr fills the sampled polygon, and has no `fillShapeRoughnessGain`.
- **`f32`**
  - Options are f32, and constants go through f32 (`_c(0.2)`, `-0.0016668`, and `f32::PI() * 2` as the bound of the ellipse and arc loops).
  - As a result, ellipses and arcs gain or lose a point, and some numbers round the other way at two decimals.
- **`seed-range`**
  - roughr's seed is a `u64`. rough.js accepts any integer, so negative seeds have no roughr equivalent.
- **`path-draw-order`**
  - `generator.path` in rough.js sketches the stroke before the fill; roughr does the fill first, so each takes the other's random draws.
- **`fill-sentinel`**
  - rough.js fills a rectangle, ellipse, polygon or arc for any non-empty `fill` string, `"none"` included. roughr's fill is an `Option<Srgba>`.

Recommendation: **port**.

- 7.5 % is far from 100 %, and even the counterfactual fork reaches only 37.2 %.
- What remains after the generator swap is not a small, fixable divergence:
  - the fill algorithm, used by every filled Excalidraw shape;
  - the SVG path parser and renderer, used by every rounded or curved shape;
  - curve reseeding;
  - solid fills;
  - and f32 arithmetic throughout the crate, since `Options` is f32 in its public API.
- A fork would replace most of roughr and diverge from its upstream at the API level. At that point it is the port, with a foreign structure.

## Consequences

- `ex-203` to `ex-205` are a port of rough.js 4.6.4 into `excali-rough`, not adapter work. The port is in place: `crates/excali-rough/tests/goldens.rs` checks the same 1346 cases, exactly or within 1e-10 for platform trig, and passes.
- `roughr` is not a dependency of any product crate. `tools/roughr-eval` stays outside the workspace. It is kept so the question can be reopened with numbers if a later roughr release claims rough.js 4.6.4 parity: bump the pin, run `python3 tools/roughr-eval/fork.py --write` and `cargo run --manifest-path tools/roughr-eval/Cargo.toml -- --write`, and update the tables above. The `roughr-eval` CI job checks both reports on every PR.
- The port is never judged by eye. The goldens become a permanent CI gate (`ex-217`).
