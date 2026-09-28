//! Shape construction for lines and arrows (ex-209): the body upstream's
//! `_generateElementShape` builds for a `line` or a non-elbow `arrow`
//! (`packages/element/src/shape.ts:890-935`), and the loop test that decides
//! whether a line fills (`isPathALoop`, `packages/element/src/utils.ts:512-526`,
//! read by `generateRoughOptions`, `shape.ts:243-252`);
//! `site/content/research/rendering.md` section 2 (line / arrow).
//!
//! - No roundness: `generator.polygon(points)` when the options carry a
//!   (truthy) fill, which only a line closing into a loop has; otherwise
//!   `generator.linearPath(points)`.
//! - Roundness: `generator.curve(points)`, filled when the line loops.
//! - Empty points are drawn as the single point `[0, 0]`.
//! - Arrows never fill; their heads (ex-212) and elbow arrows (ex-210) are
//!   built elsewhere.
//!
//! The goldens are compared op by op in `tests/goldens.rs`.

use excali_core::constants::LINE_CONFIRM_THRESHOLD;
use excali_core::element::{
    ArrowFields, Element, ElementBase, ElementKind, ElementType, FillStyle, LineFields,
    LinearFields, LocalPoint, Roundness, RoundnessType,
};
use excali_rough::{Drawable, RoughGenerator, Shape};
use excali_scene::rough_options::generate_rough_options;
use excali_scene::shape::{generate_linear_shape, RenderConfig, ShapeError, Theme};
use excali_scene::utils::is_path_a_loop;

// ---------------------------------------------------------------------------
// Elements

fn element(kind: ElementKind) -> Element {
    let mut base = ElementBase::new("el", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = 200.0;
    base.height = 100.0;
    base.background_color = "#a5d8ff".to_owned();
    base.fill_style = FillStyle::Hachure;
    Element::new(base, kind)
}

fn line(points: &[LocalPoint]) -> Element {
    element(ElementKind::Line(LineFields {
        linear: LinearFields::new(points.to_vec()),
        polygon: false,
    }))
}

fn arrow(points: &[LocalPoint], elbowed: bool) -> Element {
    element(ElementKind::Arrow(ArrowFields::new(
        LinearFields::new(points.to_vec()),
        elbowed,
    )))
}

fn round(mut el: Element) -> Element {
    el.base.roundness = Some(Roundness::new(RoundnessType::ProportionalRadius));
    el
}

fn shape(el: &Element) -> Drawable {
    generate_linear_shape(el, &RoughGenerator::new(), &RenderConfig::default()).unwrap()
}

/// The rough options upstream passes for a non-elbow line or arrow:
/// `generateRoughOptions(element, false, isDarkMode)` over the generator's
/// defaults.
fn options(el: &Element, dark: bool) -> excali_rough::Options {
    generate_rough_options(el, false, dark)
        .unwrap()
        .to_rough(RoughGenerator::new().default_options())
}

const OPEN: [LocalPoint; 4] = [[0.0, 0.0], [80.0, 40.0], [120.0, -20.0], [200.0, 60.0]];
const CLOSED: [LocalPoint; 5] = [
    [0.0, 0.0],
    [120.0, 0.0],
    [160.0, 80.0],
    [40.0, 100.0],
    [0.0, 0.0],
];

// ---------------------------------------------------------------------------
// isPathALoop (utils.ts:512-526)

#[test]
fn a_loop_needs_three_points() {
    assert!(!is_path_a_loop(&[], 1.0));
    assert!(!is_path_a_loop(&[[0.0, 0.0]], 1.0));
    // two coincident points are not a loop, three are
    assert!(!is_path_a_loop(&[[5.0, 5.0], [5.0, 5.0]], 1.0));
    assert!(is_path_a_loop(&[[5.0, 5.0], [9.0, 9.0], [5.0, 5.0]], 1.0));
}

#[test]
fn a_loop_closes_within_the_confirm_threshold_over_zoom() {
    assert_eq!(LINE_CONFIRM_THRESHOLD, 8.0);
    // pointDistance is Math.hypot: a 3-4-5 triangle scaled to 8 is on the
    // threshold, which is inclusive
    let on = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [4.8, 6.4]];
    assert!(is_path_a_loop(&on, 1.0));
    let past = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [4.8, 6.41]];
    assert!(!is_path_a_loop(&past, 1.0));
    // the first and last points are measured, the middle ones are not
    let wide = [[10.0, 10.0], [-1e9, 1e9], [1e9, -1e9], [16.0, 10.0]];
    assert!(is_path_a_loop(&wide, 1.0));
    // zoomed in the threshold shrinks, zoomed out it grows
    assert!(!is_path_a_loop(&on, 1.25));
    assert!(is_path_a_loop(&on, 0.8));
    assert!(is_path_a_loop(&past, 0.5));
    let far = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 16.0]];
    assert!(is_path_a_loop(&far, 0.5));
    assert!(!is_path_a_loop(&far, 0.51));
}

