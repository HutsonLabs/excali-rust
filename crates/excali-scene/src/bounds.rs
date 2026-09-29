//! Element geometry from `packages/element/src/bounds.ts`: diamond
//! vertices, where arrowheads go (`getArrowheadSize`, `getArrowheadAngle`,
//! `getArrowheadPoints`; `site/content/research/rendering.md` section 2,
//! arrowhead geometry), and the boxes of elements
//! (`getElementAbsoluteCoords`, `getElementBounds`, `getCommonBounds`)
//! that size an export (section 6).

use std::collections::HashMap;
use std::f64::consts::PI;
use std::fmt;

use excali_core::element::{Arrowhead, BoundElementType, Element, ElementKind, LocalPoint};
use excali_math::{
    degrees_to_radians, js, line_segment, point_from, point_rotate_rads, Curve, Degrees, Global,
    GlobalPoint, LineSegment, Local, Point, Radians,
};
use excali_rough::points_on_curve::points_on_bezier_curves;
use excali_rough::{Drawable, Op, OpSetType};

use crate::geometric_shape::{get_element_shape, GeometricShape};
use crate::shape::ShapeError;
use crate::utils::{deconstruct_diamond_element, deconstruct_rectanguloid_element, ElementOutline};

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

// ---------------------------------------------------------------------------
// Element bounds (`bounds.ts:84-297, 538-709, 997-1029`)

/// `Bounds`: `[minX, minY, maxX, maxY]`.
pub type Bounds = [f64; 4];

/// `arrayToMap(elements)` (`common/src/utils.ts`): the elements by id, as
/// the `ElementsMap` upstream's geometry looks containers and bound text up
/// in. A repeated id maps to the last element with it, as `new Map(entries)`
/// keeps the last entry.
#[derive(Clone, Debug, Default)]
pub struct ElementsMap<'a> {
    map: HashMap<&'a str, &'a Element>,
}

impl<'a> ElementsMap<'a> {
    /// The map of `elements`.
    pub fn new<I: IntoIterator<Item = &'a Element>>(elements: I) -> ElementsMap<'a> {
        let mut map = HashMap::new();
        for element in elements {
            map.insert(element.base.id.as_str(), element);
        }
        ElementsMap { map }
    }

    /// `elementsMap.get(id)`.
    pub fn get(&self, id: &str) -> Option<&'a Element> {
        self.map.get(id).copied()
    }
}

/// `getContainerElement(element, elementsMap)` (`textElement.ts:357-371`):
/// the element a text's `containerId` names, when the map has it.
pub fn get_container_element<'a>(
    element: &Element,
    elements_map: &ElementsMap<'a>,
) -> Option<&'a Element> {
    match &element.kind {
        ElementKind::Text(text) => match text.container_id.as_deref() {
            Some(id) if !id.is_empty() => elements_map.get(id),
            _ => None,
        },
        _ => None,
    }
}

/// `getBoundTextElementId(container)` (`textElement.ts:326-330`): the id of
/// the first `text` entry of `boundElements`, unless it is empty.
pub fn get_bound_text_element_id(container: &Element) -> Option<&str> {
    container
        .base
        .bound_elements
        .as_deref()?
        .iter()
        .find(|b| b.kind == BoundElementType::Text)
        .map(|b| b.id.as_str())
        .filter(|id| !id.is_empty())
}

/// `getBoundTextElement(element, elementsMap)` (`textElement.ts:332-355`):
/// the element [`get_bound_text_element_id`] names, whatever its type, when
/// the map has it.
pub fn get_bound_text_element<'a>(
    element: &Element,
    elements_map: &ElementsMap<'a>,
) -> Option<&'a Element> {
    elements_map.get(get_bound_text_element_id(element)?)
}

