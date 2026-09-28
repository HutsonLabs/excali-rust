//! Element shapes against upstream's output (ex-208, through the ex-217
//! golden harness): every rectangle, iframe, embeddable, diamond and
//! ellipse case of the element goldens (`goldens/elements-{rectangle,
//! diamond,ellipse,iframe-like,upstream-fixtures}.json`, written by
//! `tools/goldens/generate.mjs` from upstream's own
//! `ShapeCache.generateElementShape`, `packages/element/src/shape.ts`)
//! compared op by op. A difference panics with the case and element id, the
//! shape, set and op index, and the expected and actual numbers
//! (`excali_rough::goldens`).
//!
//! Ellipse points go through `Math.cos` and `Math.sin`, which differ in the
//! last bit between platforms (see `crates/excali-rough/tests/goldens.rs`),
//! so ellipses are compared within `PLATFORM_TOLERANCE`; everything else,
//! and every option, exactly.

use std::collections::HashMap;
use std::path::Path;

use excali_core::element::Element;
use excali_rough::goldens::{ActualShape, Report, Tolerance, PLATFORM_TOLERANCE};
use excali_rough::RoughGenerator;
use excali_scene::shape::{generate_element_shape, RenderConfig, Theme};
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("golden parses")
}

/// The render config of a golden case (`embedsValidationStatus` built from
/// `validatedEmbeds`, as the generator builds it).
fn render_config(v: &Value) -> (bool, String, HashMap<String, bool>, Theme) {
    let embeds = v["validatedEmbeds"]
        .as_array()
        .expect("validatedEmbeds")
        .iter()
        .map(|id| (id.as_str().expect("id").to_owned(), true))
        .collect();
    let theme = match v["theme"].as_str().expect("theme") {
        "dark" => Theme::Dark,
        "light" => Theme::Light,
        other => panic!("theme {other}"),
    };
    (
        v["isExporting"].as_bool().expect("isExporting"),
        v["canvasBackgroundColor"]
            .as_str()
            .expect("canvasBackgroundColor")
            .to_owned(),
        embeds,
        theme,
    )
}

const BOX_TYPES: [&str; 5] = ["rectangle", "iframe", "embeddable", "diamond", "ellipse"];

/// Runs every rectangle, iframe, embeddable, diamond and ellipse case of
/// `file`; returns how many ran.
fn check_file(file: &str) -> usize {
    let doc = load(file);
    let generator = RoughGenerator::new();
    let mut report = Report::new(file);
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        let raw = c["element"].as_object().expect("element").clone();
        let ty = raw["type"].as_str().expect("type");
        if !BOX_TYPES.contains(&ty) {
            continue;
        }
        let el = Element::from_map(raw.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
        let (is_exporting, background, embeds, theme) = render_config(&c["renderConfig"]);
        let config = RenderConfig {
            is_exporting,
            canvas_background_color: &background,
            embeds_validation_status: Some(&embeds),
            theme,
        };
        let drawable = generate_element_shape(&el, &generator, &config)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let tolerance = if ty == "ellipse" {
            Tolerance::Relative(PLATFORM_TOLERANCE)
        } else {
            Tolerance::Exact
        };
        report.element(c, &[ActualShape::Rough(&drawable)], tolerance);
    }
    report.assert_ok()
}

#[test]
fn rectangles_match_upstream() {
    assert_eq!(check_file("elements-rectangle.json"), 54);
}

#[test]
fn diamonds_match_upstream() {
    assert_eq!(check_file("elements-diamond.json"), 54);
}

#[test]
fn ellipses_match_upstream() {
    assert_eq!(check_file("elements-ellipse.json"), 41);
}

#[test]
fn iframe_likes_match_upstream() {
    assert_eq!(check_file("elements-iframe-like.json"), 13);
}

#[test]
fn upstream_fixtures_match_upstream() {
    // elementFixture.ts and the export test's variants; the text fixture
    // has no rough shape
    assert_eq!(check_file("elements-upstream-fixtures.json"), 9);
}
