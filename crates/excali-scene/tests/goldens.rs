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
//! Lines and arrows (ex-209): every case of `goldens/elements-line.json`
//! whole (linearPath, filled polygon loops and curves), and the body
//! (`shapes[0]`) of every arrow in `elements-{arrow,arrowheads}.json`.
//!
//! Arrowheads (ex-212): every shape of every arrow in
//! `elements-{arrow,arrowheads}.json`, the body exactly and the heads after
//! it within `PLATFORM_TOLERANCE` (their wings are rotated with `Math.cos`
//! and `Math.sin`, and circles are rough.js ellipses).
//!
//! Elbow arrows (ex-210): the body (`shapes[0]`) of every case of
//! `goldens/elements-elbow-arrow.json`, the rounded path of
//! `generateElbowArrowShape(points, 16)`, and no shape at all for the arrow
//! beyond the 1e6 coordinate guard.
//!
//! Ellipse points go through `Math.cos` and `Math.sin`, which differ in the
//! last bit between platforms (see `crates/excali-rough/tests/goldens.rs`),
//! so ellipses are compared within `PLATFORM_TOLERANCE`; everything else,
//! and every option, exactly.

use std::collections::HashMap;
use std::path::Path;

use excali_core::element::Arrowhead;
use excali_core::element::Element;
use excali_rough::goldens::{ActualShape, Report, Tolerance, PLATFORM_TOLERANCE};
use excali_rough::{Drawable, RoughGenerator};
use excali_scene::rough_options::generate_rough_options;
use excali_scene::shape::{
    generate_elbow_arrow_shape, generate_element_shape, generate_linear_element_shapes,
    generate_linear_shape, RenderConfig, Theme,
};
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

/// ex-205 end to end: every `goldens/rough-strokes.json` case (solid,
/// dashed and dotted strokes at sw 1, 2, 4, roughness 0, 1, 2) goes from its
/// element through the port's `generate_rough_options` and excali-rough's
/// generator, and must give upstream's options and drawable: the dash
/// arrays, single stroke at `sw + 0.5`, `preserveVertices` and
/// `curveFitting` as upstream derives them, then the same ops.
#[test]
fn stroke_styles_match_upstream_from_element_to_ops() {
    let file = "rough-strokes.json";
    let doc = load(file);
    let generator = RoughGenerator::new();
    let mut report = Report::new(file);
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        let el = Element::from_map(c["element"].as_object().expect("element").clone())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let continuous = c["continuousPath"].as_bool().expect("continuousPath");
        let o = generate_rough_options(&el, continuous, false)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .to_rough(generator.default_options());
        let a = c["args"].as_array().expect("args");
        let n = |i: usize| a[i].as_f64().expect("number");
        let points = |v: &Value| -> Vec<[f64; 2]> {
            v.as_array()
                .expect("points")
                .iter()
                .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
                .collect()
        };
        let method = c["method"].as_str().expect("method");
        let drawable = match method {
            "line" => generator.line(n(0), n(1), n(2), n(3), &o),
            "rectangle" => generator.rectangle(n(0), n(1), n(2), n(3), &o),
            "ellipse" => generator.ellipse(n(0), n(1), n(2), n(3), &o),
            "circle" => generator.circle(n(0), n(1), n(2), &o),
            "polygon" => generator.polygon(&points(&a[0]), &o),
            "linearPath" => generator.linear_path(&points(&a[0]), &o),
            "curve" => generator.curve(&points(&a[0]), &o).expect("curve"),
            "path" => generator
                .path(a[0].as_str().expect("path data"), &o)
                .expect("path"),
            other => panic!("{id}: method {other}"),
        };
        let tolerance = if matches!(method, "ellipse" | "circle") {
            Tolerance::Relative(PLATFORM_TOLERANCE)
        } else {
            Tolerance::Exact
        };
        report.drawable(c, &c["drawable"], &drawable, tolerance);
    }
    assert_eq!(report.assert_ok(), 630);
}

// ---------------------------------------------------------------------------
// Lines and arrows (ex-209)

