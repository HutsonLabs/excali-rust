//! Arrowheads (ex-212): the geometry of `getArrowheadSize`,
//! `getArrowheadAngle` and `getArrowheadPoints`
//! (`packages/element/src/bounds.ts:710-909`) and the rough.js shapes
//! `getArrowheadShapes` builds from it (`packages/element/src/shape.ts:
//! 295-577`), pushed after an arrow's body by `_generateElementShape`
//! (`shape.ts:936-975`); `site/content/research/rendering.md` section 2
//! (arrowhead shapes and geometry) and `site/content/research/data-model.md`
//! section 2 (the fourteen kinds).
//!
//! - Sizes: arrow 25, diamond 12, crowfoot 15, cardinality marker 20,
//!   everything else 15. Angles: bar 90, arrow 20, everything else 25.
//! - The head points along the last (or first) bezier op evaluated at
//!   t = 0.3, and shrinks to half (a quarter for diamonds) of the last
//!   segment's length.
//! - Line heads cap roughness at 1 and are always solid, except dotted
//!   arrows, whose heads use `[d0, d1 - 1]` of `getDashArrayDotted(sw - 1)`;
//!   circles cap roughness at 0.5; outline variants fill with the canvas
//!   background, the others with the stroke colour.
//!
//! The goldens of every kind, at stroke widths 1, 2 and 4, are compared op
//! by op in `tests/goldens.rs`.

use excali_core::element::{
    ArrowFields, Arrowhead, Element, ElementBase, ElementKind, LineFields, LinearFields,
    LocalPoint, StrokeStyle,
};
use excali_math::Degrees;
use excali_rough::{Drawable, RoughGenerator, Shape};
use excali_scene::bounds::{
    get_arrowhead_angle, get_arrowhead_points, get_arrowhead_size, ArrowheadPoints,
    ArrowheadPosition, InvalidArrowheadOp,
};
use excali_scene::shape::{
    generate_elbow_arrow_shape, generate_linear_element_shapes, generate_linear_shape,
    RenderConfig, ShapeError, Theme,
};

// ---------------------------------------------------------------------------
// Elements

fn arrow_with(points: &[LocalPoint], start: Option<Arrowhead>, end: Option<Arrowhead>) -> Element {
    let mut base = ElementBase::new("arrow", 0.0, 0.0, 1041657908.0, 1.0);
    base.width = 100.0;
    base.height = 40.0;
    base.stroke_color = "#1e1e1e".to_owned();
    base.roughness = 0.0;
    let mut linear = LinearFields::new(points.to_vec());
    linear.start_arrowhead = start;
    linear.end_arrowhead = end;
    Element::new(base, ElementKind::Arrow(ArrowFields::new(linear, false)))
}

const STRAIGHT: [LocalPoint; 2] = [[0.0, 0.0], [100.0, 0.0]];

fn body(el: &Element) -> Vec<Drawable> {
    vec![generate_linear_shape(el, &RoughGenerator::new(), &RenderConfig::default()).unwrap()]
}

fn points_of(
    el: &Element,
    pos: ArrowheadPosition,
    head: Arrowhead,
    offset: f64,
) -> ArrowheadPoints {
    get_arrowhead_points(el, &body(el), pos, head, offset)
        .unwrap()
        .expect("a head")
}

fn close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} vs {expected:?}");
    }
}

fn shapes(el: &Element) -> Vec<Drawable> {
    generate_linear_element_shapes(el, &RoughGenerator::new(), &RenderConfig::default()).unwrap()
}

fn heads(el: &Element) -> Vec<Drawable> {
    shapes(el).split_off(1)
}

fn kinds(ds: &[Drawable]) -> Vec<Shape> {
    ds.iter().map(|d| d.shape).collect()
}

// ---------------------------------------------------------------------------
// getArrowheadSize, getArrowheadAngle (bounds.ts:710-744)

#[test]
fn sizes_per_kind() {
    use Arrowhead::*;
    let expected = [
        (Arrow, 25.0),
        (Bar, 15.0),
        (Circle, 15.0),
        (CircleOutline, 15.0),
        (Triangle, 15.0),
        (TriangleOutline, 15.0),
        (Diamond, 12.0),
        (DiamondOutline, 12.0),
        (CardinalityOne, 20.0),
        (CardinalityMany, 15.0),
        (CardinalityOneOrMany, 15.0),
        (CardinalityExactlyOne, 20.0),
        (CardinalityZeroOrOne, 20.0),
        (CardinalityZeroOrMany, 15.0),
    ];
    assert_eq!(expected.len(), Arrowhead::ALL.len());
    for (head, size) in expected {
        assert_eq!(get_arrowhead_size(head), size, "{head:?}");
    }
}

