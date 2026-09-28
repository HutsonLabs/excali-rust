+++
title = "Rendering, interaction and architecture"
description = "Canvas layering, rough.js option mapping, per-element drawing constants, math package, scene/state/undo model, interaction subsystems, export, fonts, dependencies, tests, and size."
weight = 20
[extra]
source_commit = "438d89861f53d8a90ad566113ecac1b83761098f"
source_repo = "https://github.com/excalidraw/excalidraw"
captured = "2026-09-28"
+++

Checkout: commit `438d898`. Paths are relative to `<checkout>`. Each claim cites `path:line`.

**Where this commit differs from what you might expect:**
- There is no `math/src/arc.ts`. The math package has `line.ts`, `triangle.ts` and `pca.ts` instead.
- There is a `stickynote` element type. It is drawn directly on the canvas, not through roughjs.
- Freedraw has two stroke engines, chosen by `strokeOptions.variability`: perfect-freehand for variable width, and the in-repo laser-pointer package for constant width.
- Arrow bindings use `FixedPointBinding` with a `mode` field.
- `node_modules` is not installed, so roughjs source is not in the checkout. The roughjs internals I describe (default options, RNG formula) come from my knowledge of roughjs 4.6.4, not from this repo. Check them against the upstream source before relying on them.

---

## 1. Rendering pipeline

### Canvas layering
- App.tsx mounts `<SVGLayer>`, `<StaticCanvas>`, `<NewElementCanvas>` and `<InteractiveCanvas>` in that order (`packages/excalidraw/components/App.tsx:2537, 2654, 2693, 2724`).
- Each canvas's backing size is `appState.width * scale` by `appState.height * scale`, where `scale = window.devicePixelRatio` (`components/canvases/StaticCanvas.tsx:40-41`, `InteractiveCanvas.tsx:149, 208-209`, `NewElementCanvas.tsx:56-57`).

**Static scene** (`packages/excalidraw/renderer/staticScene.ts`):
- Order of work:
  1. Snap the scroll to whole device pixels, except when exporting (`:290-292`).
  2. `bootstrapCanvas` (`:299`).
  3. Scale by zoom (`:310`).
  4. Draw the grid (`:313-326`).
  5. Draw non-iframe elements, each followed by its bound text (`:396-452`).
  6. Draw iframes/embeddables on top (`:455-505`).
  7. Draw pending flowchart nodes (`:508-522`).
- Frame clipping uses a `roundRect` with radius `FRAME_STYLE.radius / zoom` (`:165-189`).
- Link icons are drawn from their own cached canvases (`:201-273`).
- Grid:
  - Colours: light bold `#dddddd`, light regular `#e5e5e5`; dark uses the dark filter of those (`:57-66`).
  - Regular lines are skipped when `gridSize * zoom < 10` (`:124`).
  - Dash is `[lw*3, space + (lw + space)]` with `space = 1/zoom` (`:87, 130`).
  - Bold lines are up to 4 CSS px wide; regular lines 1 px (`:128`).
  - Lines are snapped to device pixels (`:98-115`).
  - Defaults: `DEFAULT_GRID_SIZE = 20`, `DEFAULT_GRID_STEP = 5` (`common/src/constants.ts:293-294`).
- Throttled with rAF (`staticScene.ts:526`).

**Helpers** (`renderer/helpers.ts`):
- `bootstrapCanvas`: `setTransform(1,0,0,1,0,0)`, then `scale(dpr)`. It clears unless the background is an opaque hex colour, then fills the background through the dark filter (`:73-127`).
- `snapScrollToDevicePixels`: `round(scroll * zoom * dpr) / (zoom * dpr)` (`:47-63`).
- `DEFAULT_SELECTION_COLOR = "#6965db"` (`:6`).

**Interactive scene** (`renderer/interactiveScene.ts`, 2166 lines; main function `_renderInteractiveScene` at `:1614`):
- Draws linear point handles, the selection rectangle, selection borders, transform handles, binding highlights, snap lines, and collaborator cursors.
- Selection border: dashes of `8/zoom` with gaps of `4/zoom`; padding `DEFAULT_TRANSFORM_HANDLE_SPACING * 2`; line width `1/zoom` (4 for an active embeddable) (`:920-973`).
- Transform handles: rounded rect with radius `2/zoom`; the rotation handle is a circle (`:1328-1369`).
- Colour constants:
  - Binding highlight RGB: `106,189,252` (light) and `104,182,240` (dark) (`:124-127`).
  - Search match colours: `:134-144`.
  - Snap lines: `#ff6b6b` / `#ff9090` / zen `#da5b5b`, width 1, cross size 2 (`renderer/renderSnaps.ts:8-14`).
  - Scrollbars: width 6, margin 4, colour `rgba(0,0,0,0.3)` (`scene/scrollbars.ts:10-12`).