/// `getElementAbsoluteCoords(element, elementsMap, includeBoundText)`
/// (`bounds.ts:246-297`): `[x1, y1, x2, y2, cx, cy]`, the element's
/// unrotated box in scene coordinates and its centre.
///
/// - Freedraw: the box of its points.
/// - Lines and arrows: the box of the first rough.js shape's curves
///   ([`crate::linear_element::get_element_absolute_coords`]), grown to
///   hold the bound text when `include_bound_text` is set.
/// - Text bound to an arrow: its box at the label position
///   ([`crate::linear_element::get_bound_text_element_position`]).
/// - Anything else: `x`, `y`, `width` and `height`.
pub fn get_element_absolute_coords(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    include_bound_text: bool,
) -> [f64; 6] {
    let b = &element.base;
    match &element.kind {
        ElementKind::Freedraw(freedraw) => {
            let [min_x, min_y, max_x, max_y] = get_bounds_from_points(&freedraw.points, 0.0);
            let x1 = min_x + b.x;
            let y1 = min_y + b.y;
            let x2 = max_x + b.x;
            let y2 = max_y + b.y;
            return [x1, y1, x2, y2, (x1 + x2) / 2.0, (y1 + y2) / 2.0];
        }
        ElementKind::Line(_) | ElementKind::Arrow(_) => {
            return crate::linear_element::get_element_absolute_coords(
                element,
                elements_map,
                include_bound_text,
            );
        }
        ElementKind::Text(_) => {
            if let Some(container) = get_container_element(element, elements_map) {
                if matches!(container.kind, ElementKind::Arrow(_)) {
                    let [x, y] = crate::linear_element::get_bound_text_element_position(
                        container,
                        element,
                        elements_map,
                    );
                    return [
                        x,
                        y,
                        x + b.width,
                        y + b.height,
                        x + b.width / 2.0,
                        y + b.height / 2.0,
                    ];
                }
            }
        }
        _ => {}
    }
    [
        b.x,
        b.y,
        b.x + b.width,
        b.y + b.height,
        b.x + b.width / 2.0,
        b.y + b.height / 2.0,
    ]
}

/// `Math.min(a, b, c, d)` and `Math.max(a, b, c, d)` of four points' x and
/// y: `[minX, minY, maxX, maxY]`.
fn extremes(points: [[f64; 2]; 4]) -> Bounds {
    let [a, b, c, d] = points;
    [
        js::min(js::min(js::min(a[0], b[0]), c[0]), d[0]),
        js::min(js::min(js::min(a[1], b[1]), c[1]), d[1]),
        js::max(js::max(js::max(a[0], b[0]), c[0]), d[0]),
        js::max(js::max(js::max(a[1], b[1]), c[1]), d[1]),
    ]
}

/// `getElementBounds(element, elementsMap)` (`bounds.ts:997-1003`,
/// `ElementBounds.calculateBounds`, `:143-240`): the axis-aligned box of the
/// element as drawn, rotation included.
///
/// - Freedraw: its points rotated about the centre.
/// - Lines and arrows: the extremes of the rotated curves of the first
///   rough.js shape, grown by the bound text's counter-rotated box
///   ([`crate::linear_element::get_linear_element_rotated_bounds`]).
/// - Diamond: the rotated midpoints of the box's sides.
/// - Ellipse: the rotated ellipse's extent.
/// - Anything else: the rotated corners.
///
/// Upstream caches the result per element and version; the port computes
/// it afresh, which gives the same numbers.
pub fn get_element_bounds(element: &Element, elements_map: &ElementsMap<'_>) -> Bounds {
    let b = &element.base;
    let [x1, y1, x2, y2, cx, cy] = get_element_absolute_coords(element, elements_map, false);
    let angle = Radians(b.angle.0);
    let center: Point<Global> = point_from(cx, cy);
    let rotate = |x: f64, y: f64| -> [f64; 2] {
        let p = point_rotate_rads(point_from(x, y), center, angle);
        [p.x, p.y]
    };
    match &element.kind {
        ElementKind::Freedraw(freedraw) => {
            let local_center: Point<Local> = point_from(cx - b.x, cy - b.y);
            let rotated: Vec<LocalPoint> = freedraw
                .points
                .iter()
                .map(|&[x, y]| {
                    let p = point_rotate_rads(point_from(x, y), local_center, angle);
                    [p.x, p.y]
                })
                .collect();
            let [min_x, min_y, max_x, max_y] = get_bounds_from_points(&rotated, 0.0);
            [min_x + b.x, min_y + b.y, max_x + b.x, max_y + b.y]
        }
        ElementKind::Line(_) | ElementKind::Arrow(_) => {
            crate::linear_element::get_linear_element_rotated_bounds(element, cx, cy, elements_map)
        }
        ElementKind::Diamond => extremes([
            rotate(cx, y1),
            rotate(cx, y2),
            rotate(x1, cy),
            rotate(x2, cy),
        ]),
        ElementKind::Ellipse => {
            let w = (x2 - x1) / 2.0;
            let h = (y2 - y1) / 2.0;
            let cos = js::cos(angle.0);
            let sin = js::sin(angle.0);
            let ww = js::hypot(w * cos, h * sin);
            let hh = js::hypot(h * cos, w * sin);
            [cx - ww, cy - hh, cx + ww, cy + hh]
        }
        _ => extremes([
            rotate(x1, y1),
            rotate(x1, y2),
            rotate(x2, y2),
            rotate(x2, y1),
        ]),
    }
}

