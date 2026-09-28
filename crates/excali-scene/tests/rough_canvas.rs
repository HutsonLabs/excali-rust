//! rough.js 4.6.4 `RoughCanvas.draw` (`bin/canvas.js`) as display items.
//!
//! Upstream draws every roughjs shape with `rc.draw(drawable)` after setting
//! `lineJoin = "round"` and `lineCap = "round"` on the context
//! (`packages/element/src/renderElement.ts:473-495`). `draw` walks the op sets:
//!
//! - `path`: `strokeStyle = o.stroke === 'none' ? 'transparent' : o.stroke`,
//!   `lineWidth = o.strokeWidth`, `setLineDash(o.strokeLineDash)` when set,
//!   `lineDashOffset = o.strokeLineDashOffset` when truthy, then `stroke()`;
//! - `fillPath`: `fillStyle = o.fill || ''`, then `fill(rule)` with
//!   `evenodd` for `curve`, `polygon` and `path` shapes and `nonzero`
//!   otherwise;
//! - `fillSketch`: `setLineDash(o.fillLineDash)` and `lineDashOffset =
//!   o.fillLineDashOffset` when set, `strokeStyle = o.fill || ''`,
//!   `lineWidth = o.fillWeight` (`o.strokeWidth / 2` when negative), then
//!   `stroke()`;
//!
//! each inside `save()`/`restore()`, with every op's data passed through
//! `+d.toFixed(fixedDecimalPlaceDigits)` when that option is a number >= 0.

use excali_rough::{Drawable, Op, OpSet, Options, RoughGenerator, Shape};
use excali_scene::display::{
    Color, DisplayItem, FillRule, LineCap, LineJoin, Path, PathCommand, Stroke,
};
use excali_scene::rough_canvas::{draw, ToFixedRangeError};

fn drawable(shape: Shape, sets: Vec<OpSet>, options: Options) -> Drawable {
    Drawable {
        shape,
        options,
        sets,
    }
}

fn ops() -> Vec<Op> {
    vec![
        Op::Move([0.0, 0.0]),
        Op::LineTo([10.0, 0.0]),
        Op::BCurveTo([12.0, 2.0, 12.0, 8.0, 10.0, 10.0]),
    ]
}

fn path_of(ops: &[Op]) -> Path {
    let mut p = Path::new();
    for op in ops {
        match *op {
            Op::Move([x, y]) => p.move_to(x, y),
            Op::LineTo([x, y]) => p.line_to(x, y),
            Op::BCurveTo([a, b, c, d, e, f]) => p.cubic_to(a, b, c, d, e, f),
        };
    }
    p
}

fn stroke(items: &[DisplayItem], i: usize) -> (&Path, &Stroke) {
    match &items[i] {
        DisplayItem::Stroke { path, stroke } => (path, stroke),
        other => panic!("item {i} is not a stroke: {other:?}"),
    }
}

fn fill(items: &[DisplayItem], i: usize) -> (&Path, &Color, FillRule) {
    match &items[i] {
        DisplayItem::Fill { path, color, rule } => (path, color, *rule),
        other => panic!("item {i} is not a fill: {other:?}"),
    }
}

#[test]
fn path_sets_are_strokes_in_the_stroke_colour() {
    let o = Options {
        stroke: "#1e1e1e".into(),
        stroke_width: 2.0,
        ..Options::default()
    };
    let d = drawable(Shape::Rectangle, vec![OpSet::path(ops())], o);
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    assert_eq!(items.len(), 1);
    let (path, s) = stroke(&items, 0);
    assert_eq!(*path, path_of(&ops()));
    assert_eq!(s.color.as_str(), "#1e1e1e");
    assert_eq!(s.width, 2.0);
    assert_eq!(s.cap, LineCap::Round);
    assert_eq!(s.join, LineJoin::Round);
    assert_eq!(s.miter_limit, 10.0);
    assert_eq!(s.dash, None);
}

#[test]
fn stroke_none_is_transparent() {
    let o = Options {
        stroke: "none".into(),
        ..Options::default()
    };
    let d = drawable(Shape::Line, vec![OpSet::path(ops())], o);
    let items = draw(&d, LineCap::Butt, LineJoin::Miter).unwrap();
    assert_eq!(stroke(&items, 0).1.color.as_str(), "transparent");
    assert_eq!(stroke(&items, 0).1.cap, LineCap::Butt);
}