- Selection rectangle while dragging: fill `rgba(0,0,200,0.04)`, 1/zoom stroke drawn at a 0.5/zoom offset (`packages/element/src/renderElement.ts:936-961`).

**Visibility culling:**
- `Renderer` class in `packages/excalidraw/scene/Renderer.ts:42`, using `isElementInViewport` (`packages/element/src/sizeHelpers.ts:80-115`), which is an AABB test against the viewport converted to scene coordinates.
- Coordinate transforms: scene = `(client - offset) / zoom - scroll` (`common/src/utils.ts:333-334`); the inverse is at `:355-356`.

### Per-element bitmap cache (`packages/element/src/renderElement.ts`)
- Outside export, every element except frames is rasterised once into its own offscreen canvas (`generateElementCanvas`, `:271-339`) and then blitted (`drawElementFromCanvas`, `:762-934`).
- Cache: `elementWithCanvasCache` is a WeakMap keyed by element object (`:682`). It regenerates when zoom (unless `shouldCacheIgnoreZoom`), theme, image crop or containing-frame opacity changes (`:687-728`).
- Padding around the element: freedraw `strokeWidth * 12`; text `fontSize / 2`; arrow 40 (20 with no arrowhead); everything else 20 (`:102-116`).
- Size caps: area 16777216, side 32767 (`:231-233`).
- Blit rules:
  - Snap to whole device pixels and turn image smoothing off when the angle is 0 or a right angle and no zoom gesture is running (`:747-752, 848-900, 1209-1218`).
  - `SNAP_TIE_BIAS = 1e-6` breaks rounding ties (`:760`).
  - Arrow labels punch an even-odd clip "hole" of label size + `BOUND_TEXT_PADDING` (5) (`:786-818`).
- When exporting, elements are drawn as vectors directly (`:1111-1190`).
- Opacity = `frameOpacity * elementOpacity / 10000`, multiplied by `ELEMENT_READY_TO_ERASE_OPACITY` (20) / 100 while an element is pending erase (`:185-194`; constant at `common/src/constants.ts:437`).

### ShapeCache and roughjs (`packages/element/src/shape.ts`)
- `ShapeCache` holds a static `RoughGenerator` and a WeakMap `element → {shape, theme}` (`:83-166`). It always regenerates when exporting (`:132`).
- `mutateElement` evicts the cache only when width, height, fileId or points change (`mutateElement.ts:132-140`). Other style changes go through `newElementWith`, which creates a new object, so the WeakMap key changes.

**`generateRoughOptions`** (`shape.ts:195-260`):

| option | value |
|---|---|
| `seed` | `element.seed` (`:201`) |
| `strokeLineDash` | dashed: `[8, 8 + sw]` (`:168`); dotted: `[1.5, 6 + sw]` (`:170`); solid: undefined (`:202-207`) |
| `disableMultiStroke` | `strokeStyle !== "solid"` (`:210`) |
| `strokeWidth` | `sw + 0.5` if not solid, else `sw` (`:213-216`) |
| `fillWeight` | `sw / 2` (`:220`) |
| `hachureGap` | `sw * 4` (`:221`) |
| `roughness` | `adjustRoughness(el)` (`:222`) |
| `stroke` | strokeColor through the dark filter (`:223`) |
| `preserveVertices` | `continuousPath \|\| roughness < 2` (`:224-225`) |
| `fillStyle` / `fill` | rect, iframe, embeddable, diamond, ellipse: `element.fillStyle`, and fill is undefined when transparent (`:229-237`) |
| `curveFitting` | `1` for ellipse only (`:238-240`) |
| line/freedraw fill | only when `isPathALoop(points)` (`:243-252`) |
| arrow | no fill (`:254-255`) |

- `adjustRoughness` (`:172-193`) keeps the roughness unchanged when any of these hold:
  - min side ≥ 20 and max side ≥ 50;
  - the element is rounded and min side ≥ 15;
  - it is a linear element with max side ≥ 50.
  
  Otherwise it returns `min(roughness / (maxSize < 10 ? 3 : 2), 2.5)`.
- Roughness presets: architect 0, artist 1, cartoonist 2 (`common/src/constants.ts:466-470`).
- Fill style type: `"hachure" | "cross-hatch" | "solid" | "zigzag"` (`element/src/types.ts:19`). The value is passed straight to roughjs.
- Default element props: stroke `#1e1e1e`, fill `solid`, strokeWidth 2, roughness 1, opacity 100 (`constants.ts:514-532`; black defined at `colors.ts:195`).
- Stroke widths: thin 1, medium 2, bold 4. Freedraw uses half of each (`constants.ts:480-501`).

