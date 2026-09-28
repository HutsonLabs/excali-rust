//! Freedraw shapes (ex-g201): the `freedraw` branch of upstream's
//! `_generateElementShape` (`packages/element/src/shape.ts:976-994` at the
//! pinned commit), "oredered in terms of z-index [background, stroke]":
//!
//! 1. when `isPathALoop(element.points)`, a rough.js `curve` over
//!    `getFreedrawFillCurvePoints(element)` = `simplify(points, 0.75)`
//!    (`shape.ts:580-581`) with `generateRoughOptions(element, false,
//!    isDarkMode)` (fill style and fill only for loops, `shape.ts:244-253`)
//!    and `stroke: "none"`;
//! 2. the stroke, `getFreeDrawSvgPath(element)`.
//!
//! Every case of `goldens/elements-freedraw.json` (upstream's own
//! `ShapeCache.generateElementShape`, written by
//! `tools/goldens/generate.mjs`) is compared whole: the 14 looped cases'
//! rough fill op by op, and every case's svgPath byte for byte; the
//! non-loop cases must have exactly one shape, the svgPath.

use std::path::Path;

use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::LINE_CONFIRM_THRESHOLD;
use excali_core::element::{
    Element, ElementBase, ElementKind, ElementType, FillStyle, FreedrawFields, LocalPoint,
};
use excali_rough::goldens::{ActualShape, Report, Tolerance};
use excali_rough::points_on_curve::simplify;
use excali_rough::{OpSetType, RoughGenerator, Shape};
use excali_scene::freedraw::{get_free_draw_svg_path, get_freedraw_fill_curve_points};
use excali_scene::rough_options::generate_rough_options;
use excali_scene::shape::{
    generate_freedraw_shapes, FreedrawShape, RenderConfig, ShapeError, Theme,
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

/// The case's element in the port's model; a legacy element without
/// strokeOptions gets what restore writes (`restoreFreedrawStrokeOptions`,
/// `data/restore.ts:273-287`), as in `tests/freedraw_path.rs`.
fn golden_element(c: &Value) -> Element {
    let id = c["id"].as_str().expect("id");
    let mut raw = c["element"].as_object().expect("element").clone();
    raw.entry("strokeOptions")
        .or_insert_with(|| serde_json::json!({"variability": "variable", "streamline": 0.5}));
    Element::from_map(raw).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn golden_theme(c: &Value) -> Theme {
    match c["renderConfig"]["theme"].as_str().expect("theme") {
        "dark" => Theme::Dark,
        "light" => Theme::Light,
        other => panic!("theme {other}"),
    }
}

fn actual(shapes: &[FreedrawShape]) -> Vec<ActualShape<'_>> {
    shapes
        .iter()
        .map(|s| match s {
            FreedrawShape::Rough(d) => ActualShape::Rough(d),
            FreedrawShape::SvgPath(d) => ActualShape::SvgPath(d),
        })
        .collect()
}

const LOOPED: [&str; 14] = [
    "freedraw/loop-sw1",
    "freedraw/loop-sw2",
    "freedraw/loop-sw4",
    "freedraw/loop-pressure",
    "freedraw/loop-streamline0.2",
    "freedraw/loop-constant-sw1",
    "freedraw/loop-constant-sw2",
    "freedraw/loop-constant-sw4",
    "freedraw/loop-fill-hachure",
    "freedraw/loop-fill-cross-hatch",
    "freedraw/loop-fill-zigzag",
    "freedraw/loop-fill-solid",
    "freedraw/loop-fill-constant",
    "freedraw/loop-transparent",
];

#[test]
fn every_freedraw_case_matches_upstream() {
    let file = "elements-freedraw.json";
    let doc = load(file);
    let generator = RoughGenerator::new();
    let mut report = Report::new(file);
    let (mut looped, mut open) = (Vec::new(), 0);
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        let el = golden_element(c);
        let config = RenderConfig {
            theme: golden_theme(c),
            ..RenderConfig::default()
        };
        let shapes = generate_freedraw_shapes(&el, &generator, &config)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let expected = c["shapes"].as_array().expect("shapes");
        if expected[0]["type"] == "rough" {
            // the background fill first, then the stroke
            looped.push(id.to_owned());
            assert_eq!(shapes.len(), 2, "{id}");
            let FreedrawShape::Rough(fill) = &shapes[0] else {
                panic!("{id}: the first shape is not the rough fill");
            };
            assert_eq!(fill.shape, Shape::Curve, "{id}");
            assert_eq!(fill.options.stroke, "none", "{id}");
            report.drawable(c, &expected[0]["drawable"], fill, Tolerance::Exact);
        } else {
            open += 1;
            assert_eq!(expected.len(), 1, "{id}");
            assert_eq!(shapes.len(), 1, "{id}: one shape, the svgPath");
            assert!(matches!(shapes[0], FreedrawShape::SvgPath(_)), "{id}");
        }
        report.element(c, &actual(&shapes), Tolerance::Exact);
    }
    assert_eq!(report.assert_ok(), 48 + 14);
    assert_eq!(looped, LOOPED);
    assert_eq!(open, 34);
}

