# Golden generator

`generate.mjs` writes `goldens/*.json`: the exact numbers upstream Excalidraw
produces for fixture elements, so the Rust crates (`excali-rough`,
`excali-freehand`, `excali-scene`) can be checked op by op.

The numbers are upstream's own. The generator bundles upstream's TypeScript
from the pinned checkout (`scripts/upstream/checkout.sh`, commit in
`site/config.toml`) with esbuild, which only strips types, and calls
`ShapeCache.generateElementShape` from `packages/element/src/shape.ts`. That is
the function the editor and the SVG export use. `@excalidraw/*` imports
resolve to the checkout's package sources, the same way the aliases in
upstream's `vitest.config.mts:63-71` and `excalidraw-app/vite.config.mts:86-89`
resolve them. That makes laser-pointer the vendored in-repo copy. The rendering packages are the
tarballs upstream's `yarn.lock` resolves. `package-lock.json` pins roughjs
4.6.4, perfect-freehand 1.2.0, points-on-curve 1.0.1 and tinycolor2 1.6.0, and
`test/pins.test.mjs` compares their integrity hashes with upstream's lockfile.

```sh
scripts/upstream/checkout.sh               # once; shared by every worktree
npm ci --prefix tools/goldens
node tools/goldens/generate.mjs            # write goldens/
node tools/goldens/generate.mjs --check    # exit 1 if goldens/ is stale
node --test tools/goldens/test/            # the suite CI runs
```

The generator runs under plain Node, with no browser or DOM. It refuses to run
unless the checkout is clean and at the pin. While it generates, `Math.random`
throws, so no output can come from rough.js's seed-0 fallback. Output is
byte-identical across runs and across Node 22, 24 and 26. CI (the `goldens`
job in `.github/workflows/gates.yml`) runs the suite and `--check` on every PR.

The committed goldens are the arm64 output (Apple silicon, Linux arm64; the two
agree byte for byte), and the CI job runs on `ubuntu-24.04-arm`. A few
math.json cases where upstream's result goes through the platform `pow` and
macOS libm and glibc disagree in the last bit are left out
(`PLATFORM_DEPENDENT_CASES` in `math.mjs`) so the two stay identical. On x86_64,
V8's floating-point results for some shapes differ in the last bits, so
`--check` and the stability test report stale files there. Regenerate goldens
on an arm64 machine only.

## Files

| file | contents | used by |
|---|---|---|
| `random.json` | `Random.next()` sequences for 7 seeds | ex-203 |
| `rough-primitives.json` | `line`, `rectangle`, `polygon`, `ellipse`, `circle`, `arc`, `curve`, `linearPath`, `path` × seeds 1, 7, 1041657908 × roughness 0, 1, 2 | ex-203, ex-206 |
| `rough-generator.json` | generator edge cases × roughness 0, 1, 2: SVG path syntax (implicit commands, missing `M`, compact numbers, `S`/`T`, arcs), `simplification`, `stroke: "none"`, single stroke, preserveVertices, short point lists, line length bands, ellipse/arc geometry, seed wrap-around | ex-203 |
| `rough-fills.json` | hachure, cross-hatch, zigzag, solid, dashed, zigzag-line at `fillWeight = sw/2`, `hachureGap = sw*4`, the seeds × roughness grid, and `edge/` cases: curve and closed-arc fills, subpaths, a concave polygon, unrotated scans and `Math.round` halves, default and sub-0.1 gaps, single fill stroke, roughness 0 and 2, fill `none`/`transparent`, a lone moveto under `simplification` | ex-204 |
| `rough-options.json` | multi-stroke, preserveVertices, curveFitting, bowing, dashes, hachure angle, and the dashed/dotted stroke rule | ex-205 |
| `rough-strokes.json` | solid, dashed and dotted strokes × sw 1, 2, 4 × roughness 0, 1, 2 (seeds 1 and 7 at sw 2), each with the options upstream's `generateRoughOptions(element, continuousPath)` returns (dashed `[8, 8+sw]`, dotted `[1.5, 6+sw]`, single stroke at `sw + 0.5`, preserveVertices, curveFitting 1 for ellipses) and drawn with the generator method that element type uses: `{ id, element, continuousPath, method, args, options, drawable }` | ex-205 |
| `elements-upstream-fixtures.json` | upstream `tests/fixtures/elementFixture.ts` and the export test's 100×100 variants | all |
| `elements-rectangle.json`, `elements-diamond.json`, `elements-ellipse.json` | seeds × roughness, fills, stroke styles, adjustRoughness sizes, corner radius, dark theme | ex-208 |
| `elements-line.json`, `elements-arrow.json` | linearPath, filled polygon loops, curves | ex-209 |
| `elements-elbow-arrow.json` | elbow paths (radius 16), extreme-coordinate guard | ex-210 |
| `elements-arrowheads.json` | all 14 arrowheads, start and end, sw 1/2/4, curved, dashed, dotted, short, outline fills | ex-212 |
| `elements-freedraw.json` | perfect-freehand and laser-pointer outlines, trimmed SVG path, loop fills | ex-213, ex-214, ex-215 |
| `elements-iframe-like.json` | `modifyIframeLikeForRoughOptions` placeholders and defaults | ex-208 |
| `elements-matrix.json` | the M2 matrix: every element type with a background colour at every fill style × roughness 0, 1, 2 × seeds 1, 7, 1041657908 (rectangle sharp and roundness 3, diamond sharp and roundness 2, ellipse, iframe, embeddable, line loop sharp and curved, polygon line, arrow sharp and curved with a filled triangle head, freedraw loop variable and constant width; text, image, frame, magicframe and stickynote record no shape); `crates/excali-scene/tests/element_matrix.rs` fails on any missing cell | ex-g202 |
| `freehand.json` | `getStrokePoints` and `getStroke` with Excalidraw's options, the library defaults, and `edge/` cases for the branches Excalidraw never reaches (taper `true`/`false`, flat caps, cap easings, one-point strokes with a taper, `{x, y, pressure}` points, missing and negative pressures, reversals, duplicates, size 0, no points) | ex-213 |
| `laser-pointer.json` | the vendored `@excalidraw/laser-pointer` (`packages/laser-pointer/src`): `excalidraw/` cases with `getConstantWidthFreedrawOutline`'s options (size `sw * 1.4`, simplify 0, `sizeMapping` `max(0.1, pressure)`, pressure 1) for the freedraw fixture points, and `edge/` cases for the rest of the library (defaults, `output`/`input`/`tail` simplify phases and the tail's `Not implemented yet` throw, corners at both speeds, zero sizes, `keepHead`, size overrides, one and two points, duplicates, no points); `sizeMapping` is named (`SIZE_MAPPINGS` in `fixtures.mjs`) | ex-214 |
| `math.json` | every `packages/math/src` export except `pca.ts`, called on fixed and Park-Miller-random inputs (`math.mjs`): `{ id, fn, args, result }`; the `curve.ts` cases (including the `curveLength` fixtures) are ex-202's | ex-201, ex-202 |
| `js-math.json` | V8's `Math` transcendentals (sin, cos, tan, asin, acos, atan, atan2, exp, log, log2, log10, cbrt, hypot, and pow where it is not platform-dependent) on tricky arguments, every double as IEEE 754 bits in hex (`js-math.mjs`); `crates/excali-math/tests/js_math.rs` checks `excali_math::js` against it bit for bit | ex-009 |
| `js-sort.json` | V8's `Array.prototype.sort` (TimSort) permutation when the comparator answers NaN, and `convexHull` on points with NaN or infinite coordinates (`jssort.mjs`) | ex-201 |
| `fractional-indexing.json` | vendored `fractional-indexing`: `validateOrderKey`, `generateKeyBetween` over every pair of a key pool (base 62, plus the rocicorp suite's base 10 and base 95), `generateNKeysBetween` for n = 0..40 and long runs, random insertion walks; result or thrown message | ex-107 |
| `fractional-index.json` | `element/src/fractionalIndex.ts`: `syncInvalidIndices` and `syncMovedIndices` on every scenario of `fractionalIndex.test.ts` and 400 random lists (indices and versions after), `syncInvalidIndicesImmutable` on those scenarios and 200 random lists with duplicate ids (the returned map), `validateFractionalIndices` log messages, `orderByFractionalIndex` (V8 TimSort order, also with unindexed elements and duplicate ids) | ex-107 |
| `manifest.json` | upstream commit, package versions, case count and sha256 per file | ex-217 |

rough.js's `dots` fill is not included. Its filler jitters every dot with
`Math.random` (`roughjs/bin/fillers/dot-filler.js:33-34`), so it has no stable
output, and Excalidraw never uses it.

## Format

Every file is `{ "description", "cases": [...] }` and every case has a unique
`id`.

- **rough cases:** `{ id, method, args, options, drawable }`. The call was
  `new RoughGenerator()[method](...args, options)` on a fresh copy of
  `args`: rough.js's pattern fillers rotate the polygon points in place and
  back (hachure-fill `rotatePoints`), which moves them in the last bits, so
  fixture arrays shared between cases would otherwise drift.
- **element cases:** `{ id, element, renderConfig, shapes }`. The call was
  `ShapeCache.generateElementShape(element, renderConfig)`, with
  `embedsValidationStatus` built from `renderConfig.validatedEmbeds`.
  - `shapes` is upstream's shape flattened to a list. Each entry is either
    `{ type: "rough", drawable }` or `{ type: "svgPath", d }` (the freedraw
    stroke). `shapes` is empty when upstream returns `null` (text) or `[]`.
  - Freedraw cases also carry `outline`, the output of
    `getFreedrawOutlinePoints`.
- **drawable:** `{ shape, options, sets: [{ type, ops: [{ op, data }] }] }`.
  This is rough.js's `Drawable` with every resolved option except the RNG
  state.
  - `type` is `path`, `fillPath` or `fillSketch`.
  - `op` is `move` (2 numbers), `lineTo` (2) or `bcurveTo` (6).
- **freehand cases:** `{ id, points, options, strokePoints, outline }`.
  `options.easing` is a name: `easeOutSine` is `sin(t·π/2)` (upstream
  `shape.ts:1241`) and `linear` is `t`.
- **js-sort cases:** `{ id, kind: "sort", pattern, values, result }` (the
  call was `[0..n).sort((i, j) => values[i] - values[j])`) or `{ id, kind:
  "convexHull", points, result }`; `result` is indices into the input. JSON
  has no NaN or infinity, so those inputs are the strings `"NaN"`,
  `"Infinity"` and `"-Infinity"`.
- **fractional indexing cases:** `{ id, fn, ...inputs, result | error }` for
  `validateOrderKey` (`valid`, `error`), `generateKeyBetween` (`a`, `b`,
  optional `digits`) and `generateNKeysBetween` (plus `n`); `walk` cases
  insert at each of `picks` in a growing sorted list and record `keys`.
- **fractional index cases:** `elements` are `{ id, index }` (validation
  cases add `type`, `boundElements`, `isDeleted`, `version`, ...). Sync cases
  record `validInput`, then `indices`, `versions` and `validOutput` (or
  `error`); `syncInvalidIndicesImmutable` cases record the returned map as
  `entries`, `[id, position in elements, index, version]` in map order;
  validation cases record the logged `messages`; order cases the
  resulting `order` as positions in `elements`. The bundle defines `import.meta.env.MODE` as
  `"production"`, which `getUpdatedTimestamp` reads.

Numbers are full-precision doubles written in ECMAScript's shortest
round-trip form. Parse them exactly; in Rust, use serde_json's
`float_roundtrip` feature. Upstream's SVG export rounds to 2 decimals.
`test/upstream-snapshot.test.mjs` shows that the goldens reproduce upstream's
`export.test.ts.snap` paths exactly at that precision.

## Rust harness

The Rust side compares with the goldens through one harness,
`excali_rough::goldens` (cargo feature `goldens`; ex-217). It turns a
`Drawable` into the golden form (`drawable_json`), walks shape, options,
sets, ops and data, and reports every difference with where it is and by how
much:

```text
elements-rectangle.json: 1 of 54 cases differ from upstream

case rectangle/seed7-r0, element rectangle_seed7-r0, shape 0, set 0 (path), op 1 (bcurveTo) data[0]
    expected 40.00629382208
    actual   40.0062938220799
    diff     -1.0658141036401503e-13 (15 ulp; tolerance exact)
    expected op bcurveTo [40.00629382208, 0, 80.0125876441598, 0, 200, 0]
    actual op   bcurveTo [40.0062938220799, 0, 80.0125876441598, 0, 200, 0]
```

Different op, set or shape counts, op kinds, set types, options and SVG path
tokens (freedraw `svgPath` shapes) are reported the same way. Every number
compares exactly, on every platform: the port's transcendentals (sin, cos,
atan2, exp, log, pow, hypot, cbrt and the rest) go through `excali_math::js`,
which answers V8's `Math` bit for bit (ADR-011, pinned by `js-math.json`), so
there is no platform tolerance. `Manifest` checks every file
against `manifest.json` (sha256, case count) and the upstream commit against
`site/config.toml` (`crates/excali-rough/tests/goldens_manifest.rs`).