**Determinism:**
- Each element stores a `seed` (`element/src/types.ts:56-58`), created as `randomInteger()` = `floor(random.next() * 2^31)` (`common/src/random.ts:9`; used at `newElement.ts:159`).
- `random` is roughjs's own `Random` class (`common/src/random.ts:2`).
- External, not in repo: roughjs `Random.next()` is the Park–Miller LCG `((2^31-1) & (seed = imul(48271, seed))) / 2^31`, and falls back to `Math.random` when seed is 0. The comment at `random.ts:21-24` confirms roughjs's generator "degenerates for seed 0".
- Tests call `reseed(7)` (for example `tests/App.test.tsx:17`).
- External, not in repo: roughjs 4.6.4 defaults are maxRandomnessOffset 2, bowing 1, curveTightness 0, curveFitting 0.95, curveStepCount 9, hachureAngle -41, fillShapeRoughnessGain 0.8, dashOffset/dashGap/zigzagOffset -1.

**Collision shapes:** built with `roughness: 0, disableMultiStroke: true, disableMultiStrokeFill: true, preserveVertices: true` (`shape.ts:629-635`).

**Dark mode:** colours go through CSS `invert(93%) hue-rotate(180deg)` computed numerically with tinycolor (`common/src/colors.ts:16-17, 86-116`; `DARK_THEME_FILTER` at `constants.ts:204`).

## 2. Element-specific drawing

- **Rectangle / iframe / embeddable** (`shape.ts:787-832`):
  - Rounded: an SVG path `M r 0 L w-r 0 Q w 0, w r …` drawn with `generator.path`, `continuousPath = true`.
  - Otherwise `generator.rectangle(0,0,w,h)`.
  - Iframe-like with transparent colours: roughness 0, background `#d3d3d3`, solid fill. Iframe defaults: stroke `#000000`, background `#f4f4f6` (`:262-293`).
- **Corner radius** (`element/src/utils.ts:528-549`):
  - PROPORTIONAL (2) or LEGACY (1): `x * 0.25`.
  - ADAPTIVE (3): `x * 0.25` when `x ≤ R / 0.25`, otherwise `R`, where `R = roundness.value ?? 32`.
  - Otherwise 0.
  - Constants at `constants.ts:443-464`.
- **Diamond:**
  - Points: `topX = floor(w/2) + 1`, `rightY = floor(h/2) + 1` (`bounds.ts:522-535`).
  - Rounded version uses cubic `C` corners with vertical/horizontal radii from `getCornerRadius` (`shape.ts:833-879`).
  - Sharp version uses `generator.polygon`.
- **Ellipse:** `generator.ellipse(w/2, h/2, w, h)` with `curveFitting = 1` (`shape.ts:880-889`).
- **Line / arrow** (`shape.ts:890-975`):
  - Elbow arrow: path with corner radius 16 (`generateElbowArrowShape`, `:1018-1081`); not drawn if any |coord| > 1e6.
  - Sharp: `linearPath`, or `polygon` when filled.
  - Round: `generator.curve(points)`.
  - Arrowheads are pushed after the main shape.
- **Arrowhead shapes** (`shape.ts:371-577`):
  - Line-type heads use roughness ≤ 1 and always a solid dash, except dotted heads, which use `[d0, d1 - 1]` from `getDashArrayDotted(sw - 1)` (`:326-343`).
  - Circles use roughness ≤ 0.5 (`:345-369`).
  - Outline variants fill with the canvas background colour.
  - Offsets: cardinality one-or-many -0.25; zero circle scale 0.8 (`:390-391`).
- **Arrowhead geometry** (`bounds.ts:710-909`):
  - Sizes: arrow 25; diamond 12; crowfoot 15; cardinality marker 20; default 15 (`:714-732`).
  - Angles: bar 90°, arrow 20°, default 25° (`:735-744`).
  - Direction comes from evaluating the last bezier op at t = 0.3 (`:790-810`).
  - `minSize = min(size, lastSegmentLength * (diamond ? 0.25 : 0.5))` (`:834-836`).
  - Circle diameter = `hypot + strokeWidth - 2` (`:843`).
  - Wing points are rotated by ±angle (`:868-877`).
- **Freedraw** (`shape.ts:976-995, 1187-1344`):
  - Fill (closed loop only): `generator.curve(simplify(points, 0.75))` with `stroke: "none"`.
  - Stroke is an SVG path string filled with the stroke colour (`renderElement.ts:496-516`).
  - Variable width: perfect-freehand `getStroke` with `size = sw * 4.25`, `thinning 0.6`, `smoothing 0.5`, `streamline = strokeOptions.streamline ?? 0.5`, easing `sin(t·π/2)`, `last: true`, `simulatePressure` from the element (`:1200-1245`; `DEFAULT_STROKE_STREAMLINE` at `constants.ts:622`).
  - Constant width: `LaserPointer` with `size = sw * 1.4`, `simplify 0`, `sizeMapping = max(0.1, pressure)`, pressure fixed at 1 (`:1207-1268`).
  - Outline to path: `M p0 Q p_i mid(p_i, p_i+1) … L p0 Z`, numbers trimmed to 2 decimals by regex (`:1314-1344`).