#[test]
fn angles_per_kind() {
    for head in Arrowhead::ALL {
        let expected = match head {
            Arrowhead::Bar => 90.0,
            Arrowhead::Arrow => 20.0,
            _ => 25.0,
        };
        assert_eq!(get_arrowhead_angle(head), Degrees(expected), "{head:?}");
    }
}

// ---------------------------------------------------------------------------
// getArrowheadPoints (bounds.ts:746-909)

#[test]
fn arrow_wings_rotate_the_base_by_twenty_degrees_about_the_tip() {
    // roughness 0: the body is the straight segment, so the head points
    // along +x; size 25 fits in half of the 100 px segment
    let el = arrow_with(&STRAIGHT, None, Some(Arrowhead::Arrow));
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let ArrowheadPoints::Wings(p) = points_of(&el, ArrowheadPosition::End, Arrowhead::Arrow, 0.0)
    else {
        panic!("wings")
    };
    close(
        &p,
        &[
            100.0,
            0.0,
            100.0 - 25.0 * c,
            25.0 * s,
            100.0 - 25.0 * c,
            -25.0 * s,
        ],
    );
}

#[test]
fn start_heads_point_back_along_the_first_op() {
    let el = arrow_with(&STRAIGHT, Some(Arrowhead::Bar), None);
    let ArrowheadPoints::Wings(p) = points_of(&el, ArrowheadPosition::Start, Arrowhead::Bar, 0.0)
    else {
        panic!("wings")
    };
    // bar: 90 degrees, size 15, tip at the first point
    close(&p, &[0.0, 0.0, 0.0, -15.0, 0.0, 15.0]);
}

#[test]
fn heads_shrink_to_half_the_last_segment() {
    // last segment 20 long: min(25, 20 * 0.5) = 10
    let el = arrow_with(&[[0.0, 0.0], [80.0, 0.0], [100.0, 0.0]], None, None);
    let ArrowheadPoints::Wings(p) = points_of(&el, ArrowheadPosition::End, Arrowhead::Arrow, 0.0)
    else {
        panic!("wings")
    };
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    close(
        &p,
        &[
            100.0,
            0.0,
            100.0 - 10.0 * c,
            10.0 * s,
            100.0 - 10.0 * c,
            -10.0 * s,
        ],
    );
    // diamonds to a quarter: min(12, 20 * 0.25) = 5, so the far vertex is
    // 2 * 5 behind the tip
    let ArrowheadPoints::Diamond(d) =
        points_of(&el, ArrowheadPosition::End, Arrowhead::Diamond, 0.0)
    else {
        panic!("diamond")
    };
    assert!((d[4] - 90.0).abs() < 1e-9 && d[5].abs() < 1e-9, "{d:?}");
}

#[test]
fn circles_are_centred_on_the_tip_with_the_stroke_width_added() {
    for sw in [1.0, 2.0, 4.0] {
        let mut el = arrow_with(&STRAIGHT, None, Some(Arrowhead::Circle));
        el.base.stroke_width = sw;
        let ArrowheadPoints::Circle(c) =
            points_of(&el, ArrowheadPosition::End, Arrowhead::Circle, 0.0)
        else {
            panic!("circle")
        };
        // diameter = hypot(base - tip) + strokeWidth - 2
        close(&c, &[100.0, 0.0, 15.0 + sw - 2.0]);
    }
}

#[test]
fn diamonds_add_the_vertex_opposite_the_tip() {
    let el = arrow_with(&STRAIGHT, None, Some(Arrowhead::Diamond));
    let ArrowheadPoints::Diamond(d) =
        points_of(&el, ArrowheadPosition::End, Arrowhead::Diamond, 0.0)
    else {
        panic!("diamond")
    };
    let (c, s) = (25f64.to_radians().cos(), 25f64.to_radians().sin());
    close(
        &d,
        &[
            100.0,
            0.0,
            100.0 - 12.0 * c,
            12.0 * s,
            76.0,
            0.0,
            100.0 - 12.0 * c,
            -12.0 * s,
        ],
    );
    let el = arrow_with(&STRAIGHT, Some(Arrowhead::Diamond), None);
    let ArrowheadPoints::Diamond(d) =
        points_of(&el, ArrowheadPosition::Start, Arrowhead::Diamond, 0.0)
    else {
        panic!("diamond")
    };
    close(
        &d,
        &[0.0, 0.0, 12.0 * c, -12.0 * s, 24.0, 0.0, 12.0 * c, 12.0 * s],
    );
}