The parity tests always build with the feature (excali-rough lists itself as
a dev-dependency with `goldens`, excali-scene lists excali-rough with it), so
a plain `cargo test --workspace` runs every golden. CI also runs the golden
targets on arm64, where the goldens were generated:

```sh
cargo test --workspace --features goldens --test goldens --test element_matrix --test golden_harness --test goldens_manifest
```

## Adding cases

Add inputs to `fixtures.mjs` (fractional indexing: `fixtures-fractional.mjs`), run `node tools/goldens/generate.mjs`, and
commit `goldens/` together with the change. Coverage the downstream tasks rely
on is asserted in `test/goldens.test.mjs`.

## Scene fixtures

`scene-fixtures.mjs` writes the `.excalidraw` fixtures of excali-core's typed
codec tests (`crates/excali-core/tests/fixtures/every-type.excalidraw`,
`unknown-keys.excalidraw`, `unknown-keys-edited.excalidraw`) with upstream's
own element constructors (`packages/element/src/newElement.ts`),
`mutateElement` and `serializeAsJSON` (`packages/excalidraw/data/json.ts`),
bundled from the pinned checkout the same way. `data/json.ts` also imports the
browser-only `./blob` and `./filesystem` modules, which `serializeAsJSON`
never calls; the loader replaces those two with empty modules.

The output is fixed: `Date.now` returns 1700000000000 before upstream's
modules load, upstream's `reseed(1700000000000)` runs before each fixture (so
every `versionNonce` drawn by `newElementWith` or `mutateElement` is
reproducible), and `Math.random` throws while generating. Node has no canvas,
so text is measured at 10 px per character through upstream's
`setCustomTextMetricsProvider`.

```sh
node tools/goldens/scene-fixtures.mjs           # write the fixtures
node tools/goldens/scene-fixtures.mjs --check   # exit 1 if they are stale
```

CI runs `--check` in the `goldens` job, and `test/scene-fixtures.test.mjs`
checks that two runs are byte-identical.

## Restore fixtures

`restore-fixtures.mjs` writes `crates/excali-core/tests/fixtures/restore-base.json`,
the table excali-core's base normalisation (ex-103) is checked against: upstream's
own `restoreElementWithProperties` and, for the generic types, `restoreElement`
(`packages/excalidraw/data/restore.ts:430-515, 726-731`), called on about 300
inputs (every base field absent, `null`, `0`, `""`, `false` and set; legacy
`strokeSharpness` per type; `boundElementIds`; links through `normalizeLink` and
`@braintree/sanitize-url` 6.0.2; negative sizes; key order; `extra`).

`restoreElementWithProperties` is private to `restore.ts`. The loader's `expose`
option (`lib/upstream.mjs`) loads that file unchanged with one line appended,
`export { restoreElementWithProperties };`, so the function called is upstream's.
Upstream runs in its test mode (`import.meta.env.MODE` is `"test"`): `randomId()`
gives `id0`, `id1`, ... (restarted by `reseed` before each case) and
`getUpdatedTimestamp()` gives 1 (`packages/common/src/random.ts:16`,
`utils.ts:552`). excali-core's `restore::TestEnv` is the same environment.