- **Text:**
  - Canvas: `fillText` per line at `y = i * lineHeightPx + verticalOffset`, with `textAlign` giving x offset 0, w/2 or w (`renderElement.ts:626-674`).
  - `lineHeightPx = fontSize * lineHeight` (`textMeasurements.ts:91-96`).
  - `verticalOffset = fontSizeEm * ascender + (lineHeightPx - fontSizeEm * ascender + fontSizeEm * descender) / 2`, where `fontSizeEm = fontSize / unitsPerEm` (`common/src/font-metadata.ts:155-172`).
  - Font string: `${fontSize}px ${Family}, fallbacks…` (`common/src/utils.ts:123-147`).
  - Tabs become 8 spaces (`textMeasurements.ts:64-70`).
  - Width comes from the canvas `measureText().width` (advance width) (`:135-149`).
  - Wrapping, with CJK and emoji handling: `element/src/textWrapping.ts:397` (`wrapText`).
- **Bound text:**
  - Padding is `BOUND_TEXT_PADDING = 5` (`constants.ts:421`).
  - Container insets: ellipse `+ (w/2)(1 - √2/2)`; diamond `+ w/4` (`textElement.ts:396-414`).
  - Max width for an ellipse: `round(w/2 · √2) - 10` (`:555`).
  - Arrow labels: max width `max(0.7 * w, fontSize * 11)` (`:545-549`).
- **Image** (`renderElement.ts:517-624`):
  - `drawImage` with `crop` (or natural size).
  - Rounded images are clipped with `roundRect(getCornerRadius(min(w, h)))`.
  - SVG images in dark mode get `DARK_THEME_FILTER`; Safari inverts pixels manually instead.
  - Placeholder: background `#E7E7E7` (light) or `#2E2E2E` (dark), icon size `min(min(w, h) * 0.4, 100)` (`:361-385`).
  - `scale` is applied after rotation to mirror the image (`:1181-1184`).
- **Frame / magicframe** (`renderElement.ts:1022-1064`):
  - Stroke `#bbb`, width `2/zoom`, radius `8/zoom`.
  - Magic frames use stroke `#7affd7` (light).
  - `FRAME_STYLE` holds name font size 14, offset 3, line height 1.25, name colours `#999999` / `#7a7a7a` (`constants.ts:206-220`).
  - In the editor, frame names are DOM elements (`App.tsx:2148, 2278, 2316`). On export they become Helvetica text elements (`scene/export.ts:105-135`).
- **Embeddable placeholder label:** Helvetica, `fontSize = max(min(w/2, w/len), w/30)`, text "Empty Web-Embed" or the link (`element/src/embeddable.ts:402-437`).
- **Sticky note** (not roughjs):
  - Painted in order: shadow (opacity 0.16), fill, clipped edge stroke (width 0.5·2, opacity 0.08), then a date footer in 12px Helvetica (`renderElement.ts:387-472`; constants `constants.ts:224-267`).
  - Roughness array `[0, 1.5, 8]`, corner radius ratio 0.04 up to a maximum of 16 (`element/src/stickyNote.ts:63-65`).
  - Uses a mulberry32 PRNG (`common/src/random.ts:25-34`).

## 3. Math package (`packages/math/src`, 2379 non-test LOC)