#[test]
fn crowfeet_swap_the_tip_and_the_base() {
    // cardinality_many: the wings open from the base point towards the
    // line's end, which is returned first
    let el = arrow_with(&STRAIGHT, None, Some(Arrowhead::CardinalityMany));
    let ArrowheadPoints::Wings(p) =
        points_of(&el, ArrowheadPosition::End, Arrowhead::CardinalityMany, 0.0)
    else {
        panic!("wings")
    };
    let (c, s) = (25f64.to_radians().cos(), 25f64.to_radians().sin());
    close(
        &p,
        &[
            85.0,
            0.0,
            85.0 + 15.0 * c,
            -15.0 * s,
            85.0 + 15.0 * c,
            15.0 * s,
        ],
    );
}

#[test]
fn the_offset_moves_the_tip_back_along_the_line() {
    // cardinality_one at offset -0.5 (exactly one, zero or one): the tip is
    // moved half a size *forward*, tx = x2 - nx * size * offset
    let el = arrow_with(&STRAIGHT, None, Some(Arrowhead::CardinalityOne));
    let ArrowheadPoints::Wings(p) =
        points_of(&el, ArrowheadPosition::End, Arrowhead::CardinalityOne, -0.5)
    else {
        panic!("wings")
    };
    assert!((p[0] - 110.0).abs() < 1e-9 && p[1].abs() < 1e-9, "{p:?}");
    // circle_outline at 1.5: 1.5 sizes back from the end
    let ArrowheadPoints::Circle(c) =
        points_of(&el, ArrowheadPosition::End, Arrowhead::CircleOutline, 1.5)
    else {
        panic!("circle")
    };
    close(&c, &[77.5, 0.0, 15.0 + el.base.stroke_width - 2.0]);
}

#[test]
fn no_head_without_a_body_op() {
    // a one-point arrow's linearPath has no ops
    let el = arrow_with(&[[0.0, 0.0]], None, Some(Arrowhead::Arrow));
    let shape = body(&el);
    assert!(shape[0].sets[0].ops.is_empty());
    assert_eq!(
        get_arrowhead_points(&el, &shape, ArrowheadPosition::End, Arrowhead::Arrow, 0.0),
        Ok(None)
    );
    assert_eq!(
        get_arrowhead_points(&el, &[], ArrowheadPosition::End, Arrowhead::Arrow, 0.0),
        Ok(None)
    );
    // and so no head shapes, only the body
    assert_eq!(shapes(&el).len(), 1);
}

#[test]
fn a_non_bezier_op_breaks_the_invariant() {
    // upstream's `invariant(data.length === 6, "Op data length is not 6")`
    let el = arrow_with(&STRAIGHT, None, Some(Arrowhead::Arrow));
    let mut shape = body(&el);
    let ops = &mut shape[0].sets[0].ops;
    let last = ops.len() - 1;
    ops[last] = excali_rough::Op::LineTo([100.0, 0.0]);
    let err = get_arrowhead_points(&el, &shape, ArrowheadPosition::End, Arrowhead::Arrow, 0.0)
        .unwrap_err();
    assert_eq!(err, InvalidArrowheadOp::DataLength(2));
    assert_eq!(err.to_string(), "Op data length is not 6");
}

// ---------------------------------------------------------------------------
// getArrowheadShapes (shape.ts:295-577)