/// `getElementBounds(element, elementsMap, true)` (`bounds.ts:101-135`):
/// the element's box as if it were not rotated (`{ ...element, angle: 0 }`).
pub fn get_element_bounds_non_rotated(element: &Element, elements_map: &ElementsMap<'_>) -> Bounds {
    if element.base.angle.0 == 0.0 {
        return get_element_bounds(element, elements_map);
    }
    let mut unrotated = element.clone();
    unrotated.base.angle.0 = 0.0;
    get_element_bounds(&unrotated, elements_map)
}

/// `elementCenterPoint(element, elementsMap)` (`bounds.ts:1555-1571`): the
/// centre of a line's, arrow's or freedraw's unrotated box
/// ([`get_element_absolute_coords`]), else of [`get_element_bounds`].
pub fn element_center_point(element: &Element, elements_map: &ElementsMap<'_>) -> [f64; 2] {
    match element.kind {
        ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_) => {
            let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
            [(x1 + x2) / 2.0, (y1 + y2) / 2.0]
        }
        _ => {
            let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
            // getCenterForBounds (bounds.ts:1164-1168)
            [x1 + (x2 - x1) / 2.0, y1 + (y2 - y1) / 2.0]
        }
    }
}

/// `doBoundsIntersect(bounds1, bounds2)` (`bounds.ts:1246-1258`): the boxes
/// overlap; boxes that only touch do not.
pub fn do_bounds_intersect(a: Bounds, b: Bounds) -> bool {
    let [min_x1, min_y1, max_x1, max_y1] = a;
    let [min_x2, min_y2, max_x2, max_y2] = b;
    min_x1 < max_x2 && max_x1 > min_x2 && min_y1 < max_y2 && max_y1 > min_y2
}

/// `getCommonBounds(elements)` (`bounds.ts:1005-1029`): the box holding
/// every element's [`get_element_bounds`], looking containers and bound
/// text up among the elements themselves; `[0, 0, 0, 0]` for no elements.
pub fn get_common_bounds(elements: &[&Element]) -> Bounds {
    let elements_map = ElementsMap::new(elements.iter().copied());
    get_common_bounds_in(elements, &elements_map)
}

/// `getCommonBounds(elements, elementsMap)`: [`get_common_bounds`] with the
/// map given.
pub fn get_common_bounds_in(elements: &[&Element], elements_map: &ElementsMap<'_>) -> Bounds {
    if elements.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for element in elements {
        let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
        min_x = js::min(min_x, x1);
        min_y = js::min(min_y, y1);
        max_x = js::max(max_x, x2);
        max_y = js::max(max_y, y2);
    }
    [min_x, min_y, max_x, max_y]
}