- **`types.ts`:** branded types `Radians` (`:9`), `GlobalPoint` (`:34`) and `LocalPoint` (`:52`) as `[x, y]` tuples, `Curve` (a 4-point cubic, `:134`), `Ellipse` (`:154`), plus LineSegment, Vector, Polygon, Rectangle and Triangle.
- **`point.ts`** (`:22-257`): pointFrom, pointRotateRads/Degs, pointTranslate, pointCenter, pointDistance(+Sq), pointScaleFromOrigin, isPointWithinBounds, pointsEqual.
- **`vector.ts`** (`:10-160`): vector, fromPoint, cross, dot, add, subtract, scale, magnitude(+Sq), normalize, normal.
- **`curve.ts`** (`:15-516`): curve, bezierEquation, curveIntersectLineSegment (`:168`), curveClosestParameter/Point (`:277`), curvePointDistance, curveTangent, Catmull-Rom quadratic/cubic approximation points, curveOffsetPoints, curveLength (`:450`) using the Legendre–Gauss N=24 tables in `constants.ts`, curvePointAtLength.
- **`segment.ts`** (`:28-185`): lineSegment, rotate, segmentsIntersectAt, pointOnLineSegment, distanceToLineSegment (`:119`), intersection points, closest parameter.
- **`polygon.ts`** (`:6-162`): polygonIncludesPoint (even-odd, `:18`) and NonZero, polygonSignedArea/area, convexHull, simplifyConvexPolygon.
- **`ellipse.ts`** (`:33-210`): includes/touches point, `ellipseDistanceFromPoint` (`:88`), segment/line intercepts.
- **`rectangle.ts`** (`:6-32`): rectangle, intersect segment, intersect rectangle.
- **`angle.ts`** (`:11-64`): normalizeRadians, cartesian2Polar, deg↔rad, isRightAngleRads, radiansBetweenAngles, radiansDifference.
- **`range.ts`** (`:12-78`): inclusive ranges, overlap, intersection.
- **`line.ts`** (`:11, 23`): infinite line and intersection.
- **`triangle.ts`** (`:14`): triangleIncludesPoint.
- **`pca.ts`** (`:37-200`): principal axes, elongation, skewness, kurtosis (used by shape recognition).
- **`utils.ts`**: `PRECISION = 10e-5`, clamp, round, roundToStep, isCloseTo (`:1`).

## 4. Scene and state

- **Element base fields** (`element/src/types.ts:40-86`): id, x, y, stroke/background colour, fillStyle, strokeWidth, strokeStyle, `roundness: {type, value?}`, roughness, opacity, width, height, angle, **seed**, **version**, **versionNonce**, **index** (fractional), isDeleted, groupIds, frameId, boundElements, **updated**, created, link, locked, customData.
- **Element types:**
  - Union at `:223-234`.
  - Text fields: `:253-291`.
  - Linear: `points`, `start/endBinding: FixedPointBinding {elementId, fixedPoint, mode}`, arrowheads (`:320-378`).
  - Elbow arrow: `fixedSegments`, `startIsSpecial`, `endIsSpecial` (`:396-420`).
  - Freedraw: `points`, `pressures`, `simulatePressure`, `strokeOptions {variability, streamline}` (`:422-437`).
- **Version bump:** `mutateElement` sets `version + 1`, `versionNonce = randomInteger()`, `updated = now` (`mutateElement.ts:142-144, 177-179, 193-194`).
- **Collaboration reconcile:** the remote element is discarded if the local one is being edited, or `local.version > remote.version`, or the versions are equal and `local.versionNonce <= remote.versionNonce` (`packages/excalidraw/data/reconcile.ts:23-40`).
- **Ordering:**
  - Vendored fractional-indexing with base-62 digits (`packages/fractional-indexing/src/index.ts:5, 212, 284`).
  - Sync and validation: `element/src/fractionalIndex.ts:49` (validate), `:150` (orderBy), `:175` (syncMoved), `:223` (syncInvalid).
- **Scene class** (`element/src/Scene.ts:108`):
  - Holds elements, the non-deleted list and map, and frames.
  - `replaceAllElements` runs `syncInvalidIndices` (`:271-300`).
  - `triggerUpdate` sets a random `sceneNonce` and notifies callbacks (`:303-310`).
  - `insertElementsAtIndex` (`:342-369`).
- **AppState:**
  - Interface: `packages/excalidraw/types.ts:326`.
  - Canvas-specific subsets: `StaticCanvasAppState` (`:205`) and `InteractiveCanvasAppState` (`:223`).
  - Defaults (`packages/excalidraw/appState.ts:24-140+`): zoom 1, scroll 0/0, `viewBackgroundColor` `#ffffff`, `frameRendering {enabled, clip, name, outline}` all true, exportScale = dpr if it is in [1,2,3], gridSize 20, gridStep 5, `currentItemRoundness` round.
  - Per-key persistence config: `APP_STATE_STORAGE_CONF` (`:153`).
- **Zoom:**
  - `MIN_ZOOM 0.1`, `MAX_ZOOM 30`, `ZOOM_STEP 0.1` (`constants.ts:362-364`).
  - Normalised by `clamp(round(z, 6), …)` (`scene/normalize.ts:7-9`).
  - Wheel: `newZoom = z - delta/100 + log10(max(1, z)) * -sign * min(1, |Δ|/20)`, with Δ capped at 10 (`components/App.wheel.ts:123-150`).
