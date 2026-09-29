//! The PNG half of D2 (ex-g402): the port's full PNG export of the fixture
//! scenes against the PNG upstream's own `exportToCanvas` draws in Chrome.
//!
//! The references are `tests/fixtures/chrome-export/<scene>.png`, written by
//! `scripts/fixtures/chrome-png-export.sh` (upstream at the pin in
//! Playwright's Chromium, the vendored fonts loaded by upstream's own
//! `Fonts.loadElementsFonts`), with a `manifest.json` naming the Chromium
//! and each scene's source, that source's SHA-256 and the PNG's. The scenes:
//!
//! - every scene of excali-scene's canvas export golden
//!   (`crates/excali-scene/tests/fixtures/canvas-export.json`), exported by
//!   the port from the same inputs (`export_canvas_png`, or the utils
//!   sizing) with frame names measured by the vendored fonts, as the
//!   browser measures them, and the files decoded from their data URLs;
//! - each element of upstream's `elementFixture.ts` as a scene file, and
//!   upstream's `test_embedded_v1.png` and `smiley_embedded_v2.png`: loaded
//!   and exported as `excali render` does (`input::load_scene`,
//!   `export::render_png`).
//!
//! Text is drawn by the CLI's `GlyphText`, images from the decoded files,
//! and the PNG is the one `excali_raster::export_png` encodes, decoded
//! again. The canvas size must be Chrome's exactly; the pixels must be
//! within both tiers of the scene's tolerance in
//! `tests/fixtures/chrome-export/tolerances.json` ([`Recorded`]): the whole
//! canvas, and the pixels outside the port's text boxes held to 8 levels.
//! The note says where the difference comes from. `smiley_embedded_v2` is
//! excluded from the comparison (its emoji has no vendored face): only its
//! canvas size and the pixels outside its text element are checked.
//!
//! `EXCALI_CHROME_EXPORT_REFERENCES=<dir>` reads the PNGs and manifest from
//! another directory (`chrome-png-export.sh --check` draws them with the
//! local Chromium). A scene over its tolerance writes the port's PNG and a
//! diff image to `target/chrome-export-diff/`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tiny_skia::{Mask, Pixmap, PremultipliedColorU8};

use excali_cli::env::CliEnv;
use excali_cli::export::{png_canvas, render_png, ExportSettings};
use excali_cli::fonts::{built_in_fonts_dir, load_fonts, GlyphText};
use excali_cli::input::{load_scene, read_file};
use excali_core::element::Element;
use excali_raster::decode::ImageFiles;
use excali_raster::diff::{compare, diff_image, Tolerance};
use excali_raster::{export_png, paint_canvas, TextRasterizer};
use excali_scene::display::{CanvasDocument, TextAlign, TextRun};
use excali_text::font_store::FontStore;

#[path = "../../excali-raster/tests/support/golden_scenes.rs"]
mod golden_scenes;

/// The element fixture scenes (`elementFixture.ts`, one element each, and
/// the export test's text in Nunito), written by the reference script.
const ELEMENT_SCENES: [&str; 7] = [
    "element-rectangle",
    "element-embeddable",
    "element-ellipse",
    "element-diamond",
    "element-rectangle-with-link",
    "element-text",
    "element-text-nunito",
];

/// Upstream's scene-bearing PNG fixtures.
const EMBEDDED_PNGS: [&str; 2] = ["test_embedded_v1", "smiley_embedded_v2"];

/// Scenes whose pixels include text: drawn without it, each must fall
/// outside its tolerance, so the comparison holds `GlyphText` to Chrome's
/// `fillText`.
const TEXT_SCENES: [&str; 9] = [
    "default",
    "scale-3-embed",
    "frames",
    "utils-max-smaller",
    "utils-max-and-get-dimensions",
    "element-text",
    "element-text-nunito",
    "test_embedded_v1",
    "embeddables",
];

