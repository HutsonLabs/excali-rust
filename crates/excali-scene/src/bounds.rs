//! Element geometry from `packages/element/src/bounds.ts`: diamond
//! vertices, and where arrowheads go (`getArrowheadSize`,
//! `getArrowheadAngle`, `getArrowheadPoints`; `site/content/research/
//! rendering.md` section 2, arrowhead geometry).

use std::f64::consts::PI;
use std::fmt;

use excali_core::element::{Arrowhead, Element, ElementKind, LocalPoint};
use excali_math::{
    degrees_to_radians, js, point_from, point_rotate_rads, Degrees, Local, Point, Radians,
};
use excali_rough::{Drawable, Op, OpSetType};

/// `getDiamondPoints(element)` (`bounds.ts:522-535`): the diamond's
/// vertices in element coordinates, `[topX, topY, rightX, rightY, bottomX,
/// bottomY, leftX, leftY]`.
///
/// The top and bottom sit at `floor(width / 2) + 1` and the left and right
/// at `floor(height / 2) + 1`: upstream adds 1 so the numbers are never 0,
/// which rough.js would otherwise complain about.
pub fn get_diamond_points(element: &Element) -> [f64; 8] {
    let (width, height) = (element.base.width, element.base.height);
    let top_x = (width / 2.0).floor() + 1.0;
    let top_y = 0.0;
    let right_x = width;
    let right_y = (height / 2.0).floor() + 1.0;
    let bottom_x = top_x;
    let bottom_y = height;
    let left_x = 0.0;
    let left_y = right_y;
    [
        top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y,
    ]
}

// ---------------------------------------------------------------------------
// Arrowheads (`bounds.ts:710-909`)

/// `CARDINALITY_MARKER_SIZE` (`bounds.ts:710`).
const CARDINALITY_MARKER_SIZE: f64 = 20.0;
/// `CROWFOOT_ARROWHEAD_SIZE` (`bounds.ts:711`).
const CROWFOOT_ARROWHEAD_SIZE: f64 = 15.0;

/// `getArrowheadSize(arrowhead)` (`bounds.ts:714-732`), in pixels: arrow
/// 25, diamonds 12, crowfeet 15, cardinality markers 20, everything else
/// 15.
pub fn get_arrowhead_size(arrowhead: Arrowhead) -> f64 {
    match arrowhead {
        Arrowhead::Arrow => 25.0,
        Arrowhead::Diamond | Arrowhead::DiamondOutline => 12.0,
        Arrowhead::CardinalityMany
        | Arrowhead::CardinalityOneOrMany
        | Arrowhead::CardinalityZeroOrMany => CROWFOOT_ARROWHEAD_SIZE,
        Arrowhead::CardinalityOne
        | Arrowhead::CardinalityExactlyOne
        | Arrowhead::CardinalityZeroOrOne => CARDINALITY_MARKER_SIZE,
        Arrowhead::Bar
        | Arrowhead::Circle
        | Arrowhead::CircleOutline
        | Arrowhead::Triangle
        | Arrowhead::TriangleOutline => 15.0,
    }
}

/// `getArrowheadAngle(arrowhead)` (`bounds.ts:735-744`): bar 90°, arrow
/// 20°, everything else 25°.
pub fn get_arrowhead_angle(arrowhead: Arrowhead) -> Degrees {
    Degrees(match arrowhead {
        Arrowhead::Bar => 90.0,
        Arrowhead::Arrow => 20.0,
        _ => 25.0,
    })
}

/// Which end of a linear element a head sits on (`"start" | "end"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowheadPosition {
    Start,
    End,
}

/// What [`get_arrowhead_points`] returns: upstream's flat number array,
/// whose length depends on the kind asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArrowheadPoints {
    /// `circle`, `circle_outline`: `[x, y, diameter]`, centred on the tip.
    Circle([f64; 3]),
    /// Every line or triangle head: `[x2, y2, x3, y3, x4, y4]`, the tip and
    /// the two wing ends. For `cardinality_many` and
    /// `cardinality_one_or_many` the first point is the base, and the wings
    /// open from it towards the tip.
    Wings([f64; 6]),
    /// `diamond`, `diamond_outline`: `[tx, ty, x3, y3, ox, oy, x4, y4]`, the
    /// tip, one wing, the vertex opposite the tip and the other wing.
    Diamond([f64; 8]),
}

