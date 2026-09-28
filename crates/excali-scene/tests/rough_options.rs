//! Option mapping (ex-207): `generateRoughOptions` and `adjustRoughness`
//! from `packages/element/src/shape.ts:166-260`, the option table of
//! `site/content/research/rendering.md` section 1.
//!
//! The first half restates every row of that table and the three
//! "keep the roughness" branches of `adjustRoughness` as unit tests. The
//! second half checks the port against upstream's own output
//! (`fixtures/rough-options.json`, written by
//! `tools/goldens/rough-options.mjs` from the pinned checkout).

use excali_core::element::{
    ArrowFields, Element, ElementBase, ElementKind, FillStyle, FrameFields, FreedrawFields,
    ImageFields, LineFields, LinearFields, LocalPoint, Roundness, RoundnessType,
    StickyNoteFields, StrokeStyle, TextFields,
};
use excali_scene::rough_options::{
    adjust_roughness, dash_array_dashed, dash_array_dotted, generate_rough_options, RoughOptions,
    UnimplementedType,
};
use excali_scene::utils::is_path_a_loop;
use serde_json::{Map, Value};

const FIXTURE: &str = include_str!("fixtures/rough-options.json");

// ---------------------------------------------------------------------------
// Elements

fn element(kind: ElementKind, width: f64, height: f64) -> Element {
    let mut base = ElementBase::new("el", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = width;
    base.height = height;
    Element::new(base, kind)
}

fn rect() -> Element {
    element(ElementKind::Rectangle, 100.0, 100.0)
}

fn line(points: &[LocalPoint]) -> Element {
    element(
        ElementKind::Line(LineFields {
            linear: LinearFields::new(points.to_vec()),
            polygon: false,
        }),
        100.0,
        100.0,
    )
}

fn arrow(points: &[LocalPoint]) -> Element {
    element(
        ElementKind::Arrow(ArrowFields::new(LinearFields::new(points.to_vec()), false)),
        100.0,
        100.0,
    )
}

fn freedraw(points: &[LocalPoint]) -> Element {
    element(
        ElementKind::Freedraw(FreedrawFields::new(points.to_vec(), true)),
        100.0,
        100.0,
    )
}

fn options(el: &Element) -> RoughOptions {
    generate_rough_options(el, false, false).unwrap()
}

const CLOSED: [LocalPoint; 4] = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 0.0]];
const OPEN: [LocalPoint; 3] = [[0.0, 0.0], [50.0, 50.0], [100.0, 0.0]];

// ---------------------------------------------------------------------------
// The option table, row by row

#[test]
fn seed_is_the_element_seed() {
    // shape.ts:201
    let mut el = rect();
    for seed in [0.0, 1.0, 7.0, 1041657908.0, 2147483648.0, -5.0] {
        el.base.seed = seed;
        assert_eq!(options(&el).seed, seed);
    }
}

#[test]
fn dash_arrays_are_upstreams() {
    // shape.ts:168-170
    assert_eq!(dash_array_dashed(2.0), [8.0, 10.0]);
    assert_eq!(dash_array_dashed(0.5), [8.0, 8.5]);
    assert_eq!(dash_array_dotted(2.0), [1.5, 8.0]);
    assert_eq!(dash_array_dotted(4.0), [1.5, 10.0]);
}

#[test]
fn stroke_line_dash_follows_the_stroke_style() {
    // shape.ts:202-207
    let mut el = rect();
    el.base.stroke_width = 4.0;
    el.base.stroke_style = StrokeStyle::Solid;
    assert_eq!(options(&el).stroke_line_dash, None);
    el.base.stroke_style = StrokeStyle::Dashed;
    assert_eq!(options(&el).stroke_line_dash, Some([8.0, 12.0]));
    el.base.stroke_style = StrokeStyle::Dotted;
    assert_eq!(options(&el).stroke_line_dash, Some([1.5, 10.0]));
}

#[test]
fn multi_stroke_is_disabled_for_non_solid_strokes() {
    // shape.ts:210
    let mut el = rect();
    for (style, disabled) in [
        (StrokeStyle::Solid, false),
        (StrokeStyle::Dashed, true),
        (StrokeStyle::Dotted, true),
    ] {
        el.base.stroke_style = style;
        assert_eq!(options(&el).disable_multi_stroke, disabled, "{style:?}");
    }
}