/// Scenes the port exports as their background alone, where the
/// background check cannot hold: `negative-size`, whose padding of -100
/// leaves the rectangle outside the canvas (Chrome draws the background
/// alone too, and the tolerance is exact), and `smiley_embedded_v2`, whose
/// only drawing is an emoji: no vendored face has one and upstream's emoji
/// family is `local:` (ADR-004), so Chrome draws the system's colour emoji
/// and the port nothing, so the scene is excluded from the comparison
/// (tolerances.json says so). The port's export of each must be the
/// background alone, so a port that starts drawing more fails here and the
/// tolerance is revisited.
const BACKGROUND_ONLY: [&str; 2] = ["negative-size", "smiley_embedded_v2"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo() -> PathBuf {
    crate_dir().join("../..")
}

fn fixture_dir() -> PathBuf {
    crate_dir().join("tests/fixtures/chrome-export")
}

fn references_dir() -> PathBuf {
    match std::env::var_os("EXCALI_CHROME_EXPORT_REFERENCES") {
        Some(dir) => PathBuf::from(dir),
        None => fixture_dir(),
    }
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn canvas_scenes() -> Vec<Value> {
    golden_scenes::fixture()["scenes"]
        .as_array()
        .unwrap()
        .clone()
}

/// Every scene compared, in order.
fn scene_names() -> Vec<String> {
    let mut names: Vec<String> = canvas_scenes()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    names.extend(ELEMENT_SCENES.iter().map(|s| s.to_string()));
    names.extend(EMBEDDED_PNGS.iter().map(|s| s.to_string()));
    names
}

/// A scene's recorded tolerance (`tolerances.json`), in two tiers:
///
/// - `canvas`, the top-level `channel` and `pixels`: over every pixel.
///   Where the scene has text it allows the glyph edges, where Chrome's
///   CoreText rasterization and `GlyphText` part by up to 64 levels.
/// - `outsideText`: over the pixels outside the boxes of the text runs the
///   port draws ([`TextBoxes`]), where the rough.js shapes, images and
///   background are held to at most 8 levels. Required wherever the port
///   draws text.
///
/// A scene with `excluded` (its reason) is not compared with Chrome: it has
/// no `canvas` tier, and `outsideText` holds the pixels outside its text
/// element's box on the canvas ([`element_box_mask`]).
struct Recorded {
    canvas: Option<Tolerance>,
    outside_text: Option<Tolerance>,
    excluded: Option<String>,
    note: String,
}

fn tolerance_of(name: &str, t: &Value) -> Tolerance {
    let channel = t["channel"]
        .as_u64()
        .unwrap_or_else(|| panic!("{name}: channel"));
    let pixels = t["pixels"]
        .as_u64()
        .unwrap_or_else(|| panic!("{name}: pixels"));
    Tolerance {
        channel: u8::try_from(channel).unwrap(),
        pixels: usize::try_from(pixels).unwrap(),
    }
}

fn tolerances() -> BTreeMap<String, Recorded> {
    let file = read_json(&fixture_dir().join("tolerances.json"));
    file["scenes"]
        .as_object()
        .expect("tolerances.json has scenes")
        .iter()
        .map(|(name, t)| {
            let excluded = t["excluded"].as_str().map(str::to_owned);
            let recorded = Recorded {
                canvas: excluded.is_none().then(|| tolerance_of(name, t)),
                outside_text: t
                    .get("outsideText")
                    .map(|o| tolerance_of(&format!("{name}.outsideText"), o)),
                excluded,
                note: t["note"].as_str().unwrap_or_default().to_owned(),
            };
            (name.clone(), recorded)
        })
        .collect()
}

/// One tier of a scene's comparison.
struct Tier {
    label: &'static str,
    tolerance: Tolerance,
    /// The port's pixmap as compared: for `outsideText`, with the text
    /// boxes' pixels replaced by Chrome's.
    actual: Pixmap,
    diff: excali_raster::diff::Diff,
}

/// The comparison of the port's `actual` with Chrome's `expected` under
/// every tier `recorded` has, `mask` the pixels the `outsideText` tier
/// leaves out; a different canvas size is an error.
fn tiers(
    actual: &Pixmap,
    expected: &Pixmap,
    recorded: &Recorded,
    mask: &[bool],
) -> Result<Vec<Tier>, String> {
    let mut tiers = Vec::new();
    if let Some(t) = recorded.canvas {
        let diff = compare(actual.as_ref(), expected.as_ref()).map_err(|e| e.to_string())?;
        tiers.push(Tier {
            label: "canvas",
            tolerance: t,
            actual: actual.clone(),
            diff,
        });
    }
    if let Some(t) = recorded.outside_text {
        let masked = outside(actual, expected, mask);
        let diff = compare(masked.as_ref(), expected.as_ref()).map_err(|e| e.to_string())?;
        tiers.push(Tier {
            label: "outsideText",
            tolerance: t,
            actual: masked,
            diff,
        });
    }
    assert!(!tiers.is_empty(), "a tolerance with no tier");
    Ok(tiers)
}

/// Whether `actual` is within every tier of `recorded`.
fn passes(actual: &Pixmap, expected: &Pixmap, recorded: &Recorded, mask: &[bool]) -> bool {
    tiers(actual, expected, recorded, mask)
        .is_ok_and(|tiers| tiers.iter().all(|t| t.diff.within(&t.tolerance)))
}

/// The pixels a scene's `outsideText` tier leaves out: the port's text
/// boxes, or for an excluded scene its text element's box.
fn scene_mask(name: &str, recorded: &Recorded) -> Vec<bool> {
    if recorded.excluded.is_some() {
        element_box_mask(name)
    } else {
        text_mask(name)
    }
}

/// The box of an excluded scene's one element on the exported canvas: the
/// scene is a single text element, exported at scale 1 with the default
/// padding of 10, so the element's box is its width and height from
/// (10, 10), and the canvas is 20 larger each way.
fn element_box_mask(name: &str) -> Vec<bool> {
    let file = read_file(&scene_file(name)).unwrap();
    let scene = load_scene(&file, &mut CliEnv::new(0.0, 1)).unwrap();
    let elements: Vec<&Element> = scene
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect();
    assert_eq!(elements.len(), 1, "{name}: one element");
    let e = &elements[0].base;
    assert_eq!(
        elements[0].element_type(),
        excali_core::element::ElementType::Text,
        "{name}"
    );
    let reference = reference(name);
    let (width, height) = (reference.width(), reference.height());
    assert_eq!(
        (f64::from(width), f64::from(height)),
        (e.width.ceil() + 20.0, e.height.ceil() + 20.0),
        "{name}: the canvas is the element's box and the padding"
    );
    let (right, bottom) = (10.0 + e.width, 10.0 + e.height);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (f64::from(x), f64::from(y))))
        .map(|(x, y)| x + 1.0 > 10.0 && x < right && y + 1.0 > 10.0 && y < bottom)
        .collect()
}