#[test]
fn stroke_dash_and_offset() {
    // shape.ts:168-170: dashed [8, 8 + sw], dotted [1.5, 6 + sw].
    let o = Options {
        stroke_line_dash: Some(vec![8.0, 10.0]),
        stroke_line_dash_offset: Some(3.0),
        ..Options::default()
    };
    let d = drawable(Shape::Line, vec![OpSet::path(ops())], o);
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    let dash = stroke(&items, 0).1.dash.clone().unwrap();
    assert_eq!(dash.segments(), &[8.0, 10.0]);
    assert_eq!(dash.offset(), 3.0);
}

#[test]
fn dash_offset_only_when_truthy_and_empty_dash_is_solid() {
    // `if (o.strokeLineDashOffset)`: 0 and NaN are skipped.
    let o = Options {
        stroke_line_dash: Some(vec![1.5, 8.0]),
        stroke_line_dash_offset: Some(f64::NAN),
        ..Options::default()
    };
    let items = draw(
        &drawable(Shape::Line, vec![OpSet::path(ops())], o),
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap();
    assert_eq!(stroke(&items, 0).1.dash.as_ref().unwrap().offset(), 0.0);
    // `setLineDash([])` is truthy in JS and gives a solid line.
    let o = Options {
        stroke_line_dash: Some(vec![]),
        ..Options::default()
    };
    let items = draw(
        &drawable(Shape::Line, vec![OpSet::path(ops())], o),
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap();
    assert_eq!(stroke(&items, 0).1.dash, None);
}

#[test]
fn fill_path_rule_depends_on_the_shape() {
    for (shape, rule) in [
        (Shape::Curve, FillRule::EvenOdd),
        (Shape::Polygon, FillRule::EvenOdd),
        (Shape::Path, FillRule::EvenOdd),
        (Shape::Rectangle, FillRule::NonZero),
        (Shape::Ellipse, FillRule::NonZero),
        (Shape::Circle, FillRule::NonZero),
        (Shape::Arc, FillRule::NonZero),
        (Shape::Line, FillRule::NonZero),
        (Shape::LinearPath, FillRule::NonZero),
    ] {
        let o = Options {
            fill: Some("#a5d8ff".into()),
            ..Options::default()
        };
        let items = draw(
            &drawable(shape, vec![OpSet::fill_path(ops())], o),
            LineCap::Round,
            LineJoin::Round,
        )
        .unwrap();
        let (path, color, r) = fill(&items, 0);
        assert_eq!(r, rule, "{shape:?}");
        assert_eq!(color.as_str(), "#a5d8ff");
        assert_eq!(*path, path_of(&ops()));
    }
}

#[test]
fn fill_without_a_colour_is_the_empty_string() {
    // `o.fill || ''`: the canvas ignores '' and the display list paints nothing.
    let items = draw(
        &drawable(
            Shape::Rectangle,
            vec![OpSet::fill_path(ops())],
            Options::default(),
        ),
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap();
    assert_eq!(fill(&items, 0).1.as_str(), "");
    assert_eq!(fill(&items, 0).1.rgba(), None);
}

#[test]
fn fill_sketch_strokes_in_the_fill_colour() {
    let o = Options {
        fill: Some("#ffc9c9".into()),
        fill_weight: 1.0,
        stroke_width: 2.0,
        stroke_line_dash: Some(vec![8.0, 10.0]),
        fill_line_dash: Some(vec![3.0]),
        fill_line_dash_offset: Some(1.0),
        ..Options::default()
    };
    let items = draw(
        &drawable(Shape::Rectangle, vec![OpSet::fill_sketch(ops())], o),
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap();
    let (path, s) = stroke(&items, 0);
    assert_eq!(*path, path_of(&ops()));
    assert_eq!(s.color.as_str(), "#ffc9c9");
    assert_eq!(s.width, 1.0);
    assert_eq!(s.cap, LineCap::Round);
    // The fill's own dash, not the stroke's.
    let dash = s.dash.clone().unwrap();
    assert_eq!(dash.segments(), &[3.0, 3.0]);
    assert_eq!(dash.offset(), 1.0);
}

#[test]
fn fill_sketch_weight_defaults_to_half_the_stroke() {
    // Default fillWeight -1: `fweight = o.strokeWidth / 2`.
    let o = Options {
        fill: Some("#000".into()),
        stroke_width: 3.0,
        ..Options::default()
    };
    let items = draw(
        &drawable(Shape::Rectangle, vec![OpSet::fill_sketch(ops())], o),
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap();
    assert_eq!(stroke(&items, 0).1.width, 1.5);
    assert_eq!(stroke(&items, 0).1.dash, None);
}

#[test]
fn sets_are_drawn_in_order() {
    // RoughGenerator puts the fill before the outline; draw keeps that order.
    let gen = RoughGenerator::new();
    let o = Options {
        seed: 7,
        fill: Some("#b2f2bb".into()),
        fill_style: "hachure".into(),
        stroke: "#2f9e44".into(),
        stroke_width: 2.0,
        ..Options::default()
    };
    let d = gen.rectangle(0.0, 0.0, 100.0, 50.0, &o);
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    assert_eq!(items.len(), d.sets.len());
    for (item, set) in items.iter().zip(&d.sets) {
        let DisplayItem::Stroke { path, stroke } = item else {
            panic!("hachure and outline are strokes");
        };
        assert_eq!(*path, path_of(&set.ops));
        let expected = match set.kind {
            excali_rough::OpSetType::FillSketch => "#b2f2bb",
            excali_rough::OpSetType::Path => "#2f9e44",
            excali_rough::OpSetType::FillPath => unreachable!(),
        };
        assert_eq!(stroke.color.as_str(), expected);
    }
    let solid = Options {
        fill_style: "solid".into(),
        ..o
    };
    let d = gen.polygon(&[[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]], &solid);
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    assert_eq!(fill(&items, 0).2, FillRule::EvenOdd);
    assert!(matches!(items[1], DisplayItem::Stroke { .. }));
}

#[test]
fn fixed_decimal_place_digits_rounds_like_to_fixed() {
    // `+d.toFixed(n)`: the exact binary value, halves away from zero.
    let o = Options {
        fixed_decimal_place_digits: Some(1.0),
        ..Options::default()
    };
    let d = drawable(
        Shape::Line,
        vec![OpSet::path(vec![
            Op::Move([0.25, -0.25]),
            Op::LineTo([1.05, 2.349]),
            Op::BCurveTo([0.15, 0.35, -1.55, 7.0, 1e21, 0.04]),
        ])],
        o,
    );
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    // 0.25 and -0.25 are exact halves: 0.3 and -0.3. 1.05 is below the
    // half in binary (1.0500000000000000444 rounds up, 0.15 is
    // 0.1499999999999999944 and rounds down, 0.35 is 0.34999999999999997780
    // and rounds down). 1e21 prints as itself.
    assert_eq!(
        stroke(&items, 0).0.commands,
        vec![
            PathCommand::MoveTo(0.3, -0.3),
            PathCommand::LineTo(1.1, 2.3),
            PathCommand::CubicTo(0.1, 0.3, -1.6, 7.0, 1e21, 0.0),
        ]
    );
    // Truncated to an integer count, and fractional counts below 1 are 0.
    let o = Options {
        fixed_decimal_place_digits: Some(0.9),
        ..Options::default()
    };
    let d = drawable(
        Shape::Line,
        vec![OpSet::path(vec![Op::Move([2.5, 1.4])])],
        o,
    );
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    assert_eq!(
        stroke(&items, 0).0.commands,
        vec![PathCommand::MoveTo(3.0, 1.0)]
    );
}

#[test]
fn fixed_decimal_place_digits_that_are_not_a_count() {
    // Negative or NaN: `typeof n === 'number' && n >= 0` is false, data as is.
    for n in [-1.0, f64::NAN] {
        let o = Options {
            fixed_decimal_place_digits: Some(n),
            ..Options::default()
        };
        let d = drawable(
            Shape::Line,
            vec![OpSet::path(vec![Op::Move([0.123, 0.456])])],
            o,
        );
        let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
        assert_eq!(
            stroke(&items, 0).0.commands,
            vec![PathCommand::MoveTo(0.123, 0.456)]
        );
    }
    // Above 100 (or infinite), toFixed throws a RangeError out of draw.
    for n in [101.0, f64::INFINITY] {
        let o = Options {
            fixed_decimal_place_digits: Some(n),
            ..Options::default()
        };
        let d = drawable(
            Shape::Line,
            vec![OpSet::path(vec![Op::Move([0.0, 0.0])])],
            o,
        );
        assert_eq!(
            draw(&d, LineCap::Round, LineJoin::Round),
            Err(ToFixedRangeError)
        );
    }
    // 100 itself is allowed.
    let o = Options {
        fixed_decimal_place_digits: Some(100.0),
        ..Options::default()
    };
    let d = drawable(
        Shape::Line,
        vec![OpSet::path(vec![Op::Move([0.1, 0.0])])],
        o,
    );
    let items = draw(&d, LineCap::Round, LineJoin::Round).unwrap();
    assert_eq!(
        stroke(&items, 0).0.commands,
        vec![PathCommand::MoveTo(0.1, 0.0)]
    );
}