/// The line or arrow body of a golden case from the port's
/// `generate_linear_shape`, under the case's render config.
fn linear_body(c: &Value) -> Drawable {
    let id = c["id"].as_str().expect("id");
    let raw = c["element"].as_object().expect("element").clone();
    let el = Element::from_map(raw).unwrap_or_else(|e| panic!("{id}: {e}"));
    let (is_exporting, background, embeds, theme) = render_config(&c["renderConfig"]);
    let config = RenderConfig {
        is_exporting,
        canvas_background_color: &background,
        embeds_validation_status: Some(&embeds),
        theme,
    };
    generate_linear_shape(&el, &RoughGenerator::new(), &config)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn lines_match_upstream() {
    // linearPath, filled polygon loops, curves (open and filled), polygon
    // lines, seeds x roughness, stroke styles, one point: a line's shapes
    // are its body alone, so every shape of every case is compared
    let file = "elements-line.json";
    let doc = load(file);
    let mut report = Report::new(file);
    let mut seen = HashMap::<&str, usize>::new();
    for c in doc["cases"].as_array().expect("cases") {
        assert_eq!(c["element"]["type"], "line");
        let body = linear_body(c);
        *seen.entry(body.shape.as_str()).or_default() += 1;
        report.element(c, &[ActualShape::Rough(&body)], Tolerance::Exact);
    }
    assert_eq!(report.assert_ok(), 42);
    assert_eq!(seen["linearPath"], 21);
    assert_eq!(seen["polygon"], 6);
    assert_eq!(seen["curve"], 15);
}

/// Compares the body (`shapes[0]`: "curve is always the first element") of
/// every arrow in `file`; [`check_arrows`] compares the heads. Returns how
/// many ran and how many bodies of each rough.js shape there were.
fn check_arrow_bodies(file: &str) -> (usize, HashMap<&'static str, usize>) {
    let doc = load(file);
    let mut report = Report::new(file);
    let mut seen = HashMap::new();
    for c in doc["cases"].as_array().expect("cases") {
        assert_eq!(c["element"]["type"], "arrow");
        assert_ne!(c["element"]["elbowed"], true);
        let body = linear_body(c);
        *seen.entry(body.shape.as_str()).or_default() += 1;
        let expected = &c["shapes"][0];
        assert_eq!(expected["type"], "rough");
        report.drawable(c, &expected["drawable"], &body, Tolerance::Exact);
    }
    (report.assert_ok(), seen)
}

#[test]
fn arrow_bodies_match_upstream() {
    let (ran, seen) = check_arrow_bodies("elements-arrow.json");
    assert_eq!(ran, 34);
    assert_eq!(seen["linearPath"], 24);
    assert_eq!(seen["curve"], 10);
}

#[test]
fn arrowhead_case_bodies_match_upstream() {
    // every head kind at both ends, widths 1/2/4, curved, dashed, dotted,
    // short: the bodies under the heads
    let (ran, seen) = check_arrow_bodies("elements-arrowheads.json");
    assert_eq!(ran, 160);
    assert_eq!(seen.values().sum::<usize>(), 160);
    assert!(seen["curve"] > 0 && seen["linearPath"] > 0);
}

// ---------------------------------------------------------------------------
// Elbow arrows (ex-210)

#[test]
fn elbow_arrow_bodies_match_upstream() {
    // L, S and U routes, corners shrunk by short segments, a long bold
    // arrow, dashed, both heads, roughness 2: the path under the heads.
    // The arrow past the extreme-coordinate guard has no shapes at all, its
    // heads included, and the port draws no body for it.
    let file = "elements-elbow-arrow.json";
    let doc = load(file);
    let mut report = Report::new(file);
    let (mut drawn, mut skipped) = (0, 0);
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        assert_eq!(c["element"]["type"], "arrow");
        assert_eq!(c["element"]["elbowed"], true);
        let el = Element::from_map(c["element"].as_object().expect("element").clone())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let (is_exporting, background, embeds, theme) = render_config(&c["renderConfig"]);
        let config = RenderConfig {
            is_exporting,
            canvas_background_color: &background,
            embeds_validation_status: Some(&embeds),
            theme,
        };
        let body = generate_elbow_arrow_shape(&el, &RoughGenerator::new(), &config)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let expected = c["shapes"].as_array().expect("shapes");
        match body {
            Some(body) => {
                drawn += 1;
                assert_eq!(expected[0]["type"], "rough", "{id}");
                assert_eq!(expected[0]["drawable"]["shape"], "path", "{id}");
                report.drawable(c, &expected[0]["drawable"], &body, Tolerance::Exact);
            }
            None => {
                skipped += 1;
                report.element(c, &[], Tolerance::Exact);
            }
        }
    }
    assert_eq!(report.assert_ok(), 9);
    assert_eq!((drawn, skipped), (8, 1));
}