- **Undo/redo:**
  - `CaptureUpdateAction` is `IMMEDIATELY | NEVER | EVENTUALLY` (`element/src/store.ts:38-69`).
  - The `Store` (`:78`) compares `StoreSnapshot`s (`:643`) and emits Durable or Ephemeral increments (`:454-495`).
  - `StoreDelta {id, elements: ElementsDelta, appState: AppStateDelta}` (`:497-503`), with squash/inverse/applyTo (`:566-611`).
  - `Delta<T> {deleted: Partial<T>, inserted: Partial<T>}` (`element/src/delta.ts:77-81`).
  - `ElementsDelta` has `added`, `removed` and `updated` records (`:1034-1039`); `AppStateDelta` is at `:526`.
  - `History` keeps undo and redo stacks of `HistoryDelta` (`packages/excalidraw/history.ts:15, 90-153`).

## 5. Interaction

- **App.tsx** is 14,138 lines. Another 3,591 lines are split into `components/App.*.ts` (wheel, pan, viewport, textTool, drawshape and others).
- **Key handlers in App.tsx:**
  - `handleCanvasPointerDown`: `:8683`
  - `handleCanvasPointerMove`: `:7964`
  - `onPointerMoveFromPointerDownHandler`: `:10708`
  - `onPointerUpFromPointerDownHandler`: `:11507`
  - `handleCanvasDoubleClick`: `:7340`
  - `handleSelectionOnPointerDown`: `:9515`
  - `handleLinearElementOnPointerDown`: `:10198`
  - `handleFreeDrawElementOnPointerDown`: `:9988`
  - `createGenericElementOnPointerDown`: `:10534`
  - `handleEraser`: `:8530`
  - `onKeyDown`: `:5585`
  - `getElementAtPosition`: `:6807`
  - `handleTextWysiwyg`: `:6468`
- **Hit threshold:** `max(sw/2 + 0.1, 0.85 * DEFAULT_COLLISION_THRESHOLD / zoom)` (`App.tsx:6927-6934`). `DEFAULT_COLLISION_THRESHOLD = 2 * 4 - 1e-5` (`constants.ts:277-284`).
- **Hit testing** (`element/src/collision.ts`):
  - `hitElementItself` (`:137-230`): single-entry result cache, then a rotated-bounds early reject, then either inside+outline or outline only.
  - `shouldTestInside` (`:87-107`): arrows never; others when they have a non-transparent background, bound text, or are text/iframe; lines and freedraw only when closed; images always.
  - Per-shape intersection helpers: `:494-829`.
  - Geometric shapes for hit testing: `getElementShape` (`shape.ts:1087-1136`) and `packages/utils/src/shape.ts:118-298`.
- **Transform handles** (`element/src/transformHandles.ts`):
  - Size: mouse 8, pen 16, touch 28 (`:49-53`).
  - Rotation handle gap 16 (`:55`), margin 4, spacing 2 (`:133-214`).
  - Layout formulas: `:152-213`.
- **Snapping** (`packages/excalidraw/snapping.ts`, 1414 lines):
  - `SNAP_DISTANCE = 8 / zoom` (`:41-49`).
  - Point snaps (`:636`), gap snaps (`:446`), dragged/resize/new-element snapping (`:692, 1108, 1246`).
- **Binding** (`element/src/binding.ts`, 3234 lines):
  - `BASE_BINDING_GAP 5`; gap = `5 + strokeWidth/2` (`:117-131`).
  - Max binding distance is 15, clamped to [15, 30] depending on zoom (`:133-143`).
  - Fixed points are normalised; exact 0.5 is avoided by using 0.5001 (`:2750-2770`).
- **Elbow arrows** (`element/src/elbowArrow.ts`, 2304 lines):
  - A* over a non-uniform grid (`:1535-1644`), using a binary heap from `common/src/binary-heap.ts`.
  - Bend penalty `bendMultiplier^3` on g; heuristic `m_dist + estBends * bendMultiplier^2`, where `bendMultiplier = manhattan(start, end)` (`:1543, 1610-1629`).
  - `BASE_PADDING 40` (`:111`).
- **Text editing** (`packages/excalidraw/wysiwyg/textWysiwyg.tsx`, 1134 lines):
  - A `<textarea>` overlay with `dir=auto` and `wrap=off` (`:487-493`).
  - CSS transform `translate(w(z-1)/2, h(z-1)/2) scale(z) rotate(deg)` (`:93-104`).
  - Height gets a 5% buffer (`:421`); font, line height and colour are set at `:436-460`.
- **Linear element editor** (`element/src/linearElementEditor.ts`, 2856 lines):
  - `POINT_HANDLE_SIZE = 10` (`:231`).
  - Midpoints (`:811-1047`).
  - A segment is "too short" when its length × zoom < 40 (`:973`).
  - Arrow label position (`:2068`).

## 6. Export

- **`exportToCanvas`** (`packages/excalidraw/scene/export.ts:180-285`):
  - Canvas size = common bounds + 2 × padding, times `exportScale` (`:198-203, 566-575`). `DEFAULT_EXPORT_PADDING = 10` (`constants.ts:402`).
  - Padding is 0 when exporting a single frame.
  - Renders the static scene with `scroll = -min + padding`, zoom 1, `isExporting: true`, no grid, and background null if `exportBackground` is off.