#[test]
fn non_solid_strokes_are_half_a_pixel_wider() {
    // shape.ts:213-216
    let mut el = rect();
    for sw in [1.0, 2.0, 4.0] {
        el.base.stroke_width = sw;
        el.base.stroke_style = StrokeStyle::Solid;
        assert_eq!(options(&el).stroke_width, sw);
        el.base.stroke_style = StrokeStyle::Dashed;
        assert_eq!(options(&el).stroke_width, sw + 0.5);
        el.base.stroke_style = StrokeStyle::Dotted;
        assert_eq!(options(&el).stroke_width, sw + 0.5);
    }
}

#[test]
fn fill_weight_and_hachure_gap_use_the_unwidened_stroke_width() {
    // shape.ts:220-221
    let mut el = rect();
    for style in [StrokeStyle::Solid, StrokeStyle::Dashed, StrokeStyle::Dotted] {
        el.base.stroke_style = style;
        for sw in [1.0, 2.0, 4.0, 3.3] {
            el.base.stroke_width = sw;
            let o = options(&el);
            assert_eq!(o.fill_weight, sw / 2.0);
            assert_eq!(o.hachure_gap, sw * 4.0);
        }
    }
}

#[test]
fn roughness_goes_through_adjust_roughness() {
    // shape.ts:222
    let mut el = rect();
    el.base.roughness = 2.0;
    assert_eq!(options(&el).roughness, 2.0);
    el.base.width = 5.0;
    el.base.height = 5.0;
    assert_eq!(options(&el).roughness, adjust_roughness(&el));
    assert_eq!(options(&el).roughness, 2.0 / 3.0);
}

#[test]
fn stroke_goes_through_the_dark_mode_filter() {
    // shape.ts:223
    let el = rect();
    assert_eq!(options(&el).stroke, "#1e1e1e");
    let dark = generate_rough_options(&el, false, true).unwrap();
    assert_eq!(dark.stroke, "#d3d3d3");
}

#[test]
fn vertices_are_preserved_for_continuous_paths_or_below_cartoonist() {
    // shape.ts:224-225: ROUGHNESS.cartoonist is 2
    let mut el = rect();
    for (roughness, preserved) in [(0.0, true), (1.0, true), (1.99, true), (2.0, false), (3.0, false)]
    {
        el.base.roughness = roughness;
        let o = generate_rough_options(&el, false, false).unwrap();
        assert_eq!(o.preserve_vertices, preserved, "roughness {roughness}");
        let o = generate_rough_options(&el, true, false).unwrap();
        assert!(o.preserve_vertices, "continuousPath, roughness {roughness}");
    }
}

#[test]
fn closed_shapes_take_fill_style_and_fill() {
    // shape.ts:229-237
    for kind in [
        ElementKind::Rectangle,
        ElementKind::Iframe,
        ElementKind::Embeddable,
        ElementKind::Diamond,
        ElementKind::Ellipse,
    ] {
        let mut el = element(kind, 100.0, 100.0);
        el.base.fill_style = FillStyle::Zigzag;
        el.base.background_color = "transparent".into();
        let o = options(&el);
        assert_eq!(o.fill_style, Some(FillStyle::Zigzag));
        assert_eq!(o.fill, None);
        // isTransparent, not a string compare
        el.base.background_color = "rgba(0,0,0,0)".into();
        assert_eq!(options(&el).fill, None);
        el.base.background_color = "#a5d8ff".into();
        assert_eq!(options(&el).fill.as_deref(), Some("#a5d8ff"));
        let dark = generate_rough_options(&el, false, true).unwrap();
        assert_eq!(dark.fill.as_deref(), Some("#154162"));
        assert_eq!(
            o.keys().last(),
            Some(if el.element_type().as_str() == "ellipse" {
                &"curveFitting"
            } else {
                &"fill"
            })
        );
    }
}