/// A scene file's path: a written element fixture or an upstream PNG.
fn scene_file(name: &str) -> PathBuf {
    if EMBEDDED_PNGS.contains(&name) {
        repo()
            .join("fixtures/upstream/packages/excalidraw/tests/fixtures")
            .join(format!("{name}.png"))
    } else {
        fixture_dir()
            .join("scenes")
            .join(format!("{name}.excalidraw"))
    }
}

/// The port's canvas for a golden scene: its document with the vendored
/// fonts' metrics, and the decoded files.
fn golden_canvas(scene: &Value) -> (CanvasDocument, ImageFiles, FontStore) {
    let elements: Vec<Element> = scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Element::from_map(e.as_object().unwrap().clone()).unwrap())
        .collect();
    let fonts = load_fonts(&built_in_fonts_dir(), &elements).unwrap();
    let files = scene["files"].as_object().cloned().unwrap_or_default();
    let images = ImageFiles::decode(files.iter().filter_map(|(id, file)| {
        Some((
            id.as_str(),
            file.get("mimeType")?.as_str()?,
            file.get("dataURL")?.as_str()?,
        ))
    }));
    let loads = |id: &str| images.mime_type(id).is_some();
    let source = golden_scenes::fixture()["origin"]
        .as_str()
        .unwrap()
        .to_owned();
    let doc = golden_scenes::document_with(scene, &source, &fonts, &loads);
    (doc, images, fonts)
}

/// The port's PNG of a golden scene, painted with `text`.
fn golden_png(scene: &Value, text: Text) -> Vec<u8> {
    let (doc, images, fonts) = golden_canvas(scene);
    match text {
        Text::Glyphs => export_png(&doc, &images, &mut GlyphText::new(&fonts)).unwrap(),
        Text::None => export_png(&doc, &images, &mut NoText).unwrap(),
    }
}