impl ArrowheadPoints {
    /// The numbers in upstream's order.
    pub fn as_slice(&self) -> &[f64] {
        match self {
            ArrowheadPoints::Circle(p) => p,
            ArrowheadPoints::Wings(p) => p,
            ArrowheadPoints::Diamond(p) => p,
        }
    }
}

/// Where [`get_arrowhead_points`] fails as upstream does: the op it reads
/// is not a bezier curve (`invariant(data.length === 6, "Op data length is
/// not 6")`), or it (or the op before it) does not exist, where upstream
/// throws a `TypeError` reading `.data` or `.op` of `undefined`. A linear
/// element's rough.js body is made of `move, bcurveTo` pairs, so neither
/// happens for the shapes `_generateElementShape` builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidArrowheadOp {
    /// The op at the index read has this many numbers, not 6.
    DataLength(usize),
    /// There is no op at this index, or none before it.
    Missing(usize),
}

impl fmt::Display for InvalidArrowheadOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidArrowheadOp::DataLength(_) => f.write_str("Op data length is not 6"),
            InvalidArrowheadOp::Missing(index) => {
                write!(f, "no op at index {index}, or none before it")
            }
        }
    }
}

impl std::error::Error for InvalidArrowheadOp {}

/// `getCurvePathOps(shape)` (`packages/utils/src/shape.ts:199-211`): the
/// ops of the first `path` set, or of the first set if there is none (no
/// ops if there are no sets).
pub fn get_curve_path_ops(shape: &Drawable) -> &[Op] {
    shape
        .sets
        .iter()
        .find(|set| set.kind == OpSetType::Path)
        .or(shape.sets.first())
        .map_or(&[], |set| &set.ops)
}