/// `getBoundsFromPoints(points, padding)` (`bounds.ts:680-697`).
pub fn get_bounds_from_points(points: &[LocalPoint], padding: f64) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &[x, y] in points {
        min_x = js::min(min_x, x);
        min_y = js::min(min_y, y);
        max_x = js::max(max_x, x);
        max_y = js::max(max_y, y);
    }
    [
        min_x - padding,
        min_y - padding,
        max_x + padding,
        max_y + padding,
    ]
}

/// `getBezierValueForT` (`bounds.ts:538-552`).
fn get_bezier_value_for_t(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let one_minus_t = 1.0 - t;
    js::pow(one_minus_t, 3.0) * p0
        + 3.0 * js::pow(one_minus_t, 2.0) * t * p1
        + 3.0 * one_minus_t * js::pow(t, 2.0) * p2
        + js::pow(t, 3.0) * p3
}

/// `solveQuadratic` (`bounds.ts:554-597`): the curve's value at each root
/// of its derivative that falls in `[0, 1]`; `None` when there is no real
/// root.
fn solve_quadratic(p0: f64, p1: f64, p2: f64, p3: f64) -> Option<[Option<f64>; 2]> {
    let i = p1 - p0;
    let j = p2 - p1;
    let k = p3 - p2;

    let a = 3.0 * i - 6.0 * j + 3.0 * k;
    let b = 6.0 * j - 6.0 * i;
    let c = 3.0 * i;

    let sqrt_part = b * b - 4.0 * a * c;
    let has_solution = sqrt_part >= 0.0;
    if !has_solution {
        return None;
    }

    let (t1, t2) = if a == 0.0 {
        let t = -c / b;
        (t, t)
    } else {
        (
            (-b + sqrt_part.sqrt()) / (2.0 * a),
            (-b - sqrt_part.sqrt()) / (2.0 * a),
        )
    };
    let value = |t: f64| {
        (0.0..=1.0)
            .contains(&t)
            .then(|| get_bezier_value_for_t(t, p0, p1, p2, p3))
    };
    Some([value(t1), value(t2)])
}

/// `getCubicBezierCurveBound(p0, p1, p2, p3)` (`bounds.ts:599-625`).
pub fn get_cubic_bezier_curve_bound(
    p0: [f64; 2],
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
) -> Bounds {
    let sol_x = solve_quadratic(p0[0], p1[0], p2[0], p3[0]);
    let sol_y = solve_quadratic(p0[1], p1[1], p2[1], p3[1]);

    let mut min_x = js::min(p0[0], p3[0]);
    let mut max_x = js::max(p0[0], p3[0]);
    if let Some(xs) = sol_x {
        for x in xs.into_iter().flatten() {
            min_x = js::min(min_x, x);
            max_x = js::max(max_x, x);
        }
    }

    let mut min_y = js::min(p0[1], p3[1]);
    let mut max_y = js::max(p0[1], p3[1]);
    if let Some(ys) = sol_y {
        for y in ys.into_iter().flatten() {
            min_y = js::min(min_y, y);
            max_y = js::max(max_y, y);
        }
    }
    [min_x, min_y, max_x, max_y]
}

/// `getMinMaxXYFromCurvePathOps(ops, transformXY)` (`bounds.ts:627-678`):
/// the extremes of the ops' cubic curves, each point passed through
/// `transform_xy` first. A `move` only sets where the next curve starts,
/// and `lineTo` is not counted ("TODO: Implement this" upstream). No
/// curves give `[Infinity, Infinity, -Infinity, -Infinity]`.
pub fn get_min_max_xy_from_curve_path_ops(
    ops: &[Op],
    transform_xy: Option<&dyn Fn([f64; 2]) -> [f64; 2]>,
) -> Bounds {
    let transform = |p: [f64; 2]| transform_xy.map_or(p, |f| f(p));
    let mut current = [0.0, 0.0];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for op in ops {
        match *op {
            Op::Move(p) => current = p,
            Op::BCurveTo(d) => {
                let p1 = transform([d[0], d[1]]);
                let p2 = transform([d[2], d[3]]);
                let p3 = transform([d[4], d[5]]);
                let p0 = transform(current);
                current = [d[4], d[5]];
                let [x1, y1, x2, y2] = get_cubic_bezier_curve_bound(p0, p1, p2, p3);
                min_x = js::min(min_x, x1);
                min_y = js::min(min_y, y1);
                max_x = js::max(max_x, x2);
                max_y = js::max(max_y, y2);
            }
            Op::LineTo(_) => {}
        }
    }
    [min_x, min_y, max_x, max_y]
}

