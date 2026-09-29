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
//! within the scene's tolerance in `tests/fixtures/chrome-export/tolerances.json`,
//! whose note says where the difference comes from.
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
use excali_raster::{export_png, TextRasterizer};
use excali_scene::display::TextRun;

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
const TEXT_SCENES: [&str; 6] = [
    "default",
    "scale-3-embed",
    "frames",
    "element-text",
    "element-text-nunito",
    "test_embedded_v1",
];

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

struct Recorded {
    tolerance: Tolerance,
    note: String,
}

fn tolerances() -> BTreeMap<String, Recorded> {
    let file = read_json(&fixture_dir().join("tolerances.json"));
    file["scenes"]
        .as_object()
        .expect("tolerances.json has scenes")
        .iter()
        .map(|(name, t)| {
            let channel = t["channel"]
                .as_u64()
                .unwrap_or_else(|| panic!("{name}: channel"));
            let pixels = t["pixels"]
                .as_u64()
                .unwrap_or_else(|| panic!("{name}: pixels"));
            let recorded = Recorded {
                tolerance: Tolerance {
                    channel: u8::try_from(channel).unwrap(),
                    pixels: usize::try_from(pixels).unwrap(),
                },
                note: t["note"].as_str().unwrap_or_default().to_owned(),
            };
            (name.clone(), recorded)
        })
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

/// The port's PNG of a golden scene: its document with the vendored
/// fonts' metrics and the decoded files, painted with `text`.
fn golden_png(scene: &Value, text: Text) -> Vec<u8> {
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
    for name in &names {
        let t = &tolerances
            .get(name)
            .unwrap_or_else(|| panic!("{name}: no tolerance"))
            .tolerance;
        let actual = decode(&port_png(name), name);
        let expected = reference(name);
        let diff = match compare(actual.as_ref(), expected.as_ref()) {
            Ok(diff) => diff,
            Err(e) => {
                // canvas size equality is required
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        summary.push(format!(
            "{name}: max {:3}; pixels over 1/8/32/64/128: {}/{}/{}/{}/{} of {}; tolerance {} x {}",
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
            diff_image(actual.as_ref(), expected.as_ref(), t)
                .unwrap()
                .save_png(out.join(format!("{name}-diff.png")))
                .unwrap();
            failures.push(format!("{name}: {}", diff.report(t)));
        }
    }
    eprintln!("{}", summary.join("\n"));
    assert!(
        failures.is_empty(),
        "{} of {} scenes differ from Chrome (the port's, Chrome's and diff images in target/chrome-export-diff/):\n{}",
        failures.len(),
        names.len(),
        failures.join("\n")
    );
}

/// Each tolerance is earned: a canvas holding only the reference's
/// background (its top-left pixel everywhere) must fall outside it, so no
/// scene passes by drawing nothing.
#[test]
fn every_scene_draws_more_than_its_tolerance() {
    let tolerances = tolerances();
    for name in scene_names() {
        let expected = reference(&name);
        let mut blank = Pixmap::new(expected.width(), expected.height()).unwrap();
        let background: PremultipliedColorU8 = expected.pixels()[0];
        blank.pixels_mut().fill(background);
        let diff = compare(blank.as_ref(), expected.as_ref()).unwrap();
        let t = &tolerances[&name].tolerance;
        assert!(
            !diff.within(t),
            "{name}: a canvas of the background alone passes the tolerance {t:?}"
        );
    }
}

/// Text is compared: without `GlyphText`, each scene with text falls
/// outside its tolerance.
#[test]
fn text_is_held_to_chromes_fill_text() {
    let tolerances = tolerances();
    for name in TEXT_SCENES {
        let t = &tolerances[name].tolerance;
        let without = if let Some(scene) = canvas_scenes().iter().find(|s| s["name"] == name) {
            golden_png(scene, Text::None)
        } else {
            let file = read_file(&scene_file(name)).unwrap();
            let scene = load_scene(&file, &mut CliEnv::new(0.0, 1)).unwrap();
            // the CLI's own canvas, with the text left out
            let canvas = png_canvas(&scene, &ExportSettings::new()).unwrap();
            export_png(&canvas.document, &canvas.images, &mut NoText).unwrap()
        };
        let diff = compare(decode(&without, name).as_ref(), reference(name).as_ref()).unwrap();
        assert!(
            !diff.within(t),
            "{name}: the scene passes its tolerance {t:?} with no text drawn"
        );
    }
}
