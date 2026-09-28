//! Shape construction (ex-208): rectangles, iframes and embeddables
//! (`generator.rectangle`, or the rounded `Q`-corner path), diamonds
//! (`generator.polygon`, or the rounded `C`-corner path), ellipses
//! (`generator.ellipse` with `curveFitting: 1`) and the iframe-like colour
//! defaults, from `packages/element/src/shape.ts:262-293, 761-889`,
//! `getCornerRadius` (`utils.ts:528-549`) and `getDiamondPoints`
//! (`bounds.ts:522-535`); `site/content/research/rendering.md` section 2.
//!
//! The first half restates upstream's formulas as unit tests: the corner
//! radius table (proportional and legacy `x * 0.25`; adaptive `x * 0.25` up
//! to the cutoff `R / 0.25`, then `R`, with `R = roundness.value ?? 32`), the
//! diamond points (`floor(w / 2) + 1`, `floor(h / 2) + 1`), the exact SVG
//! path strings upstream hands to rough.js, and
//! `modifyIframeLikeForRoughOptions`. The second half runs every case of
//! the element goldens (`goldens/elements-{rectangle,diamond,ellipse,
//! iframe-like,upstream-fixtures}.json`, written by
//! `tools/goldens/generate.mjs` from upstream's own
//! `ShapeCache.generateElementShape`) and compares the drawables op by op.

use std::collections::HashMap;
use std::path::Path;

use excali_core::element::{
    Element, ElementBase, ElementKind, ElementType, FillStyle, LineFields, LinearFields, Roundness,
    RoundnessType,
};
use excali_rough::{Drawable, Op, Options, RoughGenerator, Shape};
use excali_scene::bounds::get_diamond_points;
use excali_scene::rough_options::generate_rough_options;
use excali_scene::shape::{
    diamond_path, generate_element_shape, modify_iframe_like_for_rough_options, rectangle_path,
    RenderConfig, ShapeError, Theme,
};
use excali_scene::utils::get_corner_radius;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Elements

