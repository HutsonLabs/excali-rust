# Chrome PNG export references

The port's full PNG export is held to what upstream's own `exportToCanvas`
draws in Chrome for the same scenes (ex-g402, the PNG half of D2).

- `<scene>.png`: `canvas.toBlob("image/png")` of the canvas upstream's
  `exportToCanvas` (pinned commit, bundled by `tools/goldens/lib/upstream.mjs`)
  returned in Playwright's Chromium, software canvas in sRGB at device pixel
  ratio 1, with the vendored fonts loaded by upstream's own
  `Fonts.loadElementsFonts` (its font URLs pointed at
  `crates/excali-text/assets/fonts` through `window.EXCALIDRAW_ASSET_PATH`).
- `scenes/<scene>.excalidraw`: the element fixture scenes, each element of
  upstream's `tests/fixtures/elementFixture.ts` alone (and its text in
  Nunito, as `tests/scene/export.test.ts` exports it), written by upstream's
  `serializeAsJSON`.
- `manifest.json`: the upstream commit, the Chromium and platform, the font
  files the page loaded, and per scene its source, that source's SHA-256,
  the canvas size and the PNG's SHA-256.
- `tolerances.json`, written by hand: per scene its tolerance in two tiers
  and a `note` saying where the difference comes from. The canvas tier
  (`channel`, `pixels`) is over every pixel; the `outsideText` tier,
  required wherever the port draws text, is over the pixels outside the
  boxes of the port's text runs and holds the shapes, images and
  background to 8 levels, so the canvas tier's allowance for glyph edges
  reaches nothing else. `smiley_embedded_v2` is `excluded` (the reason is
  recorded): its only drawing is an emoji no vendored face has (ADR-004),
  so it has no canvas tier, is not counted as a compared scene, and only
  its canvas size and the pixels outside its text element's box are held
  to Chrome's (within 2 levels).

The scenes are every scene of `crates/excali-scene/tests/fixtures/canvas-export.json`
(exported from its recorded inputs), the seven element fixture scenes, and
upstream's `test_embedded_v1.png` and `smiley_embedded_v2.png`. A scene file
is loaded as the editor loads a dropped file (`loadFromBlob`) and exported
as "Export image" exports it (`exportCanvas("png")`); the port does the same
with `excali render`'s code (`input::load_scene`, `export::render_png`).

`scripts/fixtures/chrome-png-export.sh` writes everything here but
`tolerances.json` and this file; `--check` draws the references again with
the local Chromium into a temporary directory and runs the comparison
against those (the `chrome-png-export` CI job, on macOS arm64 like the
references).

`crates/excali-cli/tests/chrome_export.rs` renders each scene with the
port's full PNG export (text drawn by the CLI's `GlyphText`, images decoded
from their data URLs), decodes it, and compares it with the reference with
`excali_raster::diff`: the canvas size must be equal, and in each tier at
most `pixels` pixels may differ by more than `channel` levels in a
premultiplied channel.
A failing scene writes the port's PNG, Chrome's and a diff image to
`target/chrome-export-diff/`. The test also requires that every scene has a
reference and a noted tolerance, that each source and PNG hashes to what the
manifest recorded, that a canvas of the background alone falls outside each
tolerance (the port's export of `negative-size` and `smiley_embedded_v2` must
instead be the background alone), that without text the scenes with text
fall outside theirs, that every scene with text has an `outsideText` tier of
at most 8 levels, and that the port's canvas shifted uniformly by 12 or 40
levels fails every scene.

## Adding a scene or changing a tolerance

Add the scene to `chrome_png_export.mjs` and to the lists of
`chrome_export.rs`, run `scripts/fixtures/chrome-png-export.sh`, look at the
new PNG, run `cargo test -p excali-cli --test chrome_export -- --nocapture`
for the measured differences of each tier, and set both tiers and the note from them.