/// `getArrowheadPoints(element, shape, position, arrowhead,
/// offsetMultiplier)` (`bounds.ts:746-909`): where the head of kind
/// `arrowhead` goes at `position` of the linear `element`, whose rough.js
/// shapes are `shape` (the body first).
///
/// - The direction is from the body's second op (start) or last op (end),
///   a bezier evaluated at t = 0.3 with upstream's weighting (which puts
///   t = 0.3 near the op's end point), towards the tip.
/// - The head is [`get_arrowhead_size`] long, but at most half the length
///   of the first or last segment of `points` (a quarter for diamonds).
/// - `offset_multiplier` moves the tip that many head lengths back along
///   the line (negative: forward).
/// - The wings are the base rotated by ∓[`get_arrowhead_angle`] about the
///   tip; for `cardinality_many` and `cardinality_one_or_many` the tip is
///   rotated about the base instead.
/// - Circles are `[tip, hypot(base - tip) + strokeWidth - 2]`.
///
/// `Ok(None)` when there is no body or it has no ops (a one-point arrow).
pub fn get_arrowhead_points(
    element: &Element,
    shape: &[Drawable],
    position: ArrowheadPosition,
    arrowhead: Arrowhead,
    offset_multiplier: f64,
) -> Result<Option<ArrowheadPoints>, InvalidArrowheadOp> {
    let Some(body) = shape.first() else {
        return Ok(None);
    };
    let ops = get_curve_path_ops(body);
    if ops.is_empty() {
        return Ok(None);
    }

    // The index of the bCurve operation to examine.
    let index = match position {
        ArrowheadPosition::Start => 1,
        ArrowheadPosition::End => ops.len() - 1,
    };
    let (Some(op), Some(prev_op)) = (ops.get(index), index.checked_sub(1).map(|i| &ops[i])) else {
        return Err(InvalidArrowheadOp::Missing(index));
    };
    let Op::BCurveTo(data) = op else {
        return Err(InvalidArrowheadOp::DataLength(op.data().len()));
    };

    let p3 = [data[4], data[5]];
    let p2 = [data[2], data[3]];
    let p1 = [data[0], data[1]];

    // We need to find p0 of the bezier curve. It is typically the last
    // point of the previous curve; it can also be the position of moveTo
    // operation.
    let p0 = match prev_op {
        Op::Move(p) => *p,
        Op::BCurveTo(d) => [d[4], d[5]],
        Op::LineTo(_) => [0.0, 0.0],
    };

    // B(t) = p0 * (1-t)^3 + 3p1 * t * (1-t)^2 + 3p2 * t^2 * (1-t) + p3 * t^3
    let equation = |t: f64, idx: usize| {
        js::pow(1.0 - t, 3.0) * p3[idx]
            + 3.0 * t * js::pow(1.0 - t, 2.0) * p2[idx]
            + 3.0 * js::pow(t, 2.0) * (1.0 - t) * p1[idx]
            + p0[idx] * js::pow(t, 3.0)
    };

    // We know the last point of the arrow (or the first, if start
    // arrowhead).
    let [x2, y2] = match position {
        ArrowheadPosition::Start => p0,
        ArrowheadPosition::End => p3,
    };

    // A point closer to the last point, at t = 0.3 ("chosen arbitrarily
    // and it works best for all the tested cases").
    let (x1, y1) = (equation(0.3, 0), equation(0.3, 1));

    // The normalized direction vector.
    let distance = js::hypot(x2 - x1, y2 - y1);
    let nx = (x2 - x1) / distance;
    let ny = (y2 - y1) / distance;

    let size = get_arrowhead_size(arrowhead);

    // Length for -> arrows is based on the length of the last section.
    let points = linear_points(element);
    let n = points.len();
    if n == 0 {
        // upstream reads `undefined[0]` here; a body with ops always has
        // points behind it, so this is not reached from a real shape
        return Err(InvalidArrowheadOp::Missing(0));
    }
    let [cx, cy] = match position {
        ArrowheadPosition::End => points[n - 1],
        ArrowheadPosition::Start => points[0],
    };
    let [px, py] = if n > 1 {
        match position {
            ArrowheadPosition::End => points[n - 2],
            ArrowheadPosition::Start => points[1],
        }
    } else {
        [0.0, 0.0]
    };
    let length = js::hypot(cx - px, cy - py);

    // Scale down the arrowhead until we hit a certain size so that it
    // doesn't look weird.
    let is_diamond = matches!(arrowhead, Arrowhead::Diamond | Arrowhead::DiamondOutline);
    let length_multiplier = if is_diamond { 0.25 } else { 0.5 };
    let min_size = js::min(size, length * length_multiplier);
    let tx = x2 - nx * min_size * offset_multiplier;
    let ty = y2 - ny * min_size * offset_multiplier;
    let xs = tx - nx * min_size;
    let ys = ty - ny * min_size;

    if matches!(arrowhead, Arrowhead::Circle | Arrowhead::CircleOutline) {
        let diameter = js::hypot(ys - ty, xs - tx) + element.base.stroke_width - 2.0;
        return Ok(Some(ArrowheadPoints::Circle([tx, ty, diameter])));
    }

    let angle = get_arrowhead_angle(arrowhead);
    let tip: Point<Local> = point_from(tx, ty);
    let base: Point<Local> = point_from(xs, ys);

    if matches!(
        arrowhead,
        Arrowhead::CardinalityMany | Arrowhead::CardinalityOneOrMany
    ) {
        // swap (xs, ys) with (x2, y2)
        let a = point_rotate_rads(tip, base, degrees_to_radians(Degrees(-angle.0)));
        let b = point_rotate_rads(tip, base, degrees_to_radians(angle));
        return Ok(Some(ArrowheadPoints::Wings([xs, ys, a.x, a.y, b.x, b.y])));
    }

    let a = point_rotate_rads(base, tip, Radians((-angle.0 * PI) / 180.0));
    let b = point_rotate_rads(base, tip, degrees_to_radians(angle));

    if is_diamond {
        // point opposite to the arrowhead point
        let opposite = match position {
            ArrowheadPosition::Start => {
                let [px, py] = if n > 1 { points[1] } else { [0.0, 0.0] };
                point_rotate_rads(
                    point_from(tx + min_size * 2.0, ty),
                    tip,
                    Radians((py - ty).atan2(px - tx)),
                )
            }
            ArrowheadPosition::End => {
                let [px, py] = if n > 1 { points[n - 2] } else { [0.0, 0.0] };
                point_rotate_rads(
                    point_from(tx - min_size * 2.0, ty),
                    tip,
                    Radians((ty - py).atan2(tx - px)),
                )
            }
        };
        return Ok(Some(ArrowheadPoints::Diamond([
            tx, ty, a.x, a.y, opposite.x, opposite.y, b.x, b.y,
        ])));
    }

    Ok(Some(ArrowheadPoints::Wings([tx, ty, a.x, a.y, b.x, b.y])))
}

/// A line's or an arrow's `points`; none for any other element.
fn linear_points(element: &Element) -> &[LocalPoint] {
    match &element.kind {
        ElementKind::Line(line) => &line.linear.points,
        ElementKind::Arrow(arrow) => &arrow.linear.points,
        _ => &[],
    }
}