#[derive(Clone, Copy)]
enum Text {
    Glyphs,
    None,
}

/// No text at all: what the comparison must tell apart from `GlyphText`.
struct NoText;

impl TextRasterizer for NoText {
    fn fill_text(
        &mut self,
        _: &mut Pixmap,
        _: &TextRun,
        _: tiny_skia::Color,
        _: tiny_skia::Transform,
        _: Option<&Mask>,
    ) {
    }
}

/// Where the port draws text: each run's line box, painted into a mask of
/// the canvas's size under the run's transform and clip. The box spans the
/// run's advance (at least one em per character, so a run whose glyphs no
/// vendored face has, the emoji, still has its box) and from 1.25 em above
/// the baseline to 0.5 em below it, grown by a quarter em on every side:
/// the room Chrome's glyphs of another face (Helvetica for Liberation
/// Sans, Apple Color Emoji) and heavier stems take around the port's.
struct TextBoxes<'a> {
    store: &'a FontStore,
    boxes: Option<Pixmap>,
}

impl TextRasterizer for TextBoxes<'_> {
    fn fill_text(
        &mut self,
        target: &mut Pixmap,
        run: &TextRun,
        _: tiny_skia::Color,
        transform: tiny_skia::Transform,
        clip: Option<&Mask>,
    ) {
        let boxes = self
            .boxes
            .get_or_insert_with(|| Pixmap::new(target.width(), target.height()).unwrap());
        let em = run.font.size;
        let line = self.store.shape_line(&run.text, &run.font.css(), false);
        let width = line.width.max(em * run.text.chars().count() as f64);
        let left = run.x
            - match run.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => width / 2.0,
                TextAlign::Right => width,
            };
        let pad = em / 4.0;
        let Some(rect) = tiny_skia::Rect::from_ltrb(
            (left - pad) as f32,
            (run.y - 1.25 * em - pad) as f32,
            (left + width + pad) as f32,
            (run.y + 0.5 * em + pad) as f32,
        ) else {
            return;
        };
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = false;
        boxes.fill_rect(rect, &paint, transform, clip);
    }
}

/// The port's canvas for scene `name`, before painting: its document, the
/// decoded files and the faces its text needs.
fn port_canvas(name: &str) -> (CanvasDocument, ImageFiles, FontStore) {
    if let Some(scene) = canvas_scenes().iter().find(|s| s["name"] == name) {
        let (doc, images, fonts) = golden_canvas(scene);
        return (doc, images, fonts);
    }
    let file = read_file(&scene_file(name)).unwrap();
    let scene = load_scene(&file, &mut CliEnv::new(0.0, 1)).unwrap();
    let canvas = png_canvas(&scene, &ExportSettings::new()).unwrap();
    (canvas.document, canvas.images, canvas.fonts)
}

/// Per pixel of scene `name`'s canvas, whether it lies in the box of a
/// text run the port draws ([`TextBoxes`]).
fn text_mask(name: &str) -> Vec<bool> {
    let (doc, images, fonts) = port_canvas(name);
    let mut boxes = TextBoxes {
        store: &fonts,
        boxes: None,
    };
    let canvas = paint_canvas(&doc, &images, &mut boxes).unwrap();
    match boxes.boxes {
        Some(mask) => mask.pixels().iter().map(|p| p.alpha() > 0).collect(),
        None => vec![false; canvas.pixels().len()],
    }
}

/// `actual` with the pixels in `mask` replaced by `expected`'s, so a
/// comparison of the two counts only the pixels outside the mask.
fn outside(actual: &Pixmap, expected: &Pixmap, mask: &[bool]) -> Pixmap {
    let mut out = actual.clone();
    if out.width() != expected.width() || out.height() != expected.height() {
        return out;
    }
    for ((o, e), m) in out.pixels_mut().iter_mut().zip(expected.pixels()).zip(mask) {
        if *m {
            *o = *e;
        }
    }
    out
}