fn element(kind: ElementKind, width: f64, height: f64) -> Element {
    let mut base = ElementBase::new("el", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = width;
    base.height = height;
    Element::new(base, kind)
}

fn rounded(mut el: Element, kind: RoundnessType, value: Option<f64>) -> Element {
    el.base.roundness = Some(Roundness { kind, value });
    el
}

fn rect() -> Element {
    element(ElementKind::Rectangle, 200.0, 120.0)
}

fn with_colors(mut el: Element, stroke: &str, background: &str) -> Element {
    el.base.stroke_color = stroke.to_owned();
    el.base.background_color = background.to_owned();
    el
}

// ---------------------------------------------------------------------------
// getCornerRadius (utils.ts:528-549)

#[test]
fn corner_radius_is_zero_without_roundness() {
    let el = rect();
    for x in [0.0, 1.0, 50.0, 128.0, 1000.0] {
        assert_eq!(get_corner_radius(x, &el), 0.0, "x = {x}");
    }
}

#[test]
fn corner_radius_is_proportional_for_legacy_and_proportional() {
    // x * DEFAULT_PROPORTIONAL_RADIUS (0.25), whatever the value says
    for kind in [RoundnessType::Legacy, RoundnessType::ProportionalRadius] {
        for value in [None, Some(10.0)] {
            let el = rounded(rect(), kind, value);
            for (x, r) in [
                (0.0, 0.0),
                (1.0, 0.25),
                (10.0, 2.5),
                (101.0, 25.25),
                (128.0, 32.0),
                (129.0, 32.25),
                (1000.0, 250.0),
                (-8.0, -2.0),
            ] {
                assert_eq!(get_corner_radius(x, &el), r, "{kind:?} {value:?} x = {x}");
            }
        }
    }
}

#[test]
fn corner_radius_is_adaptive_with_a_cutoff() {
    // R = roundness.value ?? DEFAULT_ADAPTIVE_RADIUS (32); cutoff R / 0.25
    let el = rounded(rect(), RoundnessType::AdaptiveRadius, None);
    for (x, r) in [
        (0.0, 0.0),
        (10.0, 2.5),
        (100.0, 25.0),
        (127.0, 31.75),
        (128.0, 32.0), // x <= CUTOFF_SIZE (128) is still proportional
        (128.5, 32.0),
        (129.0, 32.0),
        (300.0, 32.0),
        (1e9, 32.0),
        (-4.0, -1.0),
    ] {
        assert_eq!(get_corner_radius(x, &el), r, "x = {x}");
    }

    // an explicit value moves the cutoff: 10 / 0.25 = 40
    let el = rounded(rect(), RoundnessType::AdaptiveRadius, Some(10.0));
    for (x, r) in [(20.0, 5.0), (40.0, 10.0), (40.5, 10.0), (120.0, 10.0)] {
        assert_eq!(get_corner_radius(x, &el), r, "value 10, x = {x}");
    }

    // value 0 is not nullish: cutoff 0, radius 0 above it
    let el = rounded(rect(), RoundnessType::AdaptiveRadius, Some(0.0));
    assert_eq!(get_corner_radius(0.0, &el), 0.0);
    assert_eq!(get_corner_radius(-4.0, &el), -1.0);
    assert_eq!(get_corner_radius(50.0, &el), 0.0);
}

// ---------------------------------------------------------------------------
// getDiamondPoints (bounds.ts:522-535)

#[test]
fn diamond_points_add_one_to_the_floored_halves() {
    let d = |w: f64, h: f64| get_diamond_points(&element(ElementKind::Diamond, w, h));
    // [topX, topY, rightX, rightY, bottomX, bottomY, leftX, leftY]
    assert_eq!(
        d(200.0, 120.0),
        [101.0, 0.0, 200.0, 61.0, 101.0, 120.0, 0.0, 61.0]
    );
    assert_eq!(
        d(101.0, 77.0),
        [51.0, 0.0, 101.0, 39.0, 51.0, 77.0, 0.0, 39.0]
    );
    assert_eq!(d(0.0, 0.0), [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
    assert_eq!(d(1.5, 3.9), [1.0, 0.0, 1.5, 2.0, 1.0, 3.9, 0.0, 2.0]);
    // Math.floor rounds toward negative infinity
    assert_eq!(
        d(-5.0, -3.0),
        [-2.0, 0.0, -5.0, -1.0, -2.0, -3.0, 0.0, -1.0]
    );
}

// ---------------------------------------------------------------------------
// The SVG paths upstream builds (shape.ts:806-812, 841-867)

#[test]
fn rounded_rectangle_path_uses_quadratic_corners() {
    assert_eq!(
        rectangle_path(200.0, 120.0, 30.0),
        "M 30 0 L 170 0 Q 200 0, 200 30 L 200 90 Q 200 120, 170 120 \
         L 30 120 Q 0 120, 0 90 L 0 30 Q 0 0, 30 0"
    );
    // numbers are written as JavaScript's Number::toString writes them
    assert_eq!(
        rectangle_path(0.1 + 0.2, 1e21, 0.5),
        "M 0.5 0 L -0.19999999999999996 0 Q 0.30000000000000004 0, 0.30000000000000004 0.5 \
         L 0.30000000000000004 1e+21 Q 0.30000000000000004 1e+21, -0.19999999999999996 1e+21 \
         L 0.5 1e+21 Q 0 1e+21, 0 1e+21 L 0 0.5 Q 0 0, 0.5 0"
    );
}

#[test]
fn rounded_diamond_path_uses_cubic_corners() {
    // 200 x 120, proportional: vertical radius 0.25 * |101 - 0|, horizontal
    // radius 0.25 * |61 - 0|; the template's line breaks and 12-space
    // indents are part of the string.
    let points = [101.0, 0.0, 200.0, 61.0, 101.0, 120.0, 0.0, 61.0];
    let expected = [
        "M 126.25 15.25 L 174.75 45.75",
        "            C 200 61, 200 61, 174.75 76.25",
        "            L 126.25 104.75",
        "            C 101 120, 101 120, 75.75 104.75",
        "            L 25.25 76.25",
        "            C 0 61, 0 61, 25.25 45.75",
        "            L 75.75 15.25",
        "            C 101 0, 101 0, 126.25 15.25",
    ]
    .join("\n");
    assert_eq!(diamond_path(points, 25.25, 15.25), expected);
}

// ---------------------------------------------------------------------------
// modifyIframeLikeForRoughOptions (shape.ts:262-293)

fn status(pairs: &[(&str, bool)]) -> HashMap<String, bool> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

#[test]
fn transparent_iframe_like_becomes_a_grey_placeholder() {
    for kind in [ElementKind::Iframe, ElementKind::Embeddable] {
        let el = with_colors(element(kind, 200.0, 120.0), "transparent", "#ff000000");
        let m = modify_iframe_like_for_rough_options(&el, true, None);
        assert_eq!(m.base.roughness, 0.0);
        assert_eq!(m.base.background_color, "#d3d3d3");
        assert_eq!(m.base.fill_style, FillStyle::Solid);
        // the stroke stays transparent
        assert_eq!(m.base.stroke_color, "transparent");
    }
}

#[test]
fn unvalidated_embeddables_are_placeholders_in_the_editor() {
    let mut el = with_colors(
        element(ElementKind::Embeddable, 200.0, 120.0),
        "transparent",
        "transparent",
    );
    el.base.fill_style = FillStyle::Hachure;
    for embeds in [None, Some(status(&[])), Some(status(&[("el", false)]))] {
        let m = modify_iframe_like_for_rough_options(&el, false, embeds.as_ref());
        assert_eq!(m.base.roughness, 0.0);
        assert_eq!(m.base.background_color, "#d3d3d3");
        assert_eq!(m.base.fill_style, FillStyle::Solid);
    }
    // a validated embeddable keeps its own look
    let embeds = status(&[("el", true)]);
    let m = modify_iframe_like_for_rough_options(&el, false, Some(&embeds));
    assert_eq!(*m, el);
}

#[test]
fn iframes_get_default_colours() {
    // In the editor an iframe is never a placeholder; transparent colours
    // become #000000 stroke and #f4f4f6 background.
    let el = with_colors(
        element(ElementKind::Iframe, 200.0, 120.0),
        "transparent",
        "transparent",
    );
    let m = modify_iframe_like_for_rough_options(&el, false, None);
    assert_eq!(m.base.stroke_color, "#000000");
    assert_eq!(m.base.background_color, "#f4f4f6");
    assert_eq!(m.base.roughness, el.base.roughness);
    assert_eq!(m.base.fill_style, el.base.fill_style);

    // only the transparent colour is replaced
    let el = with_colors(
        element(ElementKind::Iframe, 200.0, 120.0),
        "#e03131",
        "transparent",
    );
    let m = modify_iframe_like_for_rough_options(&el, true, None);
    assert_eq!(m.base.stroke_color, "#e03131");
    assert_eq!(m.base.background_color, "#f4f4f6");
    let el = with_colors(
        element(ElementKind::Iframe, 200.0, 120.0),
        "transparent",
        "#a5d8ff",
    );
    let m = modify_iframe_like_for_rough_options(&el, true, None);
    assert_eq!(m.base.stroke_color, "#000000");
    assert_eq!(m.base.background_color, "#a5d8ff");
}

#[test]
fn other_elements_are_left_alone() {
    let el = with_colors(rect(), "transparent", "transparent");
    assert_eq!(*modify_iframe_like_for_rough_options(&el, true, None), el);
    let el = with_colors(
        element(ElementKind::Embeddable, 200.0, 120.0),
        "#1e1e1e",
        "transparent",
    );
    assert_eq!(*modify_iframe_like_for_rough_options(&el, true, None), el);
}

// ---------------------------------------------------------------------------
// Shapes

fn shape(el: &Element) -> Drawable {
    generate_element_shape(el, &RoughGenerator::new(), &RenderConfig::default()).unwrap()
}

#[test]
fn sharp_shapes_use_rough_primitives() {
    let generator = RoughGenerator::new();
    let el = rect();
    let o = generate_rough_options(&el, false, false)
        .unwrap()
        .to_rough(generator.default_options());
    assert_eq!(shape(&el), generator.rectangle(0.0, 0.0, 200.0, 120.0, &o));

    let el = element(ElementKind::Diamond, 200.0, 120.0);
    let o = generate_rough_options(&el, false, false)
        .unwrap()
        .to_rough(generator.default_options());
    let points = [[101.0, 0.0], [200.0, 61.0], [101.0, 120.0], [0.0, 61.0]];
    assert_eq!(shape(&el), generator.polygon(&points, &o));
}

#[test]
fn ellipses_fit_curves_exactly() {
    let el = element(ElementKind::Ellipse, 200.0, 120.0);
    let d = shape(&el);
    assert_eq!(d.shape, Shape::Ellipse);
    assert_eq!(d.options.curve_fitting, 1.0);
    let generator = RoughGenerator::new();
    let o = generate_rough_options(&el, false, false)
        .unwrap()
        .to_rough(generator.default_options());
    assert_eq!(d, generator.ellipse(100.0, 60.0, 200.0, 120.0, &o));
}

#[test]
fn rounded_shapes_are_continuous_paths() {
    let generator = RoughGenerator::new();
    let el = rounded(rect(), RoundnessType::AdaptiveRadius, None);
    let mut o = generate_rough_options(&el, true, false)
        .unwrap()
        .to_rough(generator.default_options());
    assert!(o.preserve_vertices);
    assert_eq!(
        shape(&el),
        generator
            .path(&rectangle_path(200.0, 120.0, 30.0), &o)
            .unwrap()
    );

    // roughness 2 keeps preserveVertices only because the path is continuous
    let mut el = rounded(
        element(ElementKind::Diamond, 200.0, 120.0),
        RoundnessType::ProportionalRadius,
        None,
    );
    el.base.roughness = 2.0;
    o = generate_rough_options(&el, true, false)
        .unwrap()
        .to_rough(generator.default_options());
    assert!(o.preserve_vertices);
    let d = shape(&el);
    assert_eq!(d.shape, Shape::Path);
    assert_eq!(
        d,
        generator
            .path(&diamond_path(get_diamond_points(&el), 25.25, 15.25), &o)
            .unwrap()
    );
}

#[test]
fn rough_options_merge_over_the_generator_defaults() {
    let mut el = rounded(
        element(ElementKind::Ellipse, 200.0, 120.0),
        RoundnessType::ProportionalRadius,
        None,
    );
    el.base.stroke_style = excali_core::element::StrokeStyle::Dashed;
    el.base.background_color = "#a5d8ff".to_owned();
    el.base.fill_style = FillStyle::CrossHatch;
    el.base.seed = 2147483648.0 + 7.0; // ToInt32 wraps like Math.imul
    let o = generate_rough_options(&el, false, true)
        .unwrap()
        .to_rough(&Options::default());
    assert_eq!(o.seed, -2147483641);
    assert_eq!(o.stroke_line_dash, Some(vec![8.0, 10.0]));
    assert!(o.disable_multi_stroke);
    assert_eq!(o.stroke_width, 2.5);
    assert_eq!(o.fill_weight, 1.0);
    assert_eq!(o.hachure_gap, 8.0);
    assert_eq!(o.fill_style, "cross-hatch");
    assert!(o.fill.is_some());
    assert_eq!(o.curve_fitting, 1.0);
    // untouched defaults
    assert_eq!(o.max_randomness_offset, 2.0);
    assert_eq!(o.bowing, 1.0);
    assert_eq!(o.hachure_angle, -41.0);
}

#[test]
fn other_types_are_not_box_shapes() {
    let el = element(
        ElementKind::Line(LineFields {
            linear: LinearFields::new(vec![[0.0, 0.0], [10.0, 10.0]]),
            polygon: false,
        }),
        10.0,
        10.0,
    );
    assert_eq!(
        generate_element_shape(&el, &RoughGenerator::new(), &RenderConfig::default()),
        Err(ShapeError::NotABoxShape(ElementType::Line))
    );
}

// ---------------------------------------------------------------------------
// Upstream's output: ShapeCache.generateElementShape

/// Relative tolerance for ellipses, whose points go through `Math.cos` and
/// `Math.sin` (see `crates/excali-rough/tests/goldens.rs`).
const PLATFORM_TOLERANCE: f64 = 1e-10;

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

/// The resolved options as the goldens record them.
fn options_json(o: &Options) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_owned(), v);
    };
    put("maxRandomnessOffset", json!(o.max_randomness_offset));
    put("roughness", json!(o.roughness));
    put("bowing", json!(o.bowing));
    put("stroke", json!(o.stroke));
    put("strokeWidth", json!(o.stroke_width));
    put("curveTightness", json!(o.curve_tightness));
    put("curveFitting", json!(o.curve_fitting));
    put("curveStepCount", json!(o.curve_step_count));
    put("fillStyle", json!(o.fill_style));
    put("fillWeight", json!(o.fill_weight));
    put("hachureAngle", json!(o.hachure_angle));
    put("hachureGap", json!(o.hachure_gap));
    put("dashOffset", json!(o.dash_offset));
    put("dashGap", json!(o.dash_gap));
    put("zigzagOffset", json!(o.zigzag_offset));
    put("seed", json!(o.seed));
    put("disableMultiStroke", json!(o.disable_multi_stroke));
    put("disableMultiStrokeFill", json!(o.disable_multi_stroke_fill));
    put("preserveVertices", json!(o.preserve_vertices));
    put("fillShapeRoughnessGain", json!(o.fill_shape_roughness_gain));
    if let Some(v) = &o.fill {
        put("fill", json!(v));
    }
    if let Some(v) = o.simplification {
        put("simplification", json!(v));
    }
    if let Some(v) = &o.stroke_line_dash {
        put("strokeLineDash", json!(v));
    }
    if let Some(v) = o.stroke_line_dash_offset {
        put("strokeLineDashOffset", json!(v));
    }
    if let Some(v) = &o.fill_line_dash {
        put("fillLineDash", json!(v));
    }
    if let Some(v) = o.fill_line_dash_offset {
        put("fillLineDashOffset", json!(v));
    }
    if let Some(v) = o.fixed_decimal_place_digits {
        put("fixedDecimalPlaceDigits", json!(v));
    }
    Value::Object(m)
}