#[test]
fn every_fill_style_is_passed_through() {
    let mut el = rect();
    for style in [
        FillStyle::Hachure,
        FillStyle::CrossHatch,
        FillStyle::Solid,
        FillStyle::Zigzag,
    ] {
        el.base.fill_style = style;
        assert_eq!(options(&el).fill_style, Some(style));
    }
}

#[test]
fn curve_fitting_is_one_for_ellipses_only() {
    // shape.ts:238-240
    assert_eq!(
        options(&element(ElementKind::Ellipse, 100.0, 100.0)).curve_fitting,
        Some(1.0)
    );
    for el in [
        rect(),
        element(ElementKind::Diamond, 100.0, 100.0),
        line(&CLOSED),
        arrow(&OPEN),
    ] {
        assert_eq!(options(&el).curve_fitting, None);
    }
}

#[test]
fn lines_and_freedraw_fill_only_loops() {
    // shape.ts:243-252
    for make in [line as fn(&[LocalPoint]) -> Element, freedraw] {
        let mut el = make(&CLOSED);
        el.base.fill_style = FillStyle::CrossHatch;
        el.base.background_color = "#b2f2bb".into();
        let o = options(&el);
        assert_eq!(o.fill_style, Some(FillStyle::CrossHatch));
        assert_eq!(o.fill.as_deref(), Some("#b2f2bb"));
        // A literal "transparent" check here, not isTransparent.
        el.base.background_color = "transparent".into();
        let o = options(&el);
        assert_eq!(o.fill_style, Some(FillStyle::CrossHatch));
        assert_eq!(o.fill, None);
        el.base.background_color = "rgba(0,0,0,0)".into();
        assert_eq!(options(&el).fill.as_deref(), Some("rgba(0,0,0,0)"));

        let mut open = make(&OPEN);
        open.base.background_color = "#b2f2bb".into();
        let o = options(&open);
        assert_eq!((o.fill_style, o.fill), (None, None));
        assert!(!o.keys().contains(&"fill"));
    }
}

#[test]
fn arrows_never_fill() {
    // shape.ts:254-255
    let mut el = arrow(&CLOSED);
    el.base.background_color = "#b2f2bb".into();
    let o = options(&el);
    assert_eq!((o.fill_style, o.fill, o.curve_fitting), (None, None, None));
}

#[test]
fn other_types_are_unimplemented() {
    // shape.ts:256-258
    let text = TextFields::new("", Default::default(), 1.25);
    for kind in [
        ElementKind::Selection,
        ElementKind::StickyNote(StickyNoteFields { base_height: 100.0 }),
        ElementKind::Image(ImageFields::default()),
        ElementKind::Frame(FrameFields { name: None }),
        ElementKind::MagicFrame(FrameFields { name: None }),
        ElementKind::Text(text),
    ] {
        let el = element(kind, 100.0, 100.0);
        let err = generate_rough_options(&el, false, false).unwrap_err();
        assert_eq!(err, UnimplementedType(el.element_type()));
        assert_eq!(
            err.to_string(),
            format!("Unimplemented type {}", el.element_type())
        );
    }
}

#[test]
fn keys_are_in_upstreams_order() {
    let base = [
        "seed",
        "strokeLineDash",
        "disableMultiStroke",
        "strokeWidth",
        "fillWeight",
        "hachureGap",
        "roughness",
        "stroke",
        "preserveVertices",
    ];
    assert_eq!(options(&arrow(&OPEN)).keys(), base);
    let mut filled = base.to_vec();
    filled.extend(["fillStyle", "fill"]);
    assert_eq!(options(&rect()).keys(), filled);
    filled.push("curveFitting");
    assert_eq!(
        options(&element(ElementKind::Ellipse, 1.0, 1.0)).keys(),
        filled
    );
}

// ---------------------------------------------------------------------------
// adjustRoughness: the three branches that keep the roughness, and the
// reduction

fn sized(mut el: Element, width: f64, height: f64, roughness: f64) -> Element {
    el.base.width = width;
    el.base.height = height;
    el.base.roughness = roughness;
    el
}