// ---------------------------------------------------------------------------
// Arrowheads (ex-212)

/// Every shape of every arrow in `file` from the port's
/// `generate_linear_element_shapes`: the body exactly, each head within
/// `PLATFORM_TOLERANCE`. Returns how many shapes were compared.
fn check_arrows(file: &str) -> usize {
    let doc = load(file);
    let generator = RoughGenerator::new();
    let mut report = Report::new(file);
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        let el = Element::from_map(c["element"].as_object().expect("element").clone())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let (is_exporting, background, embeds, theme) = render_config(&c["renderConfig"]);
        let config = RenderConfig {
            is_exporting,
            canvas_background_color: &background,
            embeds_validation_status: Some(&embeds),
            theme,
        };
        let actual = generate_linear_element_shapes(&el, &generator, &config)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let expected = c["shapes"].as_array().expect("shapes");
        assert_eq!(
            expected.len(),
            actual.len(),
            "{id}: {} shapes upstream, {} in the port",
            expected.len(),
            actual.len()
        );
        for (i, (e, a)) in expected.iter().zip(&actual).enumerate() {
            assert_eq!(e["type"], "rough", "{id}");
            let tolerance = if i == 0 {
                Tolerance::Exact
            } else {
                Tolerance::Relative(PLATFORM_TOLERANCE)
            };
            report.drawable(c, &e["drawable"], a, tolerance);
        }
    }
    report.assert_ok()
}

#[test]
fn arrows_with_their_heads_match_upstream() {
    // 34 arrows, most with the default end head (an arrow: two lines)
    assert_eq!(check_arrows("elements-arrow.json"), 102);
}

#[test]
fn every_arrowhead_matches_upstream() {
    // all fourteen kinds at either end and both, sw 1/2/4, curved, dashed,
    // dotted, short, and the outline fills on a tinted and a dark canvas
    assert_eq!(check_arrows("elements-arrowheads.json"), 442);
}

/// The acceptance of ex-212: a golden for every arrowhead at stroke widths
/// 1, 2 and 4, at the start and at the end, and outline variants filled
/// with the canvas background (tinted and dark-filtered).
#[test]
fn arrowhead_goldens_cover_every_kind_width_and_outline_fill() {
    let doc = load("elements-arrowheads.json");
    let cases = doc["cases"].as_array().expect("cases");
    for head in Arrowhead::ALL {
        for sw in [1, 2, 4] {
            for (key, other) in [
                ("startArrowhead", "endArrowhead"),
                ("endArrowhead", "startArrowhead"),
            ] {
                assert!(
                    cases.iter().any(|c| c["element"][key] == head.as_str()
                        && c["element"][other].is_null()
                        && c["element"]["strokeWidth"] == sw),
                    "no golden for {} as {key} at sw {sw}",
                    head.as_str()
                );
            }
        }
    }
    // in the goldens themselves the outline heads fill with the canvas
    // background (dark-filtered on a dark canvas), the others with the
    // stroke colour
    let fill_of = |id: &str, shape: usize| -> Value {
        let c = cases.iter().find(|c| c["id"] == id).expect(id);
        c["shapes"][shape]["drawable"]["options"]["fill"].clone()
    };
    let dark = excali_core::color::apply_dark_mode_filter("#ffffff", true);
    for kind in ["circle_outline", "triangle_outline", "diamond_outline"] {
        assert_eq!(fill_of(&format!("arrowhead/{kind}-end-sw2"), 1), "#ffffff");
        assert_eq!(
            fill_of(&format!("arrowhead/{kind}-tinted-canvas"), 1),
            "#fff9db"
        );
        assert_eq!(fill_of(&format!("arrowhead/{kind}-dark"), 1), dark.as_str());
    }
    for kind in ["circle", "triangle", "diamond"] {
        assert_eq!(fill_of(&format!("arrowhead/{kind}-end-sw2"), 1), "#1e1e1e");
    }
    assert_eq!(
        fill_of("arrowhead/cardinality_zero_or_one-end-sw2", 1),
        "#ffffff"
    );
}