- **Utils wrapper:** `maxWidthOrHeight` gives scale = max / maxDim (`packages/utils/src/export.ts:42-105`).
- **PNG blob:** a `tEXt` chunk with keyword `application/vnd.excalidraw+json` holds the compressed JSON, inserted before IEND (`packages/excalidraw/data/image.ts:25-44`). The compression is pako `deflate` (`data/encode.ts:1`).
- **`exportToSvg`** (`scene/export.ts:293-508`):
  - Root: `viewBox 0 0 w h`, `width/height * exportScale`.
  - Contents in order: `<!-- svg-source:excalidraw -->`, `<metadata>` (optional base64 payload between `payload-start`/`payload-end` comments, `:510-529`), `<defs>` with one frame `clipPath` per frame (rx 8), `<style class="style-fonts">` with subsetted `@font-face` rules, a background `<rect>`, then the elements.
- **Element SVG** (`renderer/staticSvgScene.ts`):
  - roughjs drawables go through `rough.svg().draw` with `fixedDecimalPlaceDigits = 2` (`:60-74`; `MAX_DECIMALS_FOR_SVG_EXPORT` at `constants.ts:399`).
  - Each element is wrapped in `<g stroke-linecap="round" transform="translate(ox oy) rotate(deg cx cy)">`.
  - Opacity is written as stroke-opacity and fill-opacity.
  - Arrow labels use a `<mask>` (`:404-470`).
  - Freedraw: `<path fill=stroke d=…>` (`:516-575`).
  - Images: `<symbol>` + `<use>`, with a clipPath for roundness (`:577-738`).
  - Text: one `<text>` per line with `dominant-baseline="alphabetic"`, `text-anchor` start/middle/end, `white-space: pre` (`:776-842`).
  - Golden output example (a hachure fill path has stroke-width 0.5, which is `sw / 2`): `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap:138-236`.
- **Clipboard** (`packages/excalidraw/clipboard.ts`):
  - JSON copy writes both the `application/vnd.excalidraw.clipboard+json` and `text/plain` types (`:195-206`).
  - PNG copy via `ClipboardItem` (`:557-568`).
  - Paste parsing: `:445-555`.
- **JSON file:** `{type: "excalidraw", version: 2, source, elements, appState, files}` (`data/json.ts:59-65`).

## 7. Fonts

- **Family IDs:** Virgil 1, Helvetica 2, Cascadia 3, Excalifont 5 (the default), Nunito 6, Lilita One 7, Comic Shanns 8, Liberation Sans 9, Assistant 10 (`constants.ts:140-151, 268`).
- **Fallbacks:** Xiaolai 100 (CJK, Excalifont only), sans-serif 998, monospace 999, Segoe UI Emoji 1000 (`:158-197`).
- **Metrics** (unitsPerEm / ascender / descender / lineHeight), from `common/src/font-metadata.ts:35-138`:

| Font | unitsPerEm | ascender | descender | lineHeight |
|---|---|---|---|---|
| Excalifont | 1000 | 886 | -374 | 1.25 |
| Nunito | 1000 | 1011 | -353 | 1.25 |
| Lilita | 1000 | 923 | -220 | 1.15 |
| Comic Shanns | 1000 | 750 | -250 | 1.25 |
| Virgil | 1000 | 886 | -374 | 1.25 |
| Helvetica | 2048 | 1577 | -471 | 1.15 |
| Cascadia | 2048 | 1900 | -480 | 1.2 |
| Liberation | 2048 | 1854 | -434 | 1.15 |
| Assistant | 2048 | 1021 | -287 | 1.25 |
| Xiaolai | 1000 | 880 | -144 | 1.25 |

- **Assets:** woff2 files split by unicode range, in `packages/excalidraw/fonts/<Family>/`. Excalifont has 7 files, with ranges at `fonts/Excalifont/index.ts`. Xiaolai has about 209 files. Helvetica and Emoji are `local:` only and never inlined.
- **Registry:** `Fonts.init` (`fonts/Fonts.ts:375-419`) and `register` (`:339-373`).
- **SVG font inlining:**
  - `generateFontFaceDeclarations` (`Fonts.ts:182-217`) collects the characters used per family, adds Xiaolai when the text contains CJK, and runs at concurrency 3.
  - Each font face calls `ExcalidrawFontFace.toCSS(chars)`, producing `@font-face { font-family: X; src: url(data:font/woff2;base64,…) }` (`fonts/ExcalidrawFontFace.ts:37-87`).
  - Subsetting: `subset-main.ts` sends work to a worker (`subset-worker.chunk.ts`). The worker does WASM woff2 decompress, HarfBuzz subset, then woff2 compress (`subset/subset-shared.chunk.ts:44-57`, with `subset/harfbuzz/*` and `subset/woff2/*`).
  - The asset fallback URL is esm.sh (`ExcalidrawFontFace.ts:11`).