#[test]
fn adjust_roughness_keeps_it_when_both_sides_are_big() {
    // minSize >= 20 && maxSize >= 50 (shape.ts:180-181)
    assert_eq!(adjust_roughness(&sized(rect(), 20.0, 50.0, 2.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(rect(), 50.0, 20.0, 2.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(rect(), 19.99, 50.0, 2.0)), 1.0);
    assert_eq!(adjust_roughness(&sized(rect(), 20.0, 49.99, 2.0)), 1.0);
}

#[test]
fn adjust_roughness_keeps_it_for_rounded_elements_of_15px() {
    // minSize >= 15 && roundness && canChangeRoundness (shape.ts:183-185)
    let mut round = rect();
    round.base.roundness = Some(Roundness::new(RoundnessType::AdaptiveRadius));
    assert_eq!(adjust_roughness(&sized(round.clone(), 15.0, 15.0, 2.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(round.clone(), 14.99, 15.0, 2.0)), 1.0);
    assert_eq!(adjust_roughness(&sized(rect(), 15.0, 15.0, 2.0)), 1.0);
    // an ellipse cannot change roundness, whatever its roundness field says
    let mut ellipse = element(ElementKind::Ellipse, 0.0, 0.0);
    ellipse.base.roundness = Some(Roundness::new(RoundnessType::ProportionalRadius));
    assert_eq!(adjust_roughness(&sized(ellipse, 15.0, 15.0, 2.0)), 1.0);
}

#[test]
fn adjust_roughness_keeps_it_for_long_linear_elements() {
    // isLinearElement && maxSize >= 50 (shape.ts:187)
    assert_eq!(adjust_roughness(&sized(line(&OPEN), 0.0, 50.0, 2.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(arrow(&OPEN), 50.0, 0.0, 2.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(arrow(&OPEN), 49.99, 0.0, 2.0)), 1.0);
    // freedraw is not linear
    assert_eq!(adjust_roughness(&sized(freedraw(&OPEN), 50.0, 0.0, 2.0)), 1.0);
}

#[test]
fn adjust_roughness_reduces_small_elements() {
    // Math.min(roughness / (maxSize < 10 ? 3 : 2), 2.5) (shape.ts:192)
    assert_eq!(adjust_roughness(&sized(rect(), 9.99, 9.99, 3.0)), 1.0);
    assert_eq!(adjust_roughness(&sized(rect(), 10.0, 9.99, 3.0)), 1.5);
    assert_eq!(adjust_roughness(&sized(rect(), 30.0, 30.0, 4.0)), 2.0);
    assert_eq!(adjust_roughness(&sized(rect(), 30.0, 30.0, 9.0)), 2.5);
    assert_eq!(adjust_roughness(&sized(rect(), 5.0, 5.0, 9.0)), 2.5);
    assert_eq!(adjust_roughness(&sized(rect(), 5.0, 5.0, 0.0)), 0.0);
    // negative sizes compare as numbers
    assert_eq!(adjust_roughness(&sized(rect(), -60.0, 100.0, 2.0)), 1.0);
}

#[test]
fn path_loops_close_within_the_confirm_threshold() {
    // utils.ts:512-526
    assert!(is_path_a_loop(&CLOSED, 1.0));
    assert!(!is_path_a_loop(&OPEN, 1.0));
    let near = [[0.0, 0.0], [100.0, 0.0], [50.0, 50.0], [8.0, 0.0]];
    assert!(is_path_a_loop(&near, 1.0));
    assert!(!is_path_a_loop(&near, 2.0));
    assert!(is_path_a_loop(&near, 0.5));
    let far = [[0.0, 0.0], [100.0, 0.0], [50.0, 50.0], [8.001, 0.0]];
    assert!(!is_path_a_loop(&far, 1.0));
    assert!(!is_path_a_loop(&[[0.0, 0.0], [0.0, 0.0]], 1.0));
    assert!(is_path_a_loop(&[[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]], 1.0));
    assert!(!is_path_a_loop(&[], 1.0));
}

// ---------------------------------------------------------------------------
// Upstream's output

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

/// `{...base, ...typeFields[type], ...overrides}`, as the generator builds it.
fn fixture_element(fixture: &Value, overrides: &Map<String, Value>) -> Element {
    let mut map = fixture["base"].as_object().unwrap().clone();
    let ty = overrides
        .get("type")
        .or(map.get("type"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    for (k, v) in fixture["typeFields"][ty.as_str()].as_object().unwrap() {
        map.insert(k.clone(), v.clone());
    }
    for (k, v) in overrides {
        map.insert(k.clone(), v.clone());
    }
    Element::from_map(map).unwrap_or_else(|e| panic!("{ty}: {e}"))
}

fn num(value: f64) -> Value {
    Value::from(value)
}

/// The options as `JSON.stringify` writes them: `None` values dropped.
fn to_json(o: &RoughOptions) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("seed".into(), num(o.seed));
    if let Some(dash) = o.stroke_line_dash {
        m.insert("strokeLineDash".into(), dash.iter().copied().map(num).collect());
    }
    m.insert("disableMultiStroke".into(), o.disable_multi_stroke.into());
    m.insert("strokeWidth".into(), num(o.stroke_width));
    m.insert("fillWeight".into(), num(o.fill_weight));
    m.insert("hachureGap".into(), num(o.hachure_gap));
    m.insert("roughness".into(), num(o.roughness));
    m.insert("stroke".into(), o.stroke.clone().into());
    m.insert("preserveVertices".into(), o.preserve_vertices.into());
    if let Some(style) = o.fill_style {
        m.insert("fillStyle".into(), serde_json::to_value(style).unwrap());
    }
    if let Some(fill) = &o.fill {
        m.insert("fill".into(), fill.clone().into());
    }
    if let Some(c) = o.curve_fitting {
        m.insert("curveFitting".into(), num(c));
    }
    m
}

/// Numbers compared as f64 (the fixture has integers, the port floats).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter().zip(y).all(|((ka, a), (kb, b))| ka == kb && same(a, b))
        }
        _ => a == b,
    }
}

#[test]
fn generate_rough_options_matches_upstream() {
    let fixture = fixture();
    let cases = fixture["options"].as_array().unwrap();
    assert!(cases.len() > 300);
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let el = fixture_element(&fixture, case["element"].as_object().unwrap());
        let got = generate_rough_options(
            &el,
            case["continuousPath"].as_bool().unwrap(),
            case["isDarkMode"].as_bool().unwrap(),
        );
        match (got, case.get("error")) {
            (Err(e), Some(error)) => {
                if Value::String(format!("Error: {e}")) != *error {
                    failures.push(format!("{id}: error {e}, upstream {error}"));
                }
            }
            (Ok(o), None) => {
                let json = Value::Object(to_json(&o));
                if !same(&json, &case["options"]) {
                    failures.push(format!("{id}: {json}\n  upstream {}", case["options"]));
                }
                let keys: Vec<Value> = o.keys().into_iter().map(Value::from).collect();
                if Value::Array(keys.clone()) != case["keys"] {
                    failures.push(format!("{id}: keys {keys:?}, upstream {}", case["keys"]));
                }
            }
            (got, _) => failures.push(format!("{id}: {got:?}, upstream {case}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn adjust_roughness_matches_upstream() {
    let fixture = fixture();
    let cases = fixture["adjustRoughness"].as_array().unwrap();
    assert!(cases.len() > 1400);
    let mut failures = Vec::new();
    for case in cases {
        let mut overrides = case.as_object().unwrap().clone();
        let expected = overrides.remove("result").unwrap().as_f64().unwrap();
        let el = fixture_element(&fixture, &overrides);
        let got = adjust_roughness(&el);
        let via_options = options(&el).roughness;
        if got != expected || via_options != expected {
            failures.push(format!(
                "{}: {got} / {via_options}, upstream {expected}",
                Value::Object(overrides)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dark_mode_filter_matches_upstream() {
    let fixture = fixture();
    let cases = fixture["darkMode"].as_array().unwrap();
    assert!(cases.len() > 30);
    let mut failures = Vec::new();
    for case in cases {
        let color = case["color"].as_str().unwrap();
        let got = excali_core::color::apply_dark_mode_filter(color, true);
        if Some(got.as_str()) != case["result"].as_str() {
            failures.push(format!("{color:?}: {got}, upstream {}", case["result"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