It also writes `restore-element.json`, the table for the per-type rules (ex-104):
upstream's `restoreElement` (`restore.ts:517-752`) on about 380 inputs listed in
`lib/restore-element-cases.mjs`. The `isTransparent` table behind the sticky note
colours (`packages/common/src/colors.ts:389-391`, tinycolor2 1.6.0) is in
`app-state.json` below, shared with `restoreAppState`, since both read colours
through excali-core's one tinycolor port (`color.rs`). The `upstream-*` inputs are the
per-element inputs of upstream's `tests/data/restore.test.ts`, built the way that
test builds them (`apiCreateElement` is `API.createElement` over upstream's own
constructors; text is measured at 10 px per character, as in upstream's test
environment). The others are tables over each rule: legacy `font` strings, line
height detection, freedraw points and pressures, `draw` to `line`, arrowheads,
point re-basing, polygons, the 75,000 px cap, bindings, `fixedSegments`, sticky
note colours and sizes, and the inputs upstream throws on. `reseed(1)` runs before
each case, so `versionNonce` draws start from roughjs `Random(1)`.

A legacy arrow binding (no `mode`) whose target exists is migrated with element
geometry (`restore.ts:362-418`). The case records which ends reached that branch
in `geometry`, found by running the case again with
`LinearElementEditor.getPointAtIndexGlobalCoordinates` (the branch's first call)
replaced by one that throws. excali-core's own test answers the migration
from the recorded result; `crates/excali-editor/tests/legacy_binding.rs`
restores every `geometry` case with excali-editor's `RoutingEnv`, which
computes it (ex-116), and compares the whole result.

```sh
node tools/goldens/restore-fixtures.mjs           # write the fixtures
node tools/goldens/restore-fixtures.mjs --check   # exit 1 if either is stale
```

CI runs `--check` in the `goldens` job, and `test/restore-fixtures.test.mjs`
checks that two runs are byte-identical.

## Scene-level restore fixture

`restore-elements-fixtures.mjs` writes
`crates/excali-core/tests/fixtures/restore-elements.json` for excali-core's
scene-level restore (ex-105): upstream's `restoreElements` and
`bumpElementVersions` (`packages/excalidraw/data/restore.ts:946-1173`) on the
scenes listed in `lib/restore-elements-cases.mjs`. The `upstream-*` scenes are
the scene-level inputs of `tests/data/restore.test.ts` ("restoreElements" and
"repairing bindings"), built as that test builds them; the others are tables over
each pass: duplicate ids, index repair, invisibly small elements, frames, bound
text in both directions, linear bindings, sticky notes, bound text order, elbow
arrow fix-ups, the inputs upstream throws on, and `refreshDimensions` over
rectangle, ellipse, diamond, line and arrow containers and free text of every
alignment (`refresh-*`). Each case records the elements
in order, or the message thrown. The environment is the one of
`restore-fixtures.mjs` (test mode, `reseed(1)` before each case, text at 10 px per
character).

Three steps need geometry or text measurement that excali-core takes from its
`RestoreEnv`: `updateElbowArrowPoints`, `refreshTextDimensions` and
`getStickyNoteLayout`. The loader's `patch` option (`lib/upstream.mjs`) routes the
one call of each in `restore.ts` through a probe that calls upstream's function
and records its arguments and result in the case's `hooks`, in call order;
nothing else in the module changes. excali-core's fixture test checks that the
port asks for the same calls and answers them with the recorded results;
excali-editor (`tests/elbow_routing.rs`) and excali-text
(`tests/refresh_text_dimensions.rs`) reproduce the recorded elbow routes and text
refits from their arguments; the arrow labels of `refresh-arrow-labels` are
placed by excali-editor's `SceneArrowGeometry`, so excali-editor
(`tests/restore_arrow_labels.rs`) reproduces those.

```sh
node tools/goldens/restore-elements-fixtures.mjs           # write the fixture
node tools/goldens/restore-elements-fixtures.mjs --check   # exit 1 if stale
```

CI runs `--check` in the `goldens` job, and
`test/restore-elements-fixtures.test.mjs` checks that two runs are
byte-identical.

## AppState goldens

`app-state.mjs` writes `crates/excali-core/tests/fixtures/app-state.json` for
excali-core's AppState port (ex-106). It bundles upstream's
`getDefaultAppState`, `cleanAppStateForExport`,
`clearAppStateForLocalStorage`, `clearAppStateForDatabase`
(`packages/excalidraw/appState.ts`), `restoreAppState` and
`AllowedExcalidrawActiveTools` (`packages/excalidraw/data/restore.ts`), and
`colorToHex` / `isTransparent` (`packages/common/src/colors.ts`, on the pinned
tinycolor2 1.6.0), and records:

- `getDefaultAppState()` once per environment: each is a separate bundle with
  `devicePixelRatio` and `import.meta.env.MODE` defined, since `exportScale`
  and `currentItemRoundness` are fixed when the module loads;
- the keys each storage cleaner keeps, for every `APP_STATE_STORAGE_CONF` key;
- `colorToHex` and `isTransparent` for colour strings in every notation,
  including the alpha edge cases restoreElement's sticky note rules meet;
- `restoreAppState(appState, localAppState)` for upstream's test cases and
  every branch of its legacy handling, each result a diff against the
  production, device-pixel-ratio-1 defaults (or the error it throws).

Inputs are JSON round-tripped before upstream sees them, so the Rust tests read
exactly what upstream read. Nothing here draws random numbers, and
`Math.random` throws while generating.

```sh
node tools/goldens/app-state.mjs           # write the fixture
node tools/goldens/app-state.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/app-state.test.mjs` checks
that two runs are byte-identical.

## Clipboard fixtures

`clipboard-fixtures.mjs` writes `crates/excali-core/tests/fixtures/clipboard.json`
for excali-core's clipboard JSON codec (ex-113). It bundles upstream's
`serializeAsClipboardJSON`, `parseDataTransferEvent` and `parseClipboard`
(`packages/excalidraw/clipboard.ts:143-193, 467-555`), with the browser-only
`./data/blob` module stubbed (the string paths never call it), and records:

- `serialize`: the exact string `serializeAsClipboardJSON({elements, files})`
  returns for element arrays built from `every-type.excalidraw` with
  `frameId`, id, `fileId` and unknown keys varied per case, after
  `reseed(seed)`. An element whose `frameId` is cleared goes through
  `mutateElement`, so its `versionNonce` is the next `randomInteger()` and
  its `updated` is 1 (test mode);
- `parse`: `parseClipboard` of a paste event holding one `text/plain` string
  (or none), as a normal and as a plain paste: the returned keys, in order,
  and their values.

```sh
node tools/goldens/clipboard-fixtures.mjs           # write the fixture
node tools/goldens/clipboard-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/clipboard-fixtures.test.mjs`
checks that two runs are byte-identical.

## Rough option goldens

`rough-options.mjs` writes `crates/excali-scene/tests/fixtures/rough-options.json`
for excali-scene's option mapping (ex-207). It bundles upstream's
`generateRoughOptions` (`packages/element/src/shape.ts:195-260`, which calls
the module-private `adjustRoughness` at `:172-193`) and `applyDarkModeFilter`
(`packages/common/src/colors.ts:86-125`), and records:

- `options`: `generateRoughOptions(element, continuousPath, isDarkMode)` for
  every row of the option table in `site/content/research/rendering.md`
  section 1 (seed, dashes, stroke widths, roughness, stroke colours light and
  dark, fill per type, `curveFitting`, line and freedraw loops, arrows) and
  the error for the types it throws on. Each case keeps the returned object's
  own keys, so a key set to `undefined` is recorded too;
- `adjustRoughness`: the returned roughness over a grid of sizes around every
  threshold of `adjustRoughness`, for sharp, rounded, linear and freedraw
  elements, and roughness values around the 2.5 cap;
- `darkMode`: `applyDarkModeFilter(color)` for colours in every notation.

Each element is `{...base, ...typeFields[type], ...overrides}`, with `base`
and `typeFields` in the fixture, so the Rust test builds the same object.

```sh
node tools/goldens/rough-options.mjs           # write the fixture
node tools/goldens/rough-options.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/rough-options.test.mjs`
checks that two runs are byte-identical.

## Dark-mode filter goldens

`dark-mode.mjs` writes `crates/excali-scene/tests/fixtures/dark-mode.json` for
the dark-mode colour filter (ex-218). It bundles upstream's
`applyDarkModeFilter` and `removeDarkModeFilter`
(`packages/common/src/colors.ts:86-160`), `rgbToHex` (`:345-362`),
`COLOR_PALETTE` (`:193-212`) and `DARK_THEME_FILTER`
(`common/src/constants.ts:204`), and records:

- `palette`: `COLOR_PALETTE` in upstream's key order;
- `paletteFilter`: for every palette colour, `applyDarkModeFilter`,
  `removeDarkModeFilter`, the reverse of the filtered colour, and the filter
  of that again (the round trip of `colors.test.ts:225-235`);
- `apply`: `applyDarkModeFilter(color)` and `(color, false)` for the cases of
  `colors.test.ts` and every notation tinycolor reads, including strings it
  does not (opaque black);
- `remove`: `removeDarkModeFilter` for the same inputs and all 256 greys;
- `rgbToHex`: the cases of `colors.test.ts:237-305` and alpha rounding.

```sh
node tools/goldens/dark-mode.mjs           # write the fixture
node tools/goldens/dark-mode.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/dark-mode.test.mjs` checks
that two runs are byte-identical and that the palette and its filtered colours
equal upstream's vitest snapshot (`common/src/__snapshots__/colors.test.ts.snap`).

## Font metadata goldens

`font-metadata.mjs` writes `crates/excali-text/tests/fixtures/font-metadata.json`
for the font metadata table and the baseline formula (ex-301). It bundles
upstream's `FONT_METADATA`, `GOOGLE_FONTS_RANGES`, `LOCAL_FONT_PROTOCOL`,
`getVerticalOffset` and `getLineHeight`
(`packages/common/src/font-metadata.ts:35-181`), `getLineHeightInPx`
(`packages/element/src/textMeasurements.ts:91-96`), the family ids and
fallbacks (`common/src/constants.ts:129-197`) and `getFontFamilyString` /
`getFontString` (`common/src/utils.ts:123-147`), and records:

- `metadata`: `Object.entries(FONT_METADATA)` in JavaScript key order;
- `families`: `getLineHeight`, the generic fallback, the fallback list and
  `getFontFamilyString` for named, fallback, unused and custom ids;
- `lineHeightInPx`: `getLineHeightInPx` over sizes and unitless line heights;
- `verticalOffset`: `getVerticalOffset` for every id and size, with the
  family's own line height, other unitless ones and odd pixel heights;
- `fontString`: `getFontString` for every id at a few sizes.

```sh
node tools/goldens/font-metadata.mjs           # write the fixture
node tools/goldens/font-metadata.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/font-metadata.test.mjs`
checks that two runs are byte-identical and that the fixture holds upstream's
`textElement.test.ts:188-210` expectations. The Rust side
(`crates/excali-text/tests/font_metadata.rs`) compares every offset bit for bit.

## Text wrapping goldens

`text-wrapping.mjs` writes `crates/excali-text/tests/fixtures/text-wrapping.json`
for the wrapping port (ex-303). It bundles upstream's `parseTokens` and
`getWrappedTextLines` (`packages/element/src/textWrapping.ts`) with
`import.meta.env.MODE` set to `"test"`, as under upstream's vitest, so
`satisfiesWordInvariant` throws if a word with whitespace ever reaches
`wrapWord`. Widths come through upstream's own `setCustomTextMetricsProvider`
and `charWidth` cache (`textMeasurements.ts:106-119, 179-210`), cleared before
each call, from two providers:

- `chars10`: `text.length * 10`, the metric upstream's tests measure with;
- `varied`: per UTF-16 code unit `u`, `3 + (u * 7) % 11`, less 0.5 for each
  adjacent pair whose sum is a multiple of 5, so a line is not the sum of its
  characters and the cached single-character widths matter.

The texts are every input of `textWrapping.test.ts`, edge cases (JS `\s`
against Unicode `White_Space`, line terminators inside a line, emoji
sequences, NFD input, brackets and CJK punctuation) and 60 strings from a
seeded mulberry32 over the characters the break rules name. The fixture holds
`parseTokens` of every hard line and, per text, provider and width,
`getWrappedTextLines` as `[text, start, end]`. Non-visible code points are
written as `\uXXXX` escapes so the file passes the invisible-character gate.

```sh
node tools/goldens/text-wrapping.mjs           # write the fixture
node tools/goldens/text-wrapping.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/text-wrapping.test.mjs`
checks that two runs are byte-identical, that the fixture holds upstream's
`textWrapping.test.ts` expectations and that it has no invisible code point.
The Rust side (`crates/excali-text/tests/text_wrapping_goldens.rs`) compares
every token list and every line with its offsets.

## Font asset goldens

`font-assets.mjs` writes `crates/excali-text/tests/fixtures/font-assets.json`
for the font asset manifest and scene font loading (ex-307). It bundles
upstream's `Fonts` and `ExcalidrawFontFace` (`packages/excalidraw/fonts/`)
and `containsCJK` (`packages/element/src/textWrapping.ts:30-36`), with each
`.woff2` import resolved to its path under `fonts/` (`loadUpstream`'s
`fontUris` option), `FontFace` replaced by a class that records its arguments
(with the CSS Font Loading default `unicodeRange` "U+0-10FFFF"), and
`ExcalidrawFontFace#getContent`, which would fetch and subset the file,
answering with the file and code points it was asked for. It records:

- `registered`: `Fonts.registered` in its order, each face's file, CSS
  format, descriptors, `unicodeRange`, and `probes`, upstream's
  `getUnicodeRangeRegex()` tested either side of both ends of every range;
- `cjk`: `containsCJK` on every code point, as ranges;
- `scenes`: named element lists with `getUniqueFamilies`,
  `getCharacters` and `getFontString(family, FONT_SIZES.sm)` over all and
  over the non-deleted elements, and the faces
  `generateFontFaceDeclarations` inlines, in order.

```sh
node tools/goldens/font-assets.mjs           # write the fixture
node tools/goldens/font-assets.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/font-assets.test.mjs`
checks that two runs are byte-identical, that the registry names every font
file in upstream's font directories once, and a few facts from upstream's
sources. `scripts/fonts/assets.py` builds `crates/excali-text/assets/fonts/manifest.json`
from `registered`; the Rust side (`crates/excali-text/tests/font_assets.rs`)
checks the registry, every probe, the CJK table, the loads and the
declarations, and `tests/web` checks the load selection in Chromium.

## Font subsetting fixture

`font-subset.mjs` writes `tools/font-subset-eval/upstream-subsets.json` for
the SVG font subsetting decision (ex-408, ADR-010). It bundles upstream's
`Fonts` and `ExcalidrawFontFace` as `font-assets.mjs` does, but replaces only
`ExcalidrawFontFace#fetchFont`, which reads the face's file from the checkout
as upstream's `setupTests.ts:101-125` does. `getContent`, `subset-main` and
`subset-shared.chunk` (woff2 decompress, harfbuzzjs 0.3.6 hb-subset with
every layout feature, woff2 compress, upstream's inlined wasm) run unchanged,
on the main thread since Node has no `Worker`. For each scene it records the
text elements and every `@font-face` rule `generateFontFaceDeclarations`
writes: the family, the face's file, the code points it kept and the woff2.
It also records the bytes of the two wasm modules, raw and gzipped. The
export test scenes use upstream's test `FontFace` (every face `U+0000-00FF`,
`setupTests.ts:65-86`); the rest use each face's real range. The scenes are
the export test's, `font-assets.mjs`'s and drawings of ordinary size.
Non-ASCII characters are written as `\u` escapes, so U+00AD stays out of the
file.

```sh
node tools/goldens/font-subset.mjs           # write the fixture
node tools/goldens/font-subset.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/font-subset.test.mjs`
checks that two runs are byte-identical, that the export test scenes equal
`tests/scene/__snapshots__/export.test.ts.snap`'s `@font-face` rules byte for
byte, and that every rule is a woff2 smaller than its face. The Rust side is
`tools/font-subset-eval` (its tests and `rust.yml`'s `font-subset-eval` job)
and `crates/excali-svg/tests/writer.rs`.

## Library fixtures

`library-fixtures.mjs` writes `crates/excali-core/tests/fixtures/library.json`
for excali-core's `.excalidrawlib` port (ex-108). It bundles upstream's
`parseLibraryJSON` (`packages/excalidraw/data/blob.ts:218-228`, which checks
`isValidLibrary` and calls `restoreLibraryItems`, `data/restore.ts:1374-1415`),
`serializeLibraryAsJSON` (`data/json.ts:137-145`), `mergeLibraryItems` and
`getLibraryItemsHash` (`data/library.ts:122-157, 594-605`), and records:

- `parse`: the exact string `serializeLibraryAsJSON(parseLibraryJSON(input,
  defaultStatus))` returns, or the message of what the parse threw, for
  upstream's `fixture_library.excalidrawlib`, the inputs of `restore.test.ts`
  "restoreLibraryItems creation timestamps", and tables over the envelope
  (`libraryItems || library`, versions, non-iterable values), the items (v1
  arrays, missing, falsy and odd `id`/`status`/`created`, unknown keys, key
  order, lone surrogates) and the per-item `restoreElements(elements, null)`
  (deleted and unknown elements, duplicate ids, index sync, legacy fields);
- `catalogue`: for every library of `fixtures/libraries` (ex-003's manifest,
  232 files), the item count and the sha256 of the serialized parse, with
  `defaultStatus` `"published"` as an import from libraries.excalidraw.com,
  and `reload_sha256`, the sha256 of that output parsed and serialized once
  more (ex-114). The two differ for 59 libraries, for two reasons:
  - all 51 libraries with `geometry` (below): a legacy binding migrated with
    geometry is written as `{mode, elementId, fixedPoint}`
    (`restore.ts:412-416`); the next load reads a binding with a `mode` and
    rebuilds it as `{elementId, mode, fixedPoint}` (`restore.ts:338-342`),
    same values, new key order;
  - 9 libraries (8 without `geometry`, and `cloud/cloud`) hold a legacy
    `draw` element that became a `line` without `polygon`
    (`isLineElement` is false for `draw`, `restore.ts:645-651`): the next
    load adds `polygon: false`.

  Without the migration only the second applies: `reload_sha256_without_geometry`
  equals `output_sha256_without_geometry` except for `cloud/cloud`. The
  generator fails unless a further load leaves the reloaded file unchanged;
- `merge`: `mergeLibraryItems(local, other)` of two parsed libraries;
- `hash`: `getLibraryItemsHash` of parsed items.

`blob.ts` and `library.ts` pull in browser and React code (file dialogs,
image codecs, library item previews, jotai atoms) that these functions never
call. The loader replaces those modules with empty ones, and
`editor-jotai` with a shim whose `atom` returns a plain object, since
`library.ts` creates an atom while it loads (`shims` in `lib/upstream.mjs`).

A legacy arrow binding (no `mode`) whose target exists is migrated with
element geometry (`restore.ts:362-418`), which excali-core's restore asks its
environment for; excali-editor's `RoutingEnv` answers it (ex-116), and
`crates/excali-editor/tests/legacy_binding.rs` checks those cases against
`output`, `output_sha256` and `reload_sha256`. Cases that reach it record
`geometry`, the number of binding ends that did, and the output with that
computation failing
(`outputWithoutGeometry`, `output_sha256_without_geometry` for the
catalogue, with `reload_sha256_without_geometry` for its reload): the case
run again with `LinearElementEditor.getPointAtIndexGlobalCoordinates`
throwing, as in the restore fixtures.

Upstream runs in its test mode (`randomId()` gives `id0`, `id1`, ...,
restarted by `reseed(1)` before each parse; `getUpdatedTimestamp()` gives 1)
and `Date.now`, which `restoreLibraryItems` reads for `created`, returns 1,
so excali-core's `restore::TestEnv` answers both. `window.EXCALIDRAW_EXPORT_SOURCE`
is `https://excalidraw.com`. `Math.random` throws while generating.

```sh
node tools/goldens/library-fixtures.mjs           # write the fixture
node tools/goldens/library-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/library-fixtures.test.mjs`
checks that two runs are byte-identical.

## Library URL fixtures

`library-url-fixtures.mjs` writes
`crates/excali-core/tests/fixtures/library-url.json` for excali-core's
import-from-URL port (ex-109, `excali_core::library_url` and
`excali_core::link`). It bundles upstream's `validateLibraryUrl` and
`parseLibraryTokensFromUrl` (`packages/excalidraw/data/library.ts:497-543`),
the module-private allow-list `ALLOWED_LIBRARY_URLS` (`library.ts:54-58`,
exported for the generator), and `toValidURL` and `normalizeLink`
(`packages/common/src/url.ts:5-37`, with `@braintree/sanitize-url` 6.0.2
from `package-lock.json`). It records:

- `validate`: `validateLibraryUrl(url)` with the default allow-list, or with a
  case's own `allowList`: `ok`, or the thrown message and constructor
  (`Error`, `TypeError` for `new URL`, `SyntaxError` for an entry that is not
  a valid regular expression);
- `tokens`: `parseLibraryTokensFromUrl()` with `window.location` at `href`
  (its `search` and `hash` are recorded too);
- `normalizeLink` and `toValidURL` (with `location.origin` set) on tables of
  links: `javascript:`/`data:`/`vbscript:` in every disguise the sanitizer
  handles, character references, control characters, relative links;
- `import`: `decodeURIComponent`, `toValidURL` and `validateLibraryUrl` in the
  order `importLibraryFromURL` calls them (`library.ts:726-731`).

`library.ts` is loaded with the same stubs and jotai shim as the library
fixtures. The functions are pure apart from `location`.

```sh
node tools/goldens/library-url-fixtures.mjs           # write the fixture
node tools/goldens/library-url-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/library-url-fixtures.test.mjs` checks that two runs are byte-identical.

## URL host fixtures

`url-host-fixtures.mjs` writes `crates/excali-core/tests/fixtures/url-hosts.json`
for excali-core's `whatwg_url` (ex-109): what `new URL` gives in Node for
special URLs whose host is outside ASCII, where the domain goes through ada's
IDNA (`ada::idna::to_ascii`), which the port carries as `excali_core::ada_idna`.
It records:

- `cases`: hostname and pathname, or `error`, for hosts that show where ada
  differs from UTS 46 with Unicode 17 data (combining marks and right-to-left
  letters from Unicode 14 to 17, the Bidi rule per label, ContextJ, NFC,
  mapping, percent escapes, `xn--` labels, IPv4 after mapping, `file:`);
- `sweep`: a CRC-32 per 4,096 code points of the hostnames of
  `https://{c}/` and `https://a{c}/` for every code point from U+0080;
- `random`: CRC-32s over 20,000 URLs from a seeded Park-Miller generator
  whose tokens and schemes are in the fixture, so the Rust test rebuilds the
  same inputs.

It needs no upstream checkout. The output depends on the Node release, so the
generator refuses any but `.node-version` (26.10.0, ada 4.0.0).

```sh
node tools/goldens/url-host-fixtures.mjs           # write the fixture
node tools/goldens/url-host-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/url-host-fixtures.test.mjs`
checks that two runs are byte-identical. ada's table blob itself is taken
from the ada 4.0.0 release by `scripts/fixtures/ada-idna-tables.py`
(`--check` runs in the `ada-idna-tables` job of `gates.yml`).

## Fixture round-trip golden (D1)

`document-fixtures.mjs` writes
`crates/excali-core/tests/fixtures/document-round-trip.json` for excali-core's
D1 conformance test (`tests/fixture_round_trip.rs`, ex-g101). For every
scene-bearing file of upstream's `packages/excalidraw/tests/fixtures`
(`diagramFixture.ts`, `elementFixture.ts`, the PNG and SVG files with an
embedded scene, `fixture_library.excalidrawlib`) it records:

- `input`: the scene text upstream's loader parses. For `diagramFixture.ts`
  it is `JSON.stringify(diagramFixture, null, 2)` of the object the module
  exports, also with an unknown element key and an unknown top-level key
  added (`diagramFixture-unknown-keys`); for `elementFixture.ts` a scene of
  every exported element in source order; for an embedded PNG or SVG what
  upstream's `parseFileContents` (`data/blob.ts:32-80`, through
  `decodePngMetadata` and `decodeSvgBase64Payload`) returns for the file;
- `output`: `serializeAsJSON(elements, appState, files, "local")`
  (`data/json.ts:52-75`) of what `loadFromBlob(file, null, null)`
  (`data/blob.ts:137-216`) gives: `restoreElements` with `repairBindings` and
  `deleteInvisibleElements`, `restoreAppState` of the exported `appState`;
- `reload`: the same for `output` loaded as a `.excalidraw` file.

The library is `loadLibraryFromBlob` then `serializeLibraryAsJSON`, twice.
`edges` holds scenes that are not upstream fixtures, one per step of loading
and saving (files of live images only, `files` that is an array, a string, a number or
an object with an own `__proto__` key, falsy and malformed top-level values,
legacy `appState`, dropped and deleted elements, files the loader rejects).

The whole loader is upstream's: `blob.ts`, `image.ts`, `encode.ts` and
`scene/export.ts` from the checkout, with pako 2.0.3 and the png-chunk
packages at the tarballs upstream's `yarn.lock` locks (`test/pins.test.mjs`).
The file dialogs, image resizing, fonts and renderers are stubbed; loading a
scene without local state never reaches them. Node has no `FileReader`, which
`parseFileContents` uses to read a Blob as text, so the generator installs one
that reads it with `Blob#text()`. Upstream runs in its test mode with
`reseed(1)` before each load, `Date.now` returns 1 and `Math.random` throws.

```sh
node tools/goldens/document-fixtures.mjs           # write the fixture
node tools/goldens/document-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/document-fixtures.test.mjs`
checks that two runs are byte-identical and that every scene-bearing fixture
has a case.

## Elbow arrow routing fixture

`elbow-routing-fixtures.mjs` writes
`crates/excali-editor/tests/fixtures/elbow-routing.json` for excali-editor's
elbow arrow router (ex-211): upstream's `updateElbowArrowPoints(arrow,
arrayToMap(elements), updates)` (`packages/element/src/elbowArrow.ts:907-1167`)
with no options, on 578 scenes:

- `upstream-*`: the scenes of `packages/element/tests/elbowArrow.test.tsx`
  ("elbow arrow routing"), built with `API.createElement`;
- `unbound-*`: seeded random endpoints, collinear and coincident ones, arrows
  with more than two points and the 1e6 clamp;
- `bound-*`: seeded random scenes of one or two elements of every bindable
  type (sharp and rounded rectangles and diamonds, ellipses, text, image,
  frame, sticky notes, iframe, embeddable), rotated and not, with random fixed
  points, arrowheads and stroke widths, bound at one or both ends;
- `fixed-*`: fixed segments moved, released, renormalised, resized and held
  while an end is dragged, chained from routed arrows and from a five-segment
  staircase, unbound and bound on every side pairing, and a zero-length
  segment released between two fixed ones;
- `edge-*`: the early returns (fewer than two points, a missing or
  non-bindable target, an empty scene, the no-op short circuit);
- `throw-*`: where upstream throws: an endpoint drag on a fixed-segment arrow
  short of a third point (`handleEndpointDrag`, `elbowArrow.ts:752-757`),
  for `startIsSpecial` null, false and true. The generator's header explains
  why `handleEndpointDrag`'s "to last" throw and `handleSegmentRelease`'s
  "Property 'points' is required" cannot be reached through
  `updateElbowArrowPoints`; excali-editor unit-tests those guards alone.

Each case records the arrow, the scene, the updates (taken before the call:
`handleSegmentMove` writes into the segments it is given) and the returned
update, or for `throw-*` the exception's message as `error` in its place. A
`throw-*` case that stops throwing, or any other case that throws, fails
generation. Upstream runs in its test mode with `reseed(1)` before each case and
`Math.random` throwing; the scenes come from a Park-Miller generator seeded
per case.

```sh
node tools/goldens/elbow-routing-fixtures.mjs           # write the fixture
node tools/goldens/elbow-routing-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/elbow-routing-fixtures.test.mjs` checks that two runs are byte-identical.

## Hit testing fixture

`collision-fixtures.mjs` writes
`crates/excali-editor/tests/fixtures/collision.json` for excali-editor's hit
testing (ex-507): upstream's `packages/element/src/collision.ts` and
`distance.ts` on these scenes:

- `upstream-*`: the scenes of `packages/element/tests/collision.test.tsx`,
  built with `API.createElement` (the rotated arrow as `UI.createElement`
  draws it: round, with an arrow end head), and `newFreeDrawElement` where
  the test calls it;
- `random-*`: eight seeded scenes per element kind (sharp and round
  rectangles and diamonds, ellipses, text, text in a container, arrow
  labels, images, frames, magic frames, iframes, embeddables, sticky notes,
  open, looped and round lines, sharp, round and elbow arrows, open and
  looped freedraw at both variabilities), filled and transparent, rotated
  and not, each probed at twelve points (its outline, inside, beyond) with
  the threshold `getElementHitThreshold` gives at a random zoom, sometimes
  with `overrideShouldTestInside` or a frame name bound. A probe records
  `hitElementItself` (cache reset first), `distanceToElement`,
  `isPointInElement`, `shouldTestInside`, `hitElementBoundingBox`,
  `hitElementBoundText` and `hitElementBoundingBoxOnly`;
- `intersect-*`: `intersectElementWithLineSegment` with offsets 0, 3 and 6.5
  and `onlyFirst` on seeded segments through every element kind;
- `binding-*`: `getHoveredElementForBinding` and
  `getAllHoveredElementAtPoint` on seeded scenes of up to four bindable
  elements (locked ones, frames with children) at zooms 0.2 to 4;
- `inside-*`: `isBindableElementInsideOtherBindable` both ways on seeded
  pairs.

Upstream runs in its test mode with `reseed(1)` before each case.
`Math.random` throws, except inside `getFreedrawFillPolygon`, which draws its
curve with an unseeded `RoughGenerator` at roughness 0 (every draw is
multiplied by the roughness, so none reaches the output).

```sh
node tools/goldens/collision-fixtures.mjs           # write the fixture
node tools/goldens/collision-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/collision-fixtures.test.mjs` checks that two runs are byte-identical and
that the fixture holds `collision.test.tsx`'s answers.

## Linear element editor fixture

`linear-editor-fixtures.mjs` writes
`crates/excali-editor/tests/fixtures/linear-editor.json` for excali-editor's
linear element editor (ex-511): upstream's `LinearElementEditor`
(`packages/element/src/linearElementEditor.ts`) on ten hand-written lines
and arrows (two points, short segments, round, a tiny round curve, rotated,
a polygon, labelled straight, bent and round arrows, an elbow arrow) and
sixteen seeded random ones (sharp or round, rotated or not, some labelled).
Each case records the global points and `isPointHandle`; at zooms 0.5, 1
and 4, in and out of the editor, `getEditorMidPoints`, the
`isSegmentTooShort` call it makes per segment, `getPointIndexUnderCursor`
and `getSegmentMidpointHitCoords` at probes around every point and
midpoint (some with a hovered midpoint), and `getSegmentMidPointIndex`;
`shouldAddMidpoint` for seeded initial states; and, each on a fresh copy of
the scene, `addMidpoint` (with and without a grid), `deletePoints` (with and
without an uncommitted last point), `addPoints` and
`handleBoundTextDragging`, with the whole scene afterwards.

Upstream runs in its test mode with `reseed(1)` before each case and op;
text is 10 px per character and `Math.random` throws.

```sh
node tools/goldens/linear-editor-fixtures.mjs           # write the fixture
node tools/goldens/linear-editor-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/linear-editor-fixtures.test.mjs` checks that two runs are
byte-identical and that the fixture holds the handle and midpoint rules.

## Flowchart fixture

`flowchart-fixtures.mjs` writes
`crates/excali-editor/tests/fixtures/flowchart.json` for excali-editor's
flowchart creator and navigator (ex-534): upstream's
`packages/element/src/flowchart.ts` on seeded inputs of the module-private
`findNearestFreeSlot` and `placeCluster`; `FlowChartCreator.createNodes` step
by step (growing, turning, clearing) from rectangles, diamonds, ellipses and
sticky notes, next to flowcharts upstream's own creator grew, inside, across
and outside a frame, and on forty seeded random scenes, recording the pending
nodes and arrows whole and the scene elements the step changed;
`FlowChartNavigator.exploreByDirection` step by step on a tree, hand-bound
arrows and forty seeded flowcharts; and `isNodeInFlowchart`. Upstream runs in
its test mode, `reseed(1)` before each case's steps, and the ids a scene was
built with are renamed so the steps' `id0`, `id1`, ... are new.

```sh
node tools/goldens/flowchart-fixtures.mjs           # write the fixture
node tools/goldens/flowchart-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/flowchart-fixtures.test.mjs` checks that two runs are byte-identical
and that the fixture covers creation, the frame rule and the walk.

## Viewport fixture

`viewport.mjs` writes `crates/excali-editor/tests/fixtures/viewport.json` for
excali-editor's viewport (ex-505): upstream's own zoom and scroll code on
tables of states, with the stand-in App it needs:

- `constants`, `normalizedZoom`: `MIN_ZOOM`, `MAX_ZOOM`, `ZOOM_STEP`,
  `DEFAULT_OVERSCROLL` and `getNormalizedZoom` (non-finite inputs included);
- `coords`: `viewportCoordsToSceneCoords` and `sceneCoordsToViewportCoords`;
- `constrain`, `zoomAt`: `constrainScrollState` (every case of
  `tests/scrollConstraints.test.tsx`'s pure suites and more) and
  `getViewportForZoomWithScrollConstraints`;
- `translate`: `AppViewport.translate` on a stand-in App: the committed
  viewport and what it called (snap-back cancelled or scheduled, unfollow);
- `zoomToFitBounds` (bounds x states x offsets x fit, range and stepping
  options), `centerScrollOn`, `scrollBoundsIntoView` (including
  `tests/viewport.test.ts`), `getClosestElementBounds`,
  `getScrollToContentState` on a fixed set of elements;
- `wheel`: `AppWheel.handle` with upstream's `AppViewport`, event by event
  (prevented or not, what it called, the state after): the scenarios of
  `tests/wheel.test.tsx`, every target, navigation off, zero deltas, tick
  runs to both zoom limits, scroll and zoom locks, and a sweep of the zoom
  formula over ten zooms and twenty deltas;
- `actions`: `actionZoomIn`, `actionZoomOut`, `actionResetZoom`,
  `actionZoomToFit`, `actionZoomToFitSelection` and
  `actionZoomToFitSelectionInViewport` performed on states with and without
  locks, element sets, selections and UI offsets, and each `keyTest` on a
  table of key events.

`actionCanvas.tsx`'s React panels, `reactUtils` (React's batching) and
`register` are stubbed; nothing the recorded paths run reaches them. The
debounced snap-back and `AnimationController.cancel` of the snap-back are
recorded instead of run. The wheel formula takes `Math.log10`, which V8 on
arm64 computes with fused multiply-adds (`excali_math::js::log10`).

```sh
node tools/goldens/viewport.mjs           # write the fixture
node tools/goldens/viewport.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/viewport.test.mjs` checks
that two runs are byte-identical and restates the zoom rules independently.

## Transform fixture

`transform-fixtures.mjs` writes
`crates/excali-editor/tests/fixtures/transform.json` for excali-editor's
transform handles, resizing and rotation (ex-508): upstream's
`packages/element/src/transformHandles.ts`, `resizeTest.ts` and
`resizeElements.ts`:

- `handles-*`: `getTransformHandles` for every element kind (rotated or
  not, small and large, locked, elbow arrows, two-point lines in every
  direction, frames, images, a labelled arrow) at zooms 0.5, 1 and 3, for
  mouse, pen and touch, with the default, no, desktop, frame and a custom
  set of omitted handles; `handles-from-coords`: seeded
  `getTransformHandlesFromCoords` calls with margins and spacings;
- `resize-test-*`: `resizeTest` and `getElementWithTransformHandleType` on
  probes at every handle, along every side and away from the element, for
  desktop and phone editors; `handle-type-from-coords`:
  `getTransformHandleTypeFromCoords` on selection boxes;
- `cursors`, `has-bounding-box`, `resize-offset`:
  `getCursorForResizingElement`, `hasBoundingBox`, `getResizeOffsetXY` and
  `getResizeArrowDirection`;
- gestures (`kind: "session"`): the pointer pressed on a handle and moved,
  as `App.tsx` runs it (`handleSelectionOnPointerDown`,
  `maybeHandleResize` without snapping), recording every element each move
  changes: the scenes of `packages/element/tests/resize.test.tsx`
  (`upstream-*`), rotation (with a label, and with a deleted one), a lone
  multi-point line with `appState.selectedLinearElement` editing it or
  hovering a point (`line-selected-linear-*`, no handle), rotated elements
  on every handle and modifier,
  sticky notes, zoom and pointer types, the grid, groups, frames, bound and
  elbow arrows, and 160 seeded random scenes (`random-*`).

`transformElements` calls `updateBoundElements` (binding.ts, ex-510) and,
through `updateStickyNoteLayout`, `getStickyNoteLayout` (stickyNote.ts,
ex-703). Their call sites are rewritten (the `patch` option of
`loadUpstream`) to record each call's arguments and what it changed, which
the Rust test checks through its `TransformEnv`: a binding call is run by
the port's own `updateBoundElements` and its effect compared with the
recorded one, a sticky-note layout is answered with the recorded result. Text is
measured as `text.length * 10`, and the character width cache starts every
gesture with no font in it.

```sh
node tools/goldens/transform-fixtures.mjs           # write the fixture
node tools/goldens/transform-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/transform-fixtures.test.mjs` checks that two runs are byte-identical and
that the fixture holds `resize.test.tsx`'s answers.

## Binding fixture

`binding-fixtures.mjs` writes `crates/excali-editor/tests/fixtures/binding.json`
for excali-editor's arrow binding (ex-510): upstream's own
`packages/element/src/binding.ts`, the binding helpers of `utils.ts`
(`getAllMidpoints`, `getElbowArrowSnapMidPoint`, `getSnapOutlineMidPoint`,
`projectFixedPointOntoDiagonal`) and the binding highlight of
`packages/excalidraw/renderer/interactiveScene.ts`
(`renderBindingHighlightForBindableElement_simple`):

- `constants` and `normalizeFixedPoint`: the gap by stroke width, the
  binding distance by zoom, and `normalizeFixedPoint` / `isFixedPoint` on
  exact halves, near halves, out-of-range and non-finite ratios;
- per scene (hand-written: two rectangles, both ends inside one ellipse,
  tiny and rotated targets, elbow arrows, shapes in a frame, a labelled
  arrow; and 60 seeded random scenes of every bindable type with simple,
  curved, multi-point, elbow and labelled arrows, stale and missing
  `boundElements` entries and deleted arrows):
  - read-only queries: fixed points, `updateBoundPoint`,
    `bindPointToSnapToElementOutline`, `avoidRectangularCorner`,
    `snapBoundPointToGrid` (module-private, exported as it is), both
    `calculateFixedPointFor*ArrowBinding`, the midpoint helpers,
    `getBindingSideMidPoint`, and the binding strategies for dragged ends in
    the simple and the `COMPLEX_BINDINGS` flavour (with upstream's
    invariant messages where it throws);
  - mutating calls on a copy of the scene, recording every element each
    changes: `updateBoundElements` (after a move, resize or rotation, with
    `simultaneouslyUpdated` and `changedElements`), `bindBindingElement`,
    `bindBindingElementToFixedPoint`, `unbindBindingElement`,
    `bindOrUnbindBindingElement(s)`, `updateBindings`,
    `reanchorBindingsToOutline`, `fixBindingsAfterDeletion` and
    `fixDuplicatedBindingsAfterDuplication`;
  - the binding highlight of each bindable element and frame, on a context
    that records every call (`interactiveScene.ts` pulls in the editor's
    jotai store, shimmed as `library-fixtures.mjs` shims it);
- `history-*`: the scenes of `history.test.tsx`'s arrow cases (`:4578`,
  `:5107`) laid out as `ElementsDelta.redrawBoundArrows` lays them out after
  each redo, which `crates/excali-editor/tests/history.rs` compares its
  arrows with.

```sh
node tools/goldens/binding-fixtures.mjs           # write the fixture
node tools/goldens/binding-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/binding-fixtures.test.mjs` checks that two runs are byte-identical and
that the fixture holds `history.test.tsx`'s answer (the arrow's second point
rounds to `[500, -400]`).

## Snapping fixture

`snapping-fixtures.mjs` writes `crates/excali-editor/tests/fixtures/snapping.json`
for excali-editor's object snapping (ex-509): upstream's own
`packages/excalidraw/snapping.ts` and `renderer/renderSnaps.ts`:

- `getSnapDistance` (8 / zoom), `isActiveToolNonLinearSnappable` for every
  tool, and `isSnappingEnabled` over snap mode, grid mode, the lasso,
  Ctrl/Cmd, no event and a lone arrow;
- per scene (hand-written: aligned rows, horizontal and vertical gaps for
  each of the six gap snap directions, a gap grid, rotated diamonds and
  ellipses, groups and bound text, lines, arrows, text and freedraw, zoom
  and an offset viewport, the snap modes; and 80 seeded random scenes):
  `getElementsCorners`, `getReferenceSnapPoints`, `getVisibleGaps`,
  `snapDraggedElements` with the snap cache filled as `App.tsx` fills it,
  `snapResizingElements` on every handle, `snapNewElement` and
  `getSnapLinesAtPointer`;
- `renderSnaps` on a context that records every call, for each kind of snap
  line in light, dark and zen mode at several zooms and scrolls.

```sh
node tools/goldens/snapping-fixtures.mjs           # write the fixture
node tools/goldens/snapping-fixtures.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and
`test/snapping-fixtures.test.mjs` checks that two runs are byte-identical and
that the fixture covers every snap line kind and the three snap colours.