## 8. External dependencies (versions)

- **`packages/excalidraw/package.json`:** roughjs 4.6.4 (`:114`), perfect-freehand 1.2.0 (`:106`), points-on-curve 1.0.1 (`:111`), pako 2.0.3 (`:105`), browser-fs-access 0.38.0 (`:93`), jotai 2.11.0 (`:100`), jotai-scope 0.7.2, nanoid 3.3.3 (`:104`), png-chunk-text / png-chunks-encode / png-chunks-extract 1.0.0 (`:108-110`), pica 7.1.1, image-blob-reduce 3.0.1, canvas-roundrect-polyfill 0.0.1, es6-promise-pool 2.5.0, @excalidraw/laser-pointer 1.3.1 (`:89`), @excalidraw/mermaid-to-excalidraw 2.2.2, @braintree/sanitize-url 6.0.2, radix-ui 1.4.3, codemirror 6, lodash.throttle/debounce, fuzzy 0.1.3, tunnel-rat 0.1.2, sass 1.51.0. Peer: react 17, 18 or 19.
- **`common`:** tinycolor2 1.6.0 (`packages/common/package.json:60`).
- **`element`:** only internal packages, including @excalidraw/fractional-indexing 3.3.0.
- **`utils`:** roughjs, perfect-freehand, pako, png-chunk*, browser-fs-access, laser-pointer.
- **Vendored in-repo:** fractional-indexing (322 LOC) and laser-pointer (527 LOC).
- **Possible Rust equivalents:**

| JS dependency | Rust option |
|---|---|
| roughjs | `roughr` (a port) or hand-port roughjs 4.6.4 |
| perfect-freehand | port it (small) |
| points-on-curve | port it |
| pako | port pako 2.0.3's deflate and inflate (byte-identical output, pako's error messages) |
| tinycolor | `csscolorparser` |
| png chunks | port of png-chunks-extract, png-chunks-encode and png-chunk-text 1.0.0 (`excali_core::png`) for their bytes, quirks and messages |
| harfbuzz subset | `hb-subset` / `allsorts` |
| woff2 | `woff2` crates |
| nanoid | `nanoid` |

## 9. Test infrastructure

- **Runner:** vitest 3.0.6 with jsdom 22.1.0 and `vitest-canvas-mock` 0.3.3 (root `package.json` devDependencies; `vitest.config.mts:82-84`). Coverage thresholds: lines 60, branches 70 (`:92-97`).
- **`setupTests.ts`:** imports the canvas mock (`:4`), mocks `throttleRAF` (`:35-40`), `matchMedia` (`:51`) and `FontFace` (`:65-92`), and mocks only font *fetching*, so real subsetting still runs in snapshot tests (`:101-115`).
- Text measurement in tests counts characters × 10 px (`textMeasurements.ts:144-146`).
- **Snapshots:** 23 `.snap` files, 55,752 lines in total. The largest are `history.test.tsx.snap` (23,011), `regressionTests` (15,824) and `contextmenu` (10,251).
- **Pixel-exact golden references:**
  - `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap` (251 lines): SVG export of the fixture elements.
  - `tests/fixtures/svg-image-exporting-reference.svg`.
- **Fixtures** (`packages/excalidraw/tests/fixtures/`): `elementFixture.ts` (seed 1041657908, hachure fill, roughness 1), `diagramFixture.ts`, `fixture_library.excalidrawlib`, and PNG/SVG files with embedded scenes (`smiley_embedded_v2.*`, `test_embedded_v1.*`). There are no `.excalidraw` files in the repo.

## 10. Lines of code (`.ts` + `.tsx`)

| Package | All | Non-test |
|---|---|---|
| common | 4,881 | 3,825 |
| element | 58,517 | 37,256 |
| math | 3,125 | 2,379 |
| excalidraw | 133,973 | 88,223 |
| utils | 1,245 | 810 |
| fractional-indexing | 322 | 322 |
| laser-pointer | 527 | 527 |

Largest core files: App.tsx 14,138; binding.ts 3,234; linearElementEditor.ts 2,856; elbowArrow.ts 2,304; delta.ts 2,213; interactiveScene.ts 2,166; bucketFill.ts 1,918; resizeElements.ts 1,594; bounds.ts 1,571; snapping.ts 1,414; shape.ts 1,346; renderElement.ts 1,329; textWysiwyg.tsx 1,134; store.ts 1,037; staticSvgScene.ts 933; export.ts 587; staticScene.ts 545.