#[test]
fn shapes_per_kind() {
    use Arrowhead::*;
    use Shape::{Circle as C, Line as L, Polygon as P};
    let expected: [(Arrowhead, &[Shape]); 14] = [
        (Arrow, &[L, L]),
        (Bar, &[L, L]),
        (Circle, &[C]),
        (CircleOutline, &[C]),
        (Triangle, &[P]),
        (TriangleOutline, &[P]),
        (Diamond, &[P]),
        (DiamondOutline, &[P]),
        (CardinalityOne, &[L]),
        (CardinalityMany, &[L, L]),
        (CardinalityOneOrMany, &[L, L, L]),
        (CardinalityExactlyOne, &[L, L]),
        (CardinalityZeroOrOne, &[C, L]),
        (CardinalityZeroOrMany, &[L, L, C]),
    ];
    for (head, want) in expected {
        let el = arrow_with(&STRAIGHT, None, Some(head));
        assert_eq!(kinds(&heads(&el)), want, "end {head:?}");
        let el = arrow_with(&STRAIGHT, Some(head), None);
        assert_eq!(kinds(&heads(&el)), want, "start {head:?}");
        // start heads come before end heads
        let el = arrow_with(&STRAIGHT, Some(head), Some(Arrowhead::Triangle));
        let both = heads(&el);
        assert_eq!(kinds(&both[..want.len()]), want);
        assert_eq!(kinds(&both[want.len()..]), [P]);
    }
}

#[test]
fn no_heads_no_extra_shapes() {
    let el = arrow_with(&STRAIGHT, None, None);
    assert_eq!(kinds(&shapes(&el)), [Shape::LinearPath]);
}

#[test]
fn an_arrow_without_an_end_arrowhead_key_gets_an_arrow() {
    // `const { startArrowhead = null, endArrowhead = "arrow" } = element`
    // (shape.ts:937): only a missing key defaults, null stays no head
    let el = arrow_with(&STRAIGHT, None, None);
    let mut legacy = el.to_map();
    legacy.shift_remove("startArrowhead");
    legacy.shift_remove("endArrowhead");
    let legacy = Element::from_map(legacy).unwrap();
    assert_eq!(
        kinds(&shapes(&legacy)),
        [Shape::LinearPath, Shape::Line, Shape::Line]
    );
    let with_arrow = arrow_with(&STRAIGHT, None, Some(Arrowhead::Arrow));
    assert_eq!(shapes(&legacy), shapes(&with_arrow));
}

#[test]
fn lines_never_get_heads() {
    let mut linear = LinearFields::new(STRAIGHT.to_vec());
    linear.start_arrowhead = Some(Arrowhead::Arrow);
    linear.end_arrowhead = Some(Arrowhead::Arrow);
    let el = Element::new(
        ElementBase::new("line", 0.0, 0.0, 1.0, 1.0),
        ElementKind::Line(LineFields {
            linear,
            polygon: false,
        }),
    );
    assert_eq!(kinds(&shapes(&el)), [Shape::LinearPath]);
}

#[test]
fn elbow_arrows_get_their_path_and_heads() {
    // an elbow arrow's body is its rounded path (generate_elbow_arrow_shape)
    // and its heads follow as for any arrow (shape.ts:900-975); the goldens
    // of elements-elbow-arrow.json hold every shape (tests/goldens.rs)
    let mut el = arrow_with(&STRAIGHT, None, Some(Arrowhead::Arrow));
    if let ElementKind::Arrow(a) = &mut el.kind {
        a.elbowed = true;
    }
    let generator = RoughGenerator::new();
    let config = RenderConfig::default();
    let shapes = generate_linear_element_shapes(&el, &generator, &config).unwrap();
    assert_eq!(shapes.len(), 3);
    assert_eq!(
        Some(&shapes[0]),
        generate_elbow_arrow_shape(&el, &generator, &config)
            .unwrap()
            .as_ref()
    );
    assert!(shapes[1..].iter().all(|s| s.shape == Shape::Line));
    // generate_linear_shape still builds only polylines and curves
    assert_eq!(
        generate_linear_shape(&el, &generator, &config),
        Err(ShapeError::ElbowArrow)
    );
}

#[test]
fn outline_variants_fill_with_the_canvas_background() {
    let canvas = "#fff9db";
    let config = RenderConfig {
        canvas_background_color: canvas,
        ..RenderConfig::default()
    };
    let generator = RoughGenerator::new();
    for head in Arrowhead::ALL {
        let el = arrow_with(&STRAIGHT, None, Some(head));
        let all = generate_linear_element_shapes(&el, &generator, &config).unwrap();
        for d in &all[1..] {
            let fill = d.options.fill.as_deref();
            match (head, d.shape) {
                (
                    Arrowhead::CircleOutline
                    | Arrowhead::TriangleOutline
                    | Arrowhead::DiamondOutline
                    | Arrowhead::CardinalityZeroOrOne
                    | Arrowhead::CardinalityZeroOrMany,
                    Shape::Circle | Shape::Polygon,
                ) => {
                    assert_eq!(fill, Some(canvas), "{head:?}");
                    assert_eq!(d.options.fill_style, "solid");
                    assert_eq!(d.options.stroke, "#1e1e1e");
                }
                (_, Shape::Circle | Shape::Polygon) => {
                    assert_eq!(fill, Some("#1e1e1e"), "{head:?}");
                    assert_eq!(d.options.fill_style, "solid");
                }
                (_, Shape::Line) => assert_eq!(fill, None, "{head:?}"),
                other => panic!("{other:?}"),
            }
        }
    }
}