## Text editing fixture

`text-editing.mjs` writes `crates/excali-editor/tests/fixtures/text-editing.json`
for excali-editor's text editing (ex-512), which excali-ui's browser suite
(`tests/web/text-editing`) replays in Chromium too:

- `redraw`: `redrawTextBoundingBox(text, container, scene)`
  (`packages/element/src/textElement.ts:51-152`) on labels of every
  container type (rotated, narrowed, moved; arrow labels with and without a
  label position; sticky notes whose font steps down or which grow) and on
  free texts: the elements it changed and the original container cache;
- `sessions`: editing sessions through upstream's own
  `App.startTextEditing`, `handleTextWysiwyg` and the members they call,
  cut out of `components/App.tsx` at the pin (`APP_MEMBERS`) with the
  imports they use and compiled into a stand-in class over upstream's
  `Scene` and `AppViewport`, with `wysiwyg/textWysiwyg.tsx` unchanged. The
  steps replay what a browser delivers to the textarea (typed characters,
  keys with their default edits, inserted text, selections, blurs, pastes,
  the editor box scrolled to the caret, theme and canvas size changes, an
  element changed elsewhere); after each the record holds the textarea's
  value and selection, the style values assigned since (as the strings the
  CSSOM keeps), the elements changed (without the drawn `seed`,
  `versionNonce` and `updated`), the app state keys changed, what the
  editor asked of the app (`executeAction`, `scheduleCapture`,
  `focusContainer`, `translate`, ...) and the original container cache.