impl<'a> ElementsMap<'a> {
    /// `elementsMap.values()`, in no particular order: callers only ask
    /// whether any element matches.
    pub fn values(&self) -> impl Iterator<Item = &'a Element> + '_ {
        self.map.values().copied()
    }
}

/// `pointInsideBoundsInclusive(p, bounds)` (`bounds.ts:1237-1244`).
pub fn point_inside_bounds_inclusive(p: [f64; 2], bounds: Bounds) -> bool {
    p[0] >= bounds[0] && p[0] <= bounds[2] && p[1] >= bounds[1] && p[1] <= bounds[3]
}

/// `boundsContainBounds(outer, inner)` (`bounds.ts:1260-1266`): every
/// corner of `inner` inside `outer`, edges included.
pub fn bounds_contain_bounds(outer: Bounds, inner: Bounds) -> bool {
    [
        [inner[0], inner[1]],
        [inner[0], inner[3]],
        [inner[2], inner[1]],
        [inner[2], inner[3]],
    ]
    .into_iter()
    .all(|p| point_inside_bounds_inclusive(p, outer))
}

/// `_isRectanguloidElement(element)` (`bounds.ts:422-437`).
fn is_rectanguloid(element: &Element) -> bool {
    match &element.kind {
        ElementKind::Rectangle
        | ElementKind::StickyNote(_)
        | ElementKind::Image(_)
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Frame(_)
        | ElementKind::MagicFrame(_) => true,
        ElementKind::Text(text) => text.container_id.as_deref().is_none_or(str::is_empty),
        _ => false,
    }
}

/// `pointsOnBezierCurves(curve, 10)` of one cubic curve.
fn points_on_curve(c: Curve<Global>) -> Result<Vec<[f64; 2]>, ShapeError> {
    points_on_bezier_curves(
        &[
            [c.0.x, c.0.y],
            [c.1.x, c.1.y],
            [c.2.x, c.2.y],
            [c.3.x, c.3.y],
        ],
        10.0,
        None,
    )
    .map_err(ShapeError::Path)
}

/// Consecutive points as segments.
fn chain(points: &[[f64; 2]], place: impl Fn([f64; 2]) -> GlobalPoint) -> Vec<LineSegment<Global>> {
    points
        .windows(2)
        .map(|w| line_segment(place(w[0]), place(w[1])))
        .collect()
}

/// `getRotatedSides(sides, center, angle)` and `getSegmentsOnCurve(corner,
/// center, angle)` (`bounds.ts:439-484`): an unrotated outline's sides and
/// flattened corners, rotated about `center`, sides first.
fn outline_segments(
    outline: &ElementOutline,
    center: GlobalPoint,
    angle: Radians,
) -> Result<Vec<LineSegment<Global>>, ShapeError> {
    let rotate = |p: GlobalPoint| point_rotate_rads(p, center, angle);
    let mut segments: Vec<LineSegment<Global>> = outline
        .sides
        .iter()
        .map(|s| line_segment(rotate(s.0), rotate(s.1)))
        .collect();
    for &corner in &outline.corners {
        let points = points_on_curve(corner)?;
        segments.extend(chain(&points, |[x, y]| rotate(point_from(x, y))));
    }
    Ok(segments)
}

