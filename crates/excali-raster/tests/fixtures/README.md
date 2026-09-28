# Raster fixtures

The tiny-skia backend is held to what Chrome's canvas paints for the same
calls (ex-401). Upstream renders with the Canvas 2D API, so the browser is the
specification.

- `display-lists/<name>.json`: a display list in the canvas's own vocabulary
  (below), with a `description`, a size, an optional device `scale`, images,
  and a `tolerance` with a `toleranceNote` saying where any difference from
  Chrome comes from.
- `chrome/<name>.png`: what headless Chrome paints for it, and
  `chrome/manifest.json`: the Chrome that drew them and each fixture's SHA-256,
  so a fixture edited without new references fails `references_are_current`.
- `images/`: image files the fixtures embed as data URLs (written by
  `scripts/fixtures/raster-images.py`, ex-404).

`scripts/fixtures/raster-references.sh` draws every fixture on a real
`CanvasRenderingContext2D` (`scripts/fixtures/raster_references.html`, which
calls the canvas directly and shares no code with the port) and writes the
references; `--check` draws them with the local Chrome into a temporary
directory and runs the Rust comparison against those (the `raster-chrome` CI
job). `tests/fixtures.rs` renders each list with `excali_raster::render_scaled`
and compares it with `excali_raster::diff` under the fixture's tolerance: at
most `pixels` pixels may differ by more than `channel` levels in a
premultiplied channel. A failing fixture writes the rendered image and a diff
image (red over the bound, yellow within it) to `target/raster-diff/`.

Generated fixtures, each checked by the test that writes it (which fails when
the file is stale):

- `sketch.json`: `EXCALI_WRITE_RASTER_FIXTURE=1 cargo test -p excali-scene
  --test raster_fixture` writes the port's own rough.js output for four
  Excalidraw shapes.
- `image-elements.json`, `image-elements-svg.json`:
  `EXCALI_WRITE_RASTER_FIXTURE=1 cargo test -p excali-scene --test
  image_elements` writes the port's display list for every case of
  `crates/excali-scene/tests/fixtures/image-elements.json` (upstream's own
  `renderElement` on image elements, `tools/goldens/image-elements.mjs`), with
  upstream's recorded canvas calls beside it as `canvasCalls`: Chrome replays
  upstream's calls, the port renders its items.
- `images-decoded.json`, `images-svg.json`: `scripts/fixtures/raster-images.py
  --lists` writes `drawImage`s of the files in `images/`.

## Vocabulary

Numbers may be JSON numbers or the strings `"NaN"`, `"Infinity"` and
`"-Infinity"`.

| Item | Keys | Canvas calls |
|---|---|---|
| `{"type": "fill"}` | `color`, `rule` (`nonzero`, `evenodd`), `path` | `fillStyle`, the path, `fill(rule)` |
| `{"type": "fillRect"}` | `color`, `rect` `[x, y, w, h]` | `fillStyle`, `fillRect(x, y, w, h)` |
| `{"type": "stroke"}` | `color`, `width`, `cap`, `join`, `miterLimit`, `dash`, `dashOffset`, `path` | `strokeStyle`, `lineWidth`, `lineCap`, `lineJoin`, `miterLimit`, `setLineDash`, `lineDashOffset`, the path, `stroke()` |
| `{"type": "image"}` | `id`, `source` `[x, y, w, h]`, `dest`, `smoothing`, `filter` (`dark`) | `imageSmoothingEnabled`, `filter`, `drawImage` |
| `{"type": "group"}` | `transform` `[a, b, c, d, e, f]`, `opacity`, `clip` `{path, rule}`, `items` | `save()`, `transform(…)`, `globalAlpha *= opacity`, the clip, the items, `restore()` |

A path is a list of calls: `["M", x, y]`, `["L", x, y]`, `["Q", cpx, cpy, x,
y]`, `["C", cp1x, cp1y, cp2x, cp2y, x, y]`, `["A", cx, cy, r, start, end,
anticlockwise]`, `["Z"]`, `["rect", x, y, w, h]` and `["roundRect", x, y, w,
h, r]`. Images are `{width, height, rgba}` with straight-alpha RGBA rows, as
`ImageData` holds them, or `{dataUrl}`, which Chrome loads as an `<img>` and
the port decodes with `excali_raster::decode`. An image id the fixture does
not define may name a built-in image (`builtin:image-placeholder`,
`builtin:image-error-placeholder`), which the backend has itself. There is no
text: glyphs come from the font pipeline, not from this backend.

A fixture may also carry `canvasCalls`, recorded canvas calls that Chrome
replays instead of drawing `items` (`["set", property, value]`, or a method
name and its arguments; `drawImage`'s image is `{"file": id}`, an entry of
`images`, or `{"placeholder": kind}`, an entry of `placeholders`, SVG sources
Chrome loads as `data:image/svg+xml,` URLs), and `placeholders`. The port
always renders `items`.

## Adding a fixture

Write the JSON with a `description`, run
`scripts/fixtures/raster-references.sh`, look at the new PNG, run `cargo test
-p excali-raster --test fixtures -- --nocapture` for the measured differences,
and set the tolerance and its note from them.
