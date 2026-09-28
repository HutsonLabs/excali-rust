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
agree byte for byte), and the CI job runs on `ubuntu-24.04-arm`. On x86_64,
V8's floating-point results for some shapes differ in the last bits, so
`--check` and the stability test report stale files there. Regenerate goldens
on an arm64 machine only.

## Files

| file | contents | used by |
|---|---|---|
| `random.json` | `Random.next()` sequences for 7 seeds | ex-203 |
| `rough-primitives.json` | `line`, `rectangle`, `polygon`, `ellipse`, `circle`, `arc`, `curve`, `linearPath`, `path` × seeds 1, 7, 1041657908 × roughness 0, 1, 2 | ex-203, ex-206 |
| `rough-generator.json` | generator edge cases × roughness 0, 1, 2: SVG path syntax (implicit commands, missing `M`, compact numbers, `S`/`T`, arcs), `simplification`, `stroke: "none"`, single stroke, preserveVertices, short point lists, line length bands, ellipse/arc geometry, seed wrap-around | ex-203 |
| `rough-fills.json` | hachure, cross-hatch, zigzag, solid, dashed, zigzag-line at `fillWeight = sw/2`, `hachureGap = sw*4` | ex-204 |
| `rough-options.json` | multi-stroke, preserveVertices, curveFitting, bowing, dashes, hachure angle, and the dashed/dotted stroke rule | ex-205 |
| `elements-upstream-fixtures.json` | upstream `tests/fixtures/elementFixture.ts` and the export test's 100×100 variants | all |
| `elements-rectangle.json`, `elements-diamond.json`, `elements-ellipse.json` | seeds × roughness, fills, stroke styles, adjustRoughness sizes, corner radius, dark theme | ex-208 |
| `elements-line.json`, `elements-arrow.json` | linearPath, filled polygon loops, curves | ex-209 |
| `elements-elbow-arrow.json` | elbow paths (radius 16), extreme-coordinate guard | ex-210 |
| `elements-arrowheads.json` | all 14 arrowheads, start and end, sw 1/2/4, curved, dashed, dotted, short, outline fills | ex-212 |
| `elements-freedraw.json` | perfect-freehand and laser-pointer outlines, trimmed SVG path, loop fills | ex-213, ex-214 |
| `elements-iframe-like.json` | `modifyIframeLikeForRoughOptions` placeholders and defaults | ex-208 |
| `freehand.json` | `getStrokePoints` and `getStroke` with Excalidraw's options and the library defaults | ex-213 |
| `math.json` | every `packages/math/src` export except `curve.ts` and `pca.ts`, called on fixed and Park-Miller-random inputs (`math.mjs`): `{ id, fn, args, result }` | ex-201 |
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
  `new RoughGenerator()[method](...args, options)`.
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
