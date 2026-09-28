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
| `freehand.json` | `getStrokePoints` and `getStroke` with Excalidraw's options, the library defaults, and `edge/` cases for the branches Excalidraw never reaches (taper `true`/`false`, flat caps, cap easings, one-point strokes with a taper, `{x, y, pressure}` points, missing and negative pressures, reversals, duplicates, size 0, no points) | ex-213 |
| `laser-pointer.json` | the vendored `@excalidraw/laser-pointer` (`packages/laser-pointer/src`): `excalidraw/` cases with `getConstantWidthFreedrawOutline`'s options (size `sw * 1.4`, simplify 0, `sizeMapping` `max(0.1, pressure)`, pressure 1) for the freedraw fixture points, and `edge/` cases for the rest of the library (defaults, `output`/`input`/`tail` simplify phases and the tail's `Not implemented yet` throw, corners at both speeds, zero sizes, `keepHead`, size overrides, one and two points, duplicates, no points); `sizeMapping` is named (`SIZE_MAPPINGS` in `fixtures.mjs`) | ex-214 |
| `math.json` | every `packages/math/src` export except `pca.ts`, called on fixed and Park-Miller-random inputs (`math.mjs`): `{ id, fn, args, result }`; the `curve.ts` cases (including the `curveLength` fixtures) are ex-202's | ex-201, ex-202 |
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
tokens (freedraw `svgPath` shapes) are reported the same way. Numbers are
exact unless the case goes through platform trigonometry, which is compared
within `PLATFORM_TOLERANCE` (relative 1e-10). `Manifest` checks every file
against `manifest.json` (sha256, case count) and the upstream commit against
`site/config.toml` (`crates/excali-rough/tests/goldens_manifest.rs`).

The parity tests always build with the feature (excali-rough lists itself as
a dev-dependency with `goldens`, excali-scene lists excali-rough with it), so
a plain `cargo test --workspace` runs every golden. CI also runs the golden
targets on arm64, where the goldens were generated:

```sh
cargo test --workspace --features goldens --test goldens --test golden_harness --test goldens_manifest
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
replaced by one that throws.

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
arrow fix-ups and the inputs upstream throws on. Each case records the elements
in order, or the message thrown. The environment is the one of
`restore-fixtures.mjs` (test mode, `reseed(1)` before each case, text at 10 px per
character).

Three steps need geometry or text measurement that excali-core takes from its
`RestoreEnv`: `updateElbowArrowPoints`, `refreshTextDimensions` and
`getStickyNoteLayout`. The loader's `patch` option (`lib/upstream.mjs`) routes the
one call of each in `restore.ts` through a probe that calls upstream's function
and records its arguments and result in the case's `hooks`, in call order;
nothing else in the module changes. excali-core's fixture test checks that the
port asks for the same calls and answers them with the recorded results.

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
environment for (ex-116). Cases that reach it record `geometry`, the number
of binding ends that did, and the output with that computation failing
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