/// The golden fills are not all empty: the patterned and solid ones carry a
/// fill op set and no stroke op set (`stroke: "none"`), and the transparent
/// loops draw nothing at all.
#[test]
fn looped_fills_have_fill_sets_and_no_outline() {
    let doc = load("elements-freedraw.json");
    let generator = RoughGenerator::new();
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id");
        if !LOOPED.contains(&id) {
            continue;
        }
        let el = golden_element(c);
        let shapes = generate_freedraw_shapes(&el, &generator, &RenderConfig::default()).unwrap();
        let FreedrawShape::Rough(fill) = &shapes[0] else {
            panic!("{id}");
        };
        assert!(fill.sets.iter().all(|s| s.kind != OpSetType::Path), "{id}");
        if el.base.background_color == "transparent" {
            assert!(fill.sets.is_empty(), "{id}");
            assert_eq!(fill.options.fill, None, "{id}");
        } else {
            let kind = if el.base.fill_style == FillStyle::Solid {
                OpSetType::FillPath
            } else {
                OpSetType::FillSketch
            };
            assert_eq!(fill.sets.len(), 1, "{id}");
            assert_eq!(fill.sets[0].kind, kind, "{id}");
            assert_eq!(fill.options.fill.as_deref(), Some("#a5d8ff"), "{id}");
        }
    }
}

// ---------------------------------------------------------------------------
// getFreedrawFillCurvePoints (shape.ts:580-581)

fn freedraw(points: &[LocalPoint]) -> Element {
    let mut base = ElementBase::new("fd", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = 100.0;
    base.height = 100.0;
    base.background_color = "#a5d8ff".to_owned();
    base.fill_style = FillStyle::Hachure;
    Element::new(
        base,
        ElementKind::Freedraw(FreedrawFields::new(points.to_vec(), true)),
    )
}

const SQUARE: [LocalPoint; 5] = [
    [0.0, 0.0],
    [10.0, 0.5],
    [20.0, 0.0],
    [20.0, 20.0],
    [0.0, 0.0],
];

#[test]
fn fill_curve_points_are_the_points_simplified_to_three_quarters() {
    // [10, 0.5] is 0.5 from the segment [0, 0]-[20, 0], within 0.75
    assert_eq!(
        get_freedraw_fill_curve_points(&freedraw(&SQUARE)),
        Some(vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 0.0]])
    );
    // 0.8 is beyond it
    let kept = [
        [0.0, 0.0],
        [10.0, 0.8],
        [20.0, 0.0],
        [20.0, 20.0],
        [0.0, 0.0],
    ];
    assert_eq!(
        get_freedraw_fill_curve_points(&freedraw(&kept)),
        Some(kept.to_vec())
    );
    // the golden loops, against points-on-curve's simplify
    let doc = load("elements-freedraw.json");
    for c in doc["cases"].as_array().expect("cases") {
        let el = golden_element(c);
        let ElementKind::Freedraw(fields) = &el.kind else {
            panic!("not freedraw");
        };
        assert_eq!(
            get_freedraw_fill_curve_points(&el),
            Some(simplify(&fields.points, 0.75).unwrap()),
            "{}",
            c["id"]
        );
    }
}