#[test]
fn a_non_finite_gap_is_not_a_loop() {
    // NaN <= threshold is false; Infinity is never within it
    let nan = [[0.0, 0.0], [100.0, 0.0], [f64::NAN, 0.0]];
    assert!(!is_path_a_loop(&nan, 1.0));
    let inf = [[0.0, 0.0], [100.0, 0.0], [f64::INFINITY, 0.0]];
    assert!(!is_path_a_loop(&inf, 1.0));
    // zoom 0 makes the threshold Infinity: every gap but NaN closes
    assert!(is_path_a_loop(&inf, 0.0));
    assert!(!is_path_a_loop(&nan, 0.0));
}

// ---------------------------------------------------------------------------
// Sharp lines and arrows: linearPath, or polygon when filled

#[test]
fn an_open_sharp_line_is_a_linear_path() {
    let generator = RoughGenerator::new();
    let el = line(&OPEN);
    let o = options(&el, false);
    assert!(o.fill.is_none());
    let d = shape(&el);
    assert_eq!(d.shape, Shape::LinearPath);
    assert_eq!(d, generator.linear_path(&OPEN, &o));
}

#[test]
fn a_filled_sharp_loop_is_a_polygon() {
    let generator = RoughGenerator::new();
    let el = line(&CLOSED);
    let o = options(&el, false);
    assert_eq!(o.fill.as_deref(), Some("#a5d8ff"));
    assert_eq!(o.fill_style, "hachure");
    let d = shape(&el);
    assert_eq!(d.shape, Shape::Polygon);
    assert_eq!(d, generator.polygon(&CLOSED, &o));
    // the fill set comes before the outline
    assert_eq!(d.sets.len(), 2);
}

#[test]
fn a_nearly_closed_line_fills_like_a_closed_one() {
    // the last point 5px from the first: isPathALoop says closed, and the
    // polygon is drawn through the points as given (rough.js closes it)
    let generator = RoughGenerator::new();
    let mut points = CLOSED.to_vec();
    *points.last_mut().unwrap() = [3.0, 4.0];
    let el = line(&points);
    let d = shape(&el);
    assert_eq!(d.shape, Shape::Polygon);
    assert_eq!(d, generator.polygon(&points, &options(&el, false)));

    // 9px away it is open, and not filled
    *points.last_mut().unwrap() = [0.0, 9.0];
    let el = line(&points);
    let d = shape(&el);
    assert_eq!(d.shape, Shape::LinearPath);
    assert_eq!(d, generator.linear_path(&points, &options(&el, false)));
}

#[test]
fn a_transparent_loop_is_not_filled() {
    // generateRoughOptions compares with the string "transparent" for
    // lines; the shape then falls back to linearPath
    let mut el = line(&CLOSED);
    el.base.background_color = "transparent".to_owned();
    assert_eq!(shape(&el).shape, Shape::LinearPath);
}

#[test]
fn an_empty_fill_is_falsy() {
    // `if (options.fill)`: an empty background string is set as the fill
    // but is falsy, so the loop is drawn as a linear path
    let generator = RoughGenerator::new();
    let mut el = line(&CLOSED);
    el.base.background_color = String::new();
    let o = options(&el, false);
    assert_eq!(o.fill.as_deref(), Some(""));
    let d = shape(&el);
    assert_eq!(d.shape, Shape::LinearPath);
    assert_eq!(d, generator.linear_path(&CLOSED, &o));
}

#[test]
fn the_polygon_flag_does_not_decide_the_fill() {
    // shape.ts reads isPathALoop, not element.polygon
    let generator = RoughGenerator::new();
    let mut el = line(&OPEN);
    if let ElementKind::Line(l) = &mut el.kind {
        l.polygon = true;
    }
    assert_eq!(
        shape(&el),
        generator.linear_path(&OPEN, &options(&el, false))
    );
}