#[test]
fn dark_mode_filters_the_stroke_and_the_canvas_background() {
    let config = RenderConfig {
        theme: Theme::Dark,
        ..RenderConfig::default()
    };
    let el = arrow_with(
        &STRAIGHT,
        Some(Arrowhead::Circle),
        Some(Arrowhead::TriangleOutline),
    );
    let all = generate_linear_element_shapes(&el, &RoughGenerator::new(), &config).unwrap();
    let dark_stroke = excali_core::color::apply_dark_mode_filter("#1e1e1e", true);
    let dark_canvas = excali_core::color::apply_dark_mode_filter("#ffffff", true);
    assert_ne!(dark_stroke, "#1e1e1e");
    assert_eq!(all[1].options.fill.as_deref(), Some(dark_stroke.as_str()));
    assert_eq!(all[1].options.stroke, dark_stroke);
    assert_eq!(all[2].options.fill.as_deref(), Some(dark_canvas.as_str()));
    assert_eq!(all[2].options.stroke, dark_stroke);
}

#[test]
fn roughness_is_capped_at_one_and_circles_at_a_half() {
    for (roughness, line, circle) in [
        (0.0, 0.0, 0.0),
        (0.4, 0.4, 0.4),
        (1.0, 1.0, 0.5),
        (2.0, 1.0, 0.5),
    ] {
        let mut el = arrow_with(
            &STRAIGHT,
            Some(Arrowhead::CardinalityZeroOrOne),
            Some(Arrowhead::Diamond),
        );
        el.base.roughness = roughness;
        let all = shapes(&el);
        // the body keeps the element's roughness (adjustRoughness leaves a
        // linear element of 50 px or more alone)
        assert_eq!(all[0].options.roughness, roughness);
        let [_, circle_head, bar, diamond] = &all[..] else {
            panic!("{:?}", kinds(&all))
        };
        assert_eq!(circle_head.options.roughness, circle, "{roughness}");
        assert_eq!(bar.options.roughness, line, "{roughness}");
        assert_eq!(diamond.options.roughness, line, "{roughness}");
    }
}

#[test]
fn heads_are_solid_except_dotted_line_heads() {
    for sw in [1.0, 2.0, 4.0] {
        for style in [StrokeStyle::Solid, StrokeStyle::Dashed, StrokeStyle::Dotted] {
            let mut el = arrow_with(
                &STRAIGHT,
                Some(Arrowhead::CardinalityZeroOrMany),
                Some(Arrowhead::Triangle),
            );
            el.base.stroke_width = sw;
            el.base.stroke_style = style;
            let all = shapes(&el);
            let [_, l1, l2, circle, triangle] = &all[..] else {
                panic!("{:?}", kinds(&all))
            };
            let line_dash = match style {
                // getDashArrayDotted(sw - 1) = [1.5, 6 + sw - 1], gap less 1
                StrokeStyle::Dotted => Some(vec![1.5, 4.0 + sw]),
                _ => None,
            };
            assert_eq!(l1.options.stroke_line_dash, line_dash, "{style:?} {sw}");
            assert_eq!(l2.options.stroke_line_dash, line_dash, "{style:?} {sw}");
            assert_eq!(circle.options.stroke_line_dash, None);
            assert_eq!(triangle.options.stroke_line_dash, None);
            // the rest of the arrow's options carry over: the half pixel
            // and single stroke of non-solid strokes
            let widen = if style == StrokeStyle::Solid {
                0.0
            } else {
                0.5
            };
            for d in &all {
                assert_eq!(d.options.stroke_width, sw + widen);
                assert_eq!(d.options.disable_multi_stroke, style != StrokeStyle::Solid);
                assert_eq!(d.options.seed, 1041657908);
            }
        }
    }
}