/// The port's PNG export of scene `name`.
fn port_png(name: &str) -> Vec<u8> {
    if let Some(scene) = canvas_scenes().iter().find(|s| s["name"] == name) {
        return golden_png(scene, Text::Glyphs);
    }
    let file = read_file(&scene_file(name)).unwrap();
    let scene = load_scene(&file, &mut CliEnv::new(0.0, 1)).unwrap();
    render_png(&scene, &ExportSettings::new()).unwrap()
}

fn decode(png: &[u8], what: &str) -> Pixmap {
    Pixmap::decode_png(png).unwrap_or_else(|e| panic!("{what}: {e}"))
}

fn reference(name: &str) -> Pixmap {
    let path = references_dir().join(format!("{name}.png"));
    Pixmap::load_png(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn out_dir() -> PathBuf {
    // tests run in the crate directory; target/ is the workspace's.
    let dir = repo().join("target/chrome-export-diff");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_scene_has_a_reference_and_a_noted_tolerance() {
    let expected: BTreeSet<String> = scene_names().into_iter().collect();
    assert!(expected.len() > 30, "{} scenes", expected.len());
    let manifest = read_json(&fixture_dir().join("manifest.json"));
    let recorded: BTreeSet<String> = manifest["scenes"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        recorded, expected,
        "one reference per scene: run scripts/fixtures/chrome-png-export.sh"
    );
    let tolerances = tolerances();
    let noted: BTreeSet<String> = tolerances.keys().cloned().collect();
    assert_eq!(
        noted, expected,
        "one tolerance per scene in tolerances.json"
    );
    for (name, t) in &tolerances {
        assert!(
            t.note.split_whitespace().count() >= 5,
            "{name}: the tolerance needs a note saying where the difference comes from"
        );
    }
    let file = read_json(&fixture_dir().join("tolerances.json"));
    for (name, t) in &tolerances {
        let raw = &file["scenes"][name];
        match &t.excluded {
            Some(reason) => {
                assert!(
                    raw["channel"].is_null() && raw["pixels"].is_null(),
                    "{name}: an excluded scene has no canvas tier"
                );
                assert!(
                    reason.split_whitespace().count() >= 5,
                    "{name}: say why the scene is excluded"
                );
                assert!(
                    t.outside_text.is_some_and(|o| o.channel <= 2 && o.pixels == 0),
                    "{name}: outside its element's box an excluded scene is Chrome's within 2 levels"
                );
            }
            None => assert!(raw["excluded"].is_null(), "{name}"),
        }
    }
}

/// Where the port draws text, the pixels outside the text boxes are held
/// to 8 levels: the canvas tier's allowance for glyph edges does not reach
/// the shapes and the background. Where it draws none, the canvas tier is
/// that tight itself.
#[test]
fn outside_the_text_every_scene_is_held_to_8_levels() {
    for (name, t) in tolerances() {
        if t.excluded.is_some() {
            continue;
        }
        let canvas = t.canvas.unwrap();
        let has_text = text_mask(&name).contains(&true);
        if has_text {
            let o = t
                .outside_text
                .unwrap_or_else(|| panic!("{name}: text, and no outsideText tier"));
            assert!(o.channel <= 8, "{name}: outsideText {o:?}");
            assert!(
                o.pixels <= 2,
                "{name}: outsideText allows {} pixels over {} levels",
                o.pixels,
                o.channel
            );
        } else {
            assert!(
                t.outside_text.is_none(),
                "{name}: no text, so the canvas tier is the whole comparison"
            );
            assert!(
                canvas.channel <= 8,
                "{name}: no text, and the canvas tier allows {canvas:?}"
            );
        }
    }
}

/// The references are Chromium's pixels for these exact inputs: each
/// scene's source (the canvas export golden, a written scene file, an
/// upstream PNG) and each PNG must hash to what the manifest recorded,
/// and each PNG has the canvas size recorded, which for a golden scene is
/// the size upstream's `exportToCanvas` gave its canvas under Node too.
#[test]
fn references_are_current() {
    let dir = references_dir();
    let manifest = read_json(&dir.join("manifest.json"));
    let upstream = std::fs::read_to_string(repo().join("site/config.toml")).unwrap();
    let commit = manifest["upstream"].as_str().unwrap();
    assert!(
        upstream.contains(&format!("upstream_commit = \"{commit}\"")),
        "the references were drawn from upstream {commit}, not the pin"
    );
    let browser = manifest["browser"].as_str().unwrap_or_default();
    assert!(
        browser.starts_with("Chromium "),
        "the manifest names the Chromium that drew the references: {browser:?}"
    );
    assert!(
        manifest["fonts"].as_array().is_some_and(|f| f
            .iter()
            .any(|f| f.as_str().is_some_and(|f| f.starts_with("Excalifont/")))),
        "the page loaded the vendored Excalifont"
    );
    let golden = golden_scenes::fixture();
    for name in scene_names() {
        let entry = &manifest["scenes"][&name];
        assert!(entry.is_object(), "{name}: not in the manifest");
        let source = entry["source"].as_str().unwrap();
        let file = source.split('#').next().unwrap();
        let bytes = std::fs::read(repo().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(
            entry["sourceSha256"].as_str().unwrap(),
            sha256(&bytes),
            "{name}: {file} changed since the reference was drawn: run scripts/fixtures/chrome-png-export.sh"
        );
        let png = std::fs::read(dir.join(format!("{name}.png"))).unwrap();
        assert_eq!(
            entry["pngSha256"].as_str().unwrap(),
            sha256(&png),
            "{name}: the PNG is not the one the manifest recorded"
        );
        let pixmap = decode(&png, &name);
        let size = (u64::from(pixmap.width()), u64::from(pixmap.height()));
        assert_eq!(
            size,
            (
                entry["width"].as_u64().unwrap(),
                entry["height"].as_u64().unwrap()
            ),
            "{name}"
        );
        if let Some(scene) = golden["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name.as_str())
        {
            assert_eq!(
                size,
                (
                    scene["width"].as_u64().unwrap(),
                    scene["height"].as_u64().unwrap()
                ),
                "{name}: Chromium's canvas is not the size upstream gave it under Node"
            );
        }
    }
}

/// The element fixture scene files are the ones the manifest names; each
/// holds its one fixture element as upstream serialized it.
#[test]
fn the_element_fixture_scenes_are_upstreams_fixtures() {
    for name in ELEMENT_SCENES {
        let scene = read_json(&scene_file(name));
        assert_eq!(scene["type"], "excalidraw", "{name}");
        let elements = scene["elements"].as_array().unwrap();
        assert_eq!(elements.len(), 1, "{name}");
        // elementFixture.ts: every fixture shares the element base
        assert_eq!(elements[0]["id"], "vWrqOAfkind2qcm7LDAGZ", "{name}");
        assert_eq!(elements[0]["seed"], 1041657908, "{name}");
    }
}

#[test]
fn the_port_matches_chrome_within_each_scenes_tolerance() {
    let tolerances = tolerances();
    let mut failures = Vec::new();
    let mut summary = Vec::new();
    let names = scene_names();
    let mut compared = 0;
    for name in &names {
        let recorded = tolerances
            .get(name)
            .unwrap_or_else(|| panic!("{name}: no tolerance"));
        let actual = decode(&port_png(name), name);
        let expected = reference(name);
        let mask = scene_mask(name, recorded);
        let tiers = match tiers(&actual, &expected, recorded, &mask) {
            Ok(tiers) => tiers,
            Err(e) => {
                // canvas size equality is required
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        if recorded.excluded.is_none() {
            compared += 1;
        }
        for tier in &tiers {
            let (diff, t) = (&tier.diff, &tier.tolerance);
            summary.push(format!(
                "{name} {}: max {:3}; pixels over 1/8/32/64/128: {}/{}/{}/{}/{} of {}; tolerance {} x {}",
                tier.label,
                diff.max_channel,
                diff.pixels_over(1),
                diff.pixels_over(8),
                diff.pixels_over(32),
                diff.pixels_over(64),
                diff.pixels_over(128),
                diff.width * diff.height,
                t.channel,
                t.pixels,
            ));
            if !diff.within(t) {
                let out = out_dir();
                actual.save_png(out.join(format!("{name}.png"))).unwrap();
                expected
                    .save_png(out.join(format!("{name}-chrome.png")))
                    .unwrap();
                diff_image(tier.actual.as_ref(), expected.as_ref(), t)
                    .unwrap()
                    .save_png(out.join(format!("{name}-{}-diff.png", tier.label)))
                    .unwrap();
                failures.push(format!("{name} ({}): {}", tier.label, diff.report(t)));
            }
        }
    }
    eprintln!("{}", summary.join("\n"));
    let excluded = tolerances.values().filter(|t| t.excluded.is_some()).count();
    assert_eq!(excluded, 1, "only smiley_embedded_v2 is excluded");
    assert_eq!(
        compared,
        names.len() - excluded,
        "every scene but the excluded one is compared with Chrome"
    );
    eprintln!("{compared} scenes compared with Chrome, {excluded} excluded");
    assert!(
        failures.is_empty(),
        "{} scenes differ from Chrome (the port's, Chrome's and diff images in target/chrome-export-diff/):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The port's canvas with every colour channel moved `by` levels (up, or
/// down where that would pass the pixel's alpha), alpha kept: a uniformly
/// shifted export, which no scene's tolerance may accept.
fn shifted(pixmap: &Pixmap, by: u8) -> Pixmap {
    let mut out = pixmap.clone();
    for p in out.pixels_mut() {
        let a = p.alpha();
        let shift = |c: u8| {
            if u16::from(c) + u16::from(by) <= u16::from(a) {
                c + by
            } else {
                c.saturating_sub(by)
            }
        };
        *p = PremultipliedColorU8::from_rgba(shift(p.red()), shift(p.green()), shift(p.blue()), a)
            .unwrap();
    }
    out
}

/// A port export off by a uniform 12 or 40 levels fails every scene's
/// tolerance, the excluded scene's check outside its element's box
/// included: each tier holds the pixels it covers.
#[test]
fn a_uniformly_shifted_port_canvas_fails_every_scene() {
    let tolerances = tolerances();
    for name in scene_names() {
        let recorded = &tolerances[&name];
        let actual = decode(&port_png(&name), &name);
        let expected = reference(&name);
        let mask = scene_mask(&name, recorded);
        assert!(
            passes(&actual, &expected, recorded, &mask),
            "{name}: the port's own export is outside the tolerance"
        );
        for by in [12, 40] {
            assert!(
                !passes(&shifted(&actual, by), &expected, recorded, &mask),
                "{name}: the port's canvas shifted by {by} levels passes the tolerance"
            );
        }
    }
}

/// Each tolerance is earned: a canvas holding only the reference's
/// background (its top-left pixel everywhere) must fall outside it, so no
/// scene passes by drawing nothing, but for [`BACKGROUND_ONLY`] scenes,
/// whose export the port draws as the background alone.
#[test]
fn every_scene_draws_more_than_its_tolerance() {
    let tolerances = tolerances();
    for name in scene_names() {
        let expected = reference(&name);
        let mut blank = Pixmap::new(expected.width(), expected.height()).unwrap();
        let background: PremultipliedColorU8 = expected.pixels()[0];
        blank.pixels_mut().fill(background);
        if BACKGROUND_ONLY.contains(&name.as_str()) {
            let port = decode(&port_png(&name), &name);
            assert_eq!(
                port.data(),
                blank.data(),
                "{name}: the port now draws more than the background; revisit its tolerance and BACKGROUND_ONLY"
            );
            continue;
        }
        let recorded = &tolerances[&name];
        let mask = scene_mask(&name, recorded);
        assert!(
            !passes(&blank, &expected, recorded, &mask),
            "{name}: a canvas of the background alone passes the tolerance"
        );
    }
}

/// Text is compared: without `GlyphText`, each scene with text falls
/// outside its canvas tier.
#[test]
fn text_is_held_to_chromes_fill_text() {
    let tolerances = tolerances();
    for name in TEXT_SCENES {
        let t = &tolerances[name].canvas.unwrap();
        let without = if let Some(scene) = canvas_scenes().iter().find(|s| s["name"] == name) {
            golden_png(scene, Text::None)
        } else {
            let (doc, images, _) = port_canvas(name);
            // the CLI's own canvas, with the text left out
            export_png(&doc, &images, &mut NoText).unwrap()
        };
        let diff = compare(decode(&without, name).as_ref(), reference(name).as_ref()).unwrap();
        assert!(
            !diff.within(t),
            "{name}: the scene passes its tolerance {t:?} with no text drawn"
        );
    }
}