fn op_json(op: &Op) -> Value {
    json!({ "op": op.name(), "data": op.data() })
}

fn drawable_json(d: &Drawable) -> Value {
    json!({
        "shape": d.shape.as_str(),
        "options": options_json(&d.options),
        "sets": d.sets.iter().map(|s| json!({
            "type": s.kind.as_str(),
            "ops": s.ops.iter().map(op_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// Structural equality; numbers exactly or within `tolerance` relative to
/// the larger of 1 and the expected magnitude.
fn same(actual: &Value, expected: &Value, tolerance: f64) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().expect("f64"), b.as_f64().expect("f64"));
            a == b || (a - b).abs() <= tolerance * b.abs().max(1.0)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y, tolerance))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| same(v, w, tolerance)))
        }
        _ => actual == expected,
    }
}

/// The first differing path, for a readable failure.
fn first_difference(actual: &Value, expected: &Value, tolerance: f64, at: &str) -> String {
    match (actual, expected) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find(|(_, (x, y))| !same(x, y, tolerance))
            .map(|(i, (x, y))| first_difference(x, y, tolerance, &format!("{at}[{i}]")))
            .unwrap_or_default(),
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => a
            .iter()
            .find(|(k, v)| !b.get(*k).is_some_and(|w| same(v, w, tolerance)))
            .map(|(k, v)| first_difference(v, &b[k], tolerance, &format!("{at}.{k}")))
            .unwrap_or_default(),
        _ => format!("{at}: got {actual}, upstream {expected}"),
    }
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
    let mut ran = 0;
    let mut failures = Vec::new();
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
        let shapes = c["shapes"].as_array().expect("shapes");
        assert_eq!(shapes.len(), 1, "{id}: one drawable");
        assert_eq!(shapes[0]["type"], "rough", "{id}");
        let expected = &shapes[0]["drawable"];
        let drawable = generate_element_shape(&el, &generator, &config)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let actual = drawable_json(&drawable);
        let tolerance = if ty == "ellipse" {
            PLATFORM_TOLERANCE
        } else {
            0.0
        };
        if !same(&actual, expected, tolerance) {
            failures.push(format!(
                "{id}: {}",
                first_difference(&actual, expected, tolerance, "drawable")
            ));
        }
        ran += 1;
    }
    assert!(
        failures.is_empty(),
        "{file}: {} of {ran} cases differ from upstream:\n{}",
        failures.len(),
        failures.join("\n")
    );
    ran
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