#[test]
fn fill_curve_points_are_only_for_freedraw() {
    let mut base = ElementBase::new("r", 0.0, 0.0, 1.0, 1.0);
    base.width = 10.0;
    base.height = 10.0;
    assert_eq!(
        get_freedraw_fill_curve_points(&Element::new(base, ElementKind::Rectangle)),
        None
    );
}

// ---------------------------------------------------------------------------
// generateElementShape, freedraw (shape.ts:976-994)

/// The fill upstream draws: `generator.curve(getFreedrawFillCurvePoints(el),
/// { ...generateRoughOptions(el, false, isDarkMode), stroke: "none" })`.
fn expected_fill(el: &Element, dark: bool) -> excali_rough::Drawable {
    let generator = RoughGenerator::new();
    let mut o = generate_rough_options(el, false, dark)
        .unwrap()
        .to_rough(generator.default_options());
    o.stroke = "none".to_owned();
    generator
        .curve(&get_freedraw_fill_curve_points(el).unwrap(), &o)
        .unwrap()
}

#[test]
fn a_loop_draws_its_fill_under_its_stroke() {
    let el = freedraw(&SQUARE);
    let shapes =
        generate_freedraw_shapes(&el, &RoughGenerator::new(), &RenderConfig::default()).unwrap();
    assert_eq!(
        shapes,
        vec![
            FreedrawShape::Rough(expected_fill(&el, false)),
            FreedrawShape::SvgPath(get_free_draw_svg_path(&el).unwrap()),
        ]
    );
}

#[test]
fn the_dark_theme_filters_the_fill() {
    let el = freedraw(&SQUARE);
    let config = RenderConfig {
        theme: Theme::Dark,
        ..RenderConfig::default()
    };
    let shapes = generate_freedraw_shapes(&el, &RoughGenerator::new(), &config).unwrap();
    let FreedrawShape::Rough(fill) = &shapes[0] else {
        panic!("no fill");
    };
    assert_eq!(
        fill.options.fill.as_deref(),
        Some(apply_dark_mode_filter("#a5d8ff", true).as_str())
    );
    assert_ne!(fill.options.fill.as_deref(), Some("#a5d8ff"));
    assert_eq!(*fill, expected_fill(&el, true));
    // the stroke path does not depend on the theme
    assert_eq!(
        shapes[1],
        FreedrawShape::SvgPath(get_free_draw_svg_path(&el).unwrap())
    );
}

#[test]
fn the_loop_test_is_is_path_a_loop_at_zoom_one() {
    let closing = |gap: f64| {
        freedraw(&[
            [0.0, 0.0],
            [40.0, 0.0],
            [40.0, 40.0],
            [0.0, 40.0],
            [0.0, gap],
        ])
    };
    let generator = RoughGenerator::new();
    let config = RenderConfig::default();
    let at =
        generate_freedraw_shapes(&closing(LINE_CONFIRM_THRESHOLD), &generator, &config).unwrap();
    assert_eq!(at.len(), 2);
    assert!(matches!(at[0], FreedrawShape::Rough(_)));
    let beyond =
        generate_freedraw_shapes(&closing(LINE_CONFIRM_THRESHOLD + 0.01), &generator, &config)
            .unwrap();
    assert_eq!(beyond.len(), 1);
    assert!(matches!(beyond[0], FreedrawShape::SvgPath(_)));
    // fewer than three points never loop
    for points in [&[][..], &[[0.0, 0.0]][..], &[[0.0, 0.0], [0.0, 0.0]][..]] {
        let shapes = generate_freedraw_shapes(&freedraw(points), &generator, &config).unwrap();
        assert_eq!(shapes.len(), 1, "{points:?}");
    }
}

#[test]
fn only_freedraw_elements_have_freedraw_shapes() {
    let mut base = ElementBase::new("r", 0.0, 0.0, 1.0, 1.0);
    base.width = 10.0;
    base.height = 10.0;
    let rect = Element::new(base, ElementKind::Rectangle);
    assert_eq!(
        generate_freedraw_shapes(&rect, &RoughGenerator::new(), &RenderConfig::default()),
        Err(ShapeError::NotAFreedraw(ElementType::Rectangle))
    );
}