#[test]
fn a_dark_loop_fills_with_the_filtered_colour() {
    let generator = RoughGenerator::new();
    let el = line(&CLOSED);
    let config = RenderConfig {
        theme: Theme::Dark,
        ..RenderConfig::default()
    };
    let o = options(&el, true);
    assert_ne!(o.fill.as_deref(), Some("#a5d8ff"));
    let d = generate_linear_shape(&el, &generator, &config).unwrap();
    assert_eq!(d, generator.polygon(&CLOSED, &o));
}

#[test]
fn arrows_never_fill() {
    let generator = RoughGenerator::new();
    let el = arrow(&CLOSED, false);
    let o = options(&el, false);
    assert!(o.fill.is_none());
    let d = shape(&el);
    assert_eq!(d.shape, Shape::LinearPath);
    assert_eq!(d, generator.linear_path(&CLOSED, &o));
}

#[test]
fn empty_points_draw_the_origin() {
    // "points array can be empty in the beginning, so it is important to
    // add initial position to it"
    let generator = RoughGenerator::new();
    for el in [line(&[]), arrow(&[], false)] {
        let o = options(&el, false);
        assert_eq!(shape(&el), generator.linear_path(&[[0.0, 0.0]], &o));
        let el = round(el);
        assert_eq!(
            shape(&el),
            generator.curve(&[[0.0, 0.0]], &o).unwrap(),
            "{:?}",
            el.element_type()
        );
    }
    // a single point is drawn as it is
    let el = line(&[[3.0, 4.0]]);
    assert_eq!(
        shape(&el),
        generator.linear_path(&[[3.0, 4.0]], &options(&el, false))
    );
}

// ---------------------------------------------------------------------------
// Round lines and arrows: curve

#[test]
fn round_lines_and_arrows_are_curves() {
    let generator = RoughGenerator::new();
    for el in [round(line(&OPEN)), round(arrow(&OPEN, false))] {
        let o = options(&el, false);
        assert!(o.fill.is_none());
        let d = shape(&el);
        assert_eq!(d.shape, Shape::Curve);
        assert_eq!(d, generator.curve(&OPEN, &o).unwrap());
        assert_eq!(d.sets.len(), 1);
    }
    // any roundness type draws a curve
    for kind in [
        RoundnessType::Legacy,
        RoundnessType::ProportionalRadius,
        RoundnessType::AdaptiveRadius,
    ] {
        let mut el = line(&OPEN);
        el.base.roundness = Some(Roundness::new(kind));
        assert_eq!(shape(&el).shape, Shape::Curve, "{kind:?}");
    }
}

#[test]
fn a_round_loop_is_a_filled_curve() {
    let generator = RoughGenerator::new();
    for style in [
        FillStyle::Hachure,
        FillStyle::CrossHatch,
        FillStyle::Zigzag,
        FillStyle::Solid,
    ] {
        let mut el = round(line(&CLOSED));
        el.base.fill_style = style;
        let o = options(&el, false);
        let d = shape(&el);
        assert_eq!(d.shape, Shape::Curve);
        assert_eq!(d, generator.curve(&CLOSED, &o).unwrap(), "{style:?}");
        assert_eq!(d.sets.len(), 2, "{style:?}: fill, then the outline");
    }
    // a round arrow over the same points is a curve without a fill
    let el = round(arrow(&CLOSED, false));
    assert_eq!(shape(&el).sets.len(), 1);
}

#[test]
fn line_options_are_not_continuous() {
    // generateRoughOptions(element, false, ...): roughness 2 and up drops
    // preserveVertices for lines and arrows alike
    for mut el in [line(&OPEN), arrow(&OPEN, false), round(line(&OPEN))] {
        el.base.roughness = 2.0;
        assert!(!shape(&el).options.preserve_vertices);
        el.base.roughness = 1.0;
        assert!(shape(&el).options.preserve_vertices);
    }
}

// ---------------------------------------------------------------------------
// What this builder does not draw

#[test]
fn elbow_arrows_are_not_built_here() {
    let el = arrow(&OPEN, true);
    assert_eq!(
        generate_linear_shape(&el, &RoughGenerator::new(), &RenderConfig::default()),
        Err(ShapeError::ElbowArrow)
    );
}

#[test]
fn other_types_are_not_linear_shapes() {
    for (kind, ty) in [
        (ElementKind::Rectangle, ElementType::Rectangle),
        (ElementKind::Ellipse, ElementType::Ellipse),
        (ElementKind::Diamond, ElementType::Diamond),
    ] {
        assert_eq!(
            generate_linear_shape(
                &element(kind),
                &RoughGenerator::new(),
                &RenderConfig::default()
            ),
            Err(ShapeError::NotALinearShape(ty))
        );
    }
}