Runs in production mode under jsdom 22.1.0 with timers and animation
frames flushed after each step. React, the actions' panels and the modules
they pull in are stubbed; the actions index is redirected to
`actionExport.tsx`, where the two actions `textWysiwyg.tsx` takes from it
live. Ids come from a counter (`nanoid` is shimmed), the clock is fixed and
text measures 10 px per UTF-16 code unit.

```sh
node tools/goldens/text-editing.mjs           # write the fixture
node tools/goldens/text-editing.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/text-editing.test.mjs`
checks that two runs are byte-identical and restates the overlay's style
rules (5% height buffer, the transform formula, the textarea's attributes)
against every record.

## Image element fixture

`image-elements.mjs` writes `crates/excali-scene/tests/fixtures/image-elements.json`
for excali-scene's image elements (ex-404): upstream's own `renderElement`
(`packages/element/src/renderElement.ts:963-1009`) exporting image elements,
as `exportToCanvas` renders them (`isExporting`: translate, rotate, `scale`,
translate, `:1111-1190`; the image case of `drawElementOnCanvas`,
`:517-624`; `drawImagePlaceholder`, `:361-385`), under jsdom 22.1.0 with a
2D context that records every call. The cases are upstream's
`newImageElement`s: placeholders for a load in progress, an errored file, a
missing cache entry and an element without a file, in both themes, from a
4.8 px icon to the 100 px cap, rotated, mirrored and rounded; a PNG and an SVG
file at natural size, cropped, flipped on either axis, rotated, clipped by
every roundness, at 45% opacity and scrolled; the SVG in the dark theme.

The file holds the placeholders' SVG sources, the files the cases draw
(`crates/excali-raster/tests/fixtures/images/quad.png` and `shape.svg`, with
the natural size an `<img>` reports) and, per case, the element, theme, cache
state, scroll and calls. `crates/excali-scene/tests/image_elements.rs` plays
the calls on a model of the canvas state and compares the draws with the
port's display list, and writes the raster fixtures `image-elements.json` and
`image-elements-svg.json` in which Chrome replays the recorded calls.

```sh
node tools/goldens/image-elements.mjs           # write the fixture
node tools/goldens/image-elements.mjs --check   # exit 1 if it is stale
```

CI runs `--check` in the `goldens` job, and `test/image-elements.test.mjs`
checks that two runs are byte-identical and restates the placeholder colours,
the icon size rule, the crop, the order of rotate and scale, the corner radii
and the SVG-only dark filter from the recorded calls.

## Main menu fixture

`main-menu.mjs` writes `crates/excali-ui/tests/fixtures/main-menu.json` and
`crates/excali-ui/src/main_menu.css` for excali-ui's main menu (ex-520):
upstream's own `components/main-menu/MainMenu.tsx`, the default menu of
`LayerUI.tsx:111-136` and the items of `main-menu/DefaultItems.tsx` over the
`components/dropdownMenu/*` components, with upstream's actions deciding
which items show:

- `menus`: the default menu for app states (closed, dark, view mode, a file
  handle, an element link dialog), trimmed `UIOptions.canvasActions`, a
  non-empty scene and the phone form factor;
- `items`: ToggleTheme with the Light/Dark/System radio for each theme (and
  without `onThemeChange`), the Preferences submenu for several app states,
  CommandPalette and LiveCollaborationTrigger;
- per tree, each handler's effects (`executeAction`, `setAppState`,
  `toggleLock`, confirm dialogs, `onThemeChange`, `trackEvent`), and the
  English strings the menu shows.

radix-ui's DropdownMenu is shimmed to the DOM it leaves for upstream's
markup (roles, `aria-haspopup`, `aria-expanded`; not its ids, data-state or
placement). The stylesheet is `DropdownMenu.scss` and `DefaultItems.scss`
compiled with sass 1.51.0.

```sh
node tools/goldens/main-menu.mjs           # write the fixture and CSS
node tools/goldens/main-menu.mjs --check   # exit 1 if either is stale
```

CI runs `--check` in the `goldens` job, and `test/main-menu.test.mjs` checks
that two runs are byte-identical and restates research 3.3: the default
items, their shortcuts, the theme radio's light/dark/system choices and the
Preferences submenu's items.