/// `getSegmentsOnEllipse(ellipse)` (`bounds.ts:486-509`): 90 chords
/// between points at equal parameter steps, rotated, closed.
fn ellipse_segments(element: &Element) -> Vec<LineSegment<Global>> {
    let b = &element.base;
    let center: GlobalPoint = point_from(b.x + b.width / 2.0, b.y + b.height / 2.0);
    let a = b.width / 2.0;
    let semi_b = b.height / 2.0;
    let n = 90;
    let delta_t = (PI * 2.0) / f64::from(n);
    let points: Vec<GlobalPoint> = (0..n)
        .map(|i| {
            let t = f64::from(i) * delta_t;
            let x = center.x + a * js::cos(t);
            let y = center.y + semi_b * js::sin(t);
            point_rotate_rads(point_from(x, y), center, Radians(b.angle.0))
        })
        .collect();
    let mut segments: Vec<LineSegment<Global>> = points
        .windows(2)
        .map(|w| line_segment(w[0], w[1]))
        .collect();
    segments.push(line_segment(points[points.len() - 1], points[0]));
    segments
}

/// `getElementLineSegments(element, elementsMap)` (`bounds.ts:299-420`):
/// the element's outline as segments in scene coordinates, which the
/// frame tests intersect.
///
/// - Lines and arrows not tested inside: each curve of the shape flattened
///   by `pointsOnBezierCurves(curve, 10)`, joined within a curve (a line
///   marked `polygon` joins across curves too).
/// - Open freedraw: its polyline.
/// - Rectangles, sticky notes, images, iframe-likes, frames and unbound
///   text: the four sides and the four flattened corners of
///   `deconstructRectanguloidElement`, rotated; diamonds likewise with
///   `deconstructDiamondElement`.
/// - Closed lines and freedraw, and text naming a container: the polygon's
///   edges; a label of a line or arrow its box's four sides.
/// - Ellipses: 90 chords.
///
/// Fails where building the shape of a line or arrow fails, or flattening
/// a curve throws.
pub fn get_element_line_segments(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Result<Vec<LineSegment<Global>>, ShapeError> {
    let shape = get_element_shape(element, elements_map)?;
    let [x1, y1, x2, y2, cx, cy] = get_element_absolute_coords(element, elements_map, false);
    let center: GlobalPoint = point_from(cx, cy);
    let angle = Radians(element.base.angle.0);
    let at = |[x, y]: [f64; 2]| -> GlobalPoint { point_from(x, y) };

    let polygon = match shape {
        GeometricShape::Polycurve(curves) => {
            let points_on_curves = curves
                .into_iter()
                .map(points_on_curve)
                .collect::<Result<Vec<_>, _>>()?;
            let per_curve = match &element.kind {
                ElementKind::Line(line) => !line.polygon,
                ElementKind::Arrow(_) => true,
                _ => false,
            };
            if per_curve {
                return Ok(points_on_curves
                    .iter()
                    .flat_map(|points| chain(points, at))
                    .collect());
            }
            let points: Vec<[f64; 2]> = points_on_curves.into_iter().flatten().collect();
            return Ok(chain(&points, at));
        }
        GeometricShape::Polyline(segments) => return Ok(segments),
        GeometricShape::Polygon(polygon) => Some(polygon),
        GeometricShape::Ellipse(_) => None,
    };
    if is_rectanguloid(element) {
        return outline_segments(&deconstruct_rectanguloid_element(element), center, angle);
    }
    if matches!(element.kind, ElementKind::Diamond) {
        return outline_segments(&deconstruct_diamond_element(element), center, angle);
    }
    let Some(polygon) = polygon else {
        return Ok(ellipse_segments(element));
    };
    if matches!(element.kind, ElementKind::Text(_)) {
        let container = get_container_element(element, elements_map);
        if container.is_some_and(|c| matches!(c.kind, ElementKind::Line(_) | ElementKind::Arrow(_)))
        {
            let p = |x: f64, y: f64| -> GlobalPoint { point_from(x, y) };
            return Ok(vec![
                line_segment(p(x1, y1), p(x2, y1)),
                line_segment(p(x2, y1), p(x2, y2)),
                line_segment(p(x2, y2), p(x1, y2)),
                line_segment(p(x1, y2), p(x1, y1)),
            ]);
        }
    }
    Ok(polygon
        .windows(2)
        .map(|w| line_segment(w[0], w[1]))
        .collect())
}
