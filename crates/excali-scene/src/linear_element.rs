//! The geometry `LinearElementEditor` (`packages/element/src/
//! linearElementEditor.ts`) gives the renderer and the exporters: a line's
//! or an arrow's box from its rough.js curves, where an arrow's label sits,
//! and the box of the two together.
//!
//! - [`get_element_absolute_coords`] (`:2237-2270`) and
//!   [`get_linear_element_rotated_bounds`] (`bounds.ts:934-995`) read the
//!   first shape `ShapeCache.generateElementShape(element, null)` builds
//!   (the body; `shape.ts:890-935`).
//! - [`get_bound_text_element_position`] (`:2068-2131`): a label with a
//!   `labelPosition` sits at that fraction of the arrow's path length
//!   ([`get_point_at_path_parameter`], `:2037-2066`); otherwise at the
//!   middle point of an odd number of points, or the middle of the middle
//!   segment ([`get_bound_text_element_center`], `:1942-1960`).
//! - The path is the arrow's collision shape (`generateLinearCollisionShape`,
//!   `shape.ts:621-715`) split into segments
//!   ([`get_linear_element_path_segments`], `utils.ts:129-231`); an elbow
//!   arrow's is its unrounded points.
//!
//! Upstream caches shapes, positions and path metrics per element version;
//! the port computes them afresh, which gives the same numbers.

use excali_core::element::{Element, ElementKind, FreedrawFields, LocalPoint, StrokeVariability};
use excali_freehand::CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE;
use excali_math::{
    curve, curve_length, curve_point_at_length, curve_point_at_length_with, js, line_segment,
    line_segment_point_at, point_center, point_distance, point_from, point_rotate_rads, Curve,
    Global, GlobalPoint, LineSegment, Radians,
};
use excali_rough::points_on_curve::simplify;
use excali_rough::{Drawable, Op, Options, RoughGenerator};

use crate::bounds::{
    get_bound_text_element, get_curve_path_ops, get_element_absolute_coords as element_coords,
    get_min_max_xy_from_curve_path_ops, Bounds, ElementsMap,
};
use crate::elbow_arrow::{elbow_arrow_path, ELBOW_ARROW_CORNER_RADIUS};
use crate::freedraw::get_freedraw_outline_points;
use crate::rough_options::to_int32;
use crate::shape::{generate_elbow_arrow_shape, generate_linear_shape, RenderConfig};

/// The first shape `ShapeCache.generateElementShape(element, null)` gives a
/// line or an arrow, its body; `None` when it has none (an elbow arrow with
/// a coordinate beyond the drawable range, or a path rough.js rejects).
fn body_shape(element: &Element) -> Option<Drawable> {
    let generator = RoughGenerator::new();
    let config = RenderConfig::default();
    match &element.kind {
        ElementKind::Arrow(arrow) if arrow.elbowed => {
            generate_elbow_arrow_shape(element, &generator, &config)
                .ok()
                .flatten()
        }
        _ => generate_linear_shape(element, &generator, &config).ok(),
    }
}

/// The ops of the body's curve (`getCurvePathOps(shape[0])`); none without
/// a body.
fn body_ops(element: &Element) -> Vec<Op> {
    body_shape(element)
        .map(|shape| get_curve_path_ops(&shape).to_vec())
        .unwrap_or_default()
}

fn points(element: &Element) -> &[LocalPoint] {
    element.kind.points().unwrap_or(&[])
}

fn angle(element: &Element) -> Radians {
    Radians(element.base.angle.0)
}

fn rotate(p: [f64; 2], center: [f64; 2], angle: Radians) -> [f64; 2] {
    let p: GlobalPoint = point_rotate_rads(
        point_from(p[0], p[1]),
        point_from(center[0], center[1]),
        angle,
    );
    [p.x, p.y]
}

/// `LinearElementEditor.getElementAbsoluteCoords(element, elementsMap,
/// includeBoundText)` (`linearElementEditor.ts:2237-2270`): the unrotated
/// box of the body's curves (`getMinMaxXYFromCurvePathOps`) moved to the
/// element's position, with its centre; with `include_bound_text`, grown
/// to hold the bound text ([`get_min_max_xy_with_bound_text`]).
///
/// A body with no curves gives infinite extremes, as the op reduce does;
/// where upstream throws reading a missing body (an elbow arrow beyond the
/// drawable range) the port answers the same infinite box.
pub fn get_element_absolute_coords(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    include_bound_text: bool,
) -> [f64; 6] {
    let [min_x, min_y, max_x, max_y] = get_min_max_xy_from_curve_path_ops(&body_ops(element), None);
    let b = &element.base;
    let x1 = min_x + b.x;
    let y1 = min_y + b.y;
    let x2 = max_x + b.x;
    let y2 = max_y + b.y;
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;

    if include_bound_text {
        if let Some(bound_text) = get_bound_text_element(element, elements_map) {
            return get_min_max_xy_with_bound_text(
                element,
                elements_map,
                [x1, y1, x2, y2],
                bound_text,
            );
        }
    }
    [x1, y1, x2, y2, cx, cy]
}

/// `getLinearElementRotatedBounds(element, cx, cy, elementsMap)`
/// (`bounds.ts:934-995`): the extremes of the body's curves rotated about
/// `(cx, cy)`, or of the one point of a line with fewer than two, grown by
/// the bound text ([`get_min_max_xy_with_bound_text`]).
///
/// Upstream reads the body from the shape cache, which
/// `getElementAbsoluteCoords` has just filled, so it is the shape
/// [`get_element_absolute_coords`] reads. With no points upstream throws
/// reading `points[0]`; the port reads the point `[0, 0]`.
pub fn get_linear_element_rotated_bounds(
    element: &Element,
    cx: f64,
    cy: f64,
    elements_map: &ElementsMap<'_>,
) -> Bounds {
    let b = &element.base;
    let bound_text = get_bound_text_element(element, elements_map);
    let pts = points(element);
    let angle = angle(element);

    let coords = if pts.len() < 2 {
        let [px, py] = pts.first().copied().unwrap_or([0.0, 0.0]);
        let [x, y] = rotate([b.x + px, b.y + py], [cx, cy], angle);
        [x, y, x, y]
    } else {
        let transform = |[x, y]: [f64; 2]| rotate([b.x + x, b.y + y], [cx, cy], angle);
        get_min_max_xy_from_curve_path_ops(&body_ops(element), Some(&transform))
    };
    match bound_text {
        Some(text) => {
            let [x1, y1, x2, y2, _, _] =
                get_min_max_xy_with_bound_text(element, elements_map, coords, text);
            [x1, y1, x2, y2]
        }
        None => coords,
    }
}

/// `LinearElementEditor.getMinMaxXYWithBoundText(element, elementsMap,
/// elementBounds, boundTextElement)` (`linearElementEditor.ts:2133-2235`):
/// `element_bounds` grown by the corners of the bound text's box, counter
/// rotated about the bounds' centre, choosing corners by the quadrant the
/// element's angle puts its top edge in; `[x1, y1, x2, y2, cx, cy]` with
/// the centre of `element_bounds`.
pub fn get_min_max_xy_with_bound_text(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    element_bounds: Bounds,
    bound_text: &Element,
) -> [f64; 6] {
    let [mut x1, mut y1, mut x2, mut y2] = element_bounds;
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let [bx1, by1] = get_bound_text_element_position(element, bound_text, elements_map);
    let bx2 = bx1 + bound_text.base.width;
    let by2 = by1 + bound_text.base.height;
    let center = [cx, cy];
    let angle = angle(element);
    let counter = Radians(-angle.0);

    let top_left = rotate([x1, y1], center, angle);
    let top_right = rotate([x2, y1], center, angle);

    let text_top_left = rotate([bx1, by1], center, counter);
    let text_top_right = rotate([bx2, by1], center, counter);
    let text_bottom_left = rotate([bx1, by2], center, counter);
    let text_bottom_right = rotate([bx2, by2], center, counter);

    if top_left[0] < top_right[0] && top_left[1] >= top_right[1] {
        x1 = js::min(x1, text_bottom_left[0]);
        x2 = js::max(x2, js::max(text_top_right[0], text_bottom_right[0]));
        y1 = js::min(y1, text_top_left[1]);
        y2 = js::max(y2, text_bottom_right[1]);
    } else if top_left[0] >= top_right[0] && top_left[1] > top_right[1] {
        x1 = js::min(x1, text_bottom_right[0]);
        x2 = js::max(x2, js::max(text_top_left[0], text_top_right[0]));
        y1 = js::min(y1, text_bottom_left[1]);
        y2 = js::max(y2, text_top_right[1]);
    } else if top_left[0] >= top_right[0] {
        x1 = js::min(x1, text_top_right[0]);
        x2 = js::max(x2, text_bottom_left[0]);
        y1 = js::min(y1, text_bottom_right[1]);
        y2 = js::max(y2, text_top_left[1]);
    } else if top_left[1] <= top_right[1] {
        x1 = js::min(x1, js::min(text_top_right[0], text_top_left[0]));
        x2 = js::max(x2, text_bottom_right[0]);
        y1 = js::min(y1, text_top_right[1]);
        y2 = js::max(y2, text_bottom_left[1]);
    }

    [x1, y1, x2, y2, cx, cy]
}

/// `LinearElementEditor.getBoundTextElementPosition(element,
/// boundTextElement, elementsMap)` (`linearElementEditor.ts:2068-2131`):
/// the top left corner of the label's box.
///
/// - Fewer than two points: the label's own `x` and `y`.
/// - An arrow's label with a `labelPosition`: centred on that fraction of
///   the path ([`get_point_at_path_parameter`]), when there is a path.
/// - Otherwise centred on [`get_bound_text_element_center`].
pub fn get_bound_text_element_position(
    element: &Element,
    bound_text: &Element,
    elements_map: &ElementsMap<'_>,
) -> [f64; 2] {
    let t = &bound_text.base;
    if points(element).len() < 2 {
        return [t.x, t.y];
    }
    let label_position = match &bound_text.kind {
        ElementKind::Text(text) => text.label_position.flatten(),
        _ => None,
    };
    if let (ElementKind::Arrow(_), Some(position)) = (&element.kind, label_position) {
        if let Some([x, y]) = get_point_at_path_parameter(element, position, elements_map) {
            return [x - t.width / 2.0, y - t.height / 2.0];
        }
    }
    let [x, y] = get_bound_text_element_center(element, elements_map);
    [x - t.width / 2.0, y - t.height / 2.0]
}

/// `LinearElementEditor.getBoundTextElementCenter(element, elementsMap)`
/// (`linearElementEditor.ts:1942-1960`): the middle point of an odd number
/// of points, else the middle of the middle segment.
pub fn get_bound_text_element_center(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> [f64; 2] {
    let pts = points(element);
    if pts.len() % 2 == 1 {
        let index = pts.len() / 2;
        return get_point_global_coordinates(element, pts[index], elements_map);
    }
    let index = pts.len() / 2;
    get_segment_mid_point(element, index, elements_map)
}

/// `LinearElementEditor.getPointGlobalCoordinates(element, p, elementsMap)`
/// (`linearElementEditor.ts:1338-1354`): a point of the element in scene
/// coordinates, rotated about the centre of its box.
pub fn get_point_global_coordinates(
    element: &Element,
    p: LocalPoint,
    elements_map: &ElementsMap<'_>,
) -> [f64; 2] {
    let [x1, y1, x2, y2, _, _] = element_coords(element, elements_map, false);
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let b = &element.base;
    rotate([b.x + p[0], b.y + p[1]], [cx, cy], angle(element))
}

/// `LinearElementEditor.getSegmentMidPoint(element, index, elementsMap)`
/// (`linearElementEditor.ts:976-1018`): the middle of segment `index`
/// (counting from 1). An elbow arrow's segments are its unrotated points;
/// any other line's are its collision shape's straight segments (the
/// midpoint) or curves (the point at half the length).
///
/// Upstream asserts the segment exists and throws otherwise; the port
/// answers `[NaN, NaN]` there. The label code only asks for a segment that
/// exists (`index` is at least 1 and below the point count).
pub fn get_segment_mid_point(
    element: &Element,
    index: usize,
    elements_map: &ElementsMap<'_>,
) -> [f64; 2] {
    let b = &element.base;
    if let ElementKind::Arrow(arrow) = &element.kind {
        if arrow.elbowed {
            let pts = points(element);
            let ends = index
                .checked_sub(1)
                .and_then(|i| Some((*pts.get(i)?, *pts.get(index)?)));
            let Some((a, c)) = ends else {
                return [f64::NAN, f64::NAN];
            };
            let p: excali_math::Point<excali_math::Local> =
                point_center(point_from(a[0], a[1]), point_from(c[0], c[1]));
            return [b.x + p.x, b.y + p.y];
        }
    }
    let (lines, curves) = deconstruct_linear_element(element, elements_map);
    let at = index.checked_sub(1);
    if !lines.is_empty() {
        if let Some(segment) = at.and_then(|i| lines.get(i)) {
            let p = point_center(segment.0, segment.1);
            return [p.x, p.y];
        }
    } else if let Some(&segment) = at.and_then(|i| curves.get(i)) {
        let p = curve_point_at_length(segment, 0.5);
        return [p.x, p.y];
    }
    // "Invalid segment index while calculating mid point": upstream throws
    [f64::NAN, f64::NAN]
}

/// A piece of a line's path (`LinearPathSegment`): straight or a cubic
/// curve, in scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LinearPathSegment {
    Line(LineSegment<Global>),
    Curve(Curve<Global>),
}

impl LinearPathSegment {
    /// `pathSegmentLength` (`linearElementEditor.ts:2820-2823`).
    fn length(&self) -> f64 {
        match *self {
            LinearPathSegment::Line(l) => point_distance(l.0, l.1),
            LinearPathSegment::Curve(c) => curve_length(c),
        }
    }

    /// `pathSegmentPointAtLength(segment, fraction, segmentLength)`
    /// (`linearElementEditor.ts:2849-2856`).
    fn point_at_length(&self, fraction: f64, length: f64) -> GlobalPoint {
        match *self {
            LinearPathSegment::Line(l) => line_segment_point_at(l, fraction),
            LinearPathSegment::Curve(c) => curve_point_at_length_with(c, fraction, length),
        }
    }
}

/// `getLinearElementPathSegments(element, elementsMap)` (`utils.ts:206-231`):
/// an elbow arrow's straight segments between its unrotated points, or the
/// curves of any other line's collision shape if it has any, else its
/// straight segments.
pub fn get_linear_element_path_segments(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<LinearPathSegment> {
    let b = &element.base;
    if let ElementKind::Arrow(arrow) = &element.kind {
        if arrow.elbowed {
            return arrow
                .linear
                .points
                .windows(2)
                .map(|w| {
                    LinearPathSegment::Line(line_segment(
                        point_from(b.x + w[0][0], b.y + w[0][1]),
                        point_from(b.x + w[1][0], b.y + w[1][1]),
                    ))
                })
                .collect();
        }
    }
    let (lines, curves) = deconstruct_linear_element(element, elements_map);
    if curves.is_empty() {
        lines.into_iter().map(LinearPathSegment::Line).collect()
    } else {
        curves.into_iter().map(LinearPathSegment::Curve).collect()
    }
}

/// `LinearElementEditor.getPointAtPathParameter(container, pathParameter,
/// elementsMap)` (`linearElementEditor.ts:2037-2066`): the point at that
/// fraction (clamped to `0..=1`) of the path's length; `None` for a
/// non-finite fraction or no path.
pub fn get_point_at_path_parameter(
    element: &Element,
    path_parameter: f64,
    elements_map: &ElementsMap<'_>,
) -> Option<[f64; 2]> {
    if !path_parameter.is_finite() {
        return None;
    }
    let segments = get_linear_element_path_segments(element, elements_map);
    if segments.is_empty() {
        return None;
    }
    let lengths: Vec<f64> = segments.iter().map(LinearPathSegment::length).collect();
    let mut prefix_sums = vec![0.0; lengths.len() + 1];
    for (i, length) in lengths.iter().enumerate() {
        prefix_sums[i + 1] = prefix_sums[i] + length;
    }
    let total_length = prefix_sums[lengths.len()];

    let target_length = clamp(path_parameter, 0.0, 1.0) * total_length;
    // the first segment whose end lies at or beyond the target; the last one
    // catches a target that floating point pushed past the total length
    let i = (0..segments.len())
        .find(|&i| target_length <= prefix_sums[i + 1])
        .unwrap_or(segments.len() - 1);
    let length_fraction = if lengths[i] == 0.0 {
        0.0
    } else {
        clamp((target_length - prefix_sums[i]) / lengths[i], 0.0, 1.0)
    };
    let p = segments[i].point_at_length(length_fraction, lengths[i]);
    Some([p.x, p.y])
}

/// `clamp(value, min, max)` (`math/src/utils.ts`): `Math.min(Math.max(value,
/// min), max)`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    js::min(js::max(value, min), max)
}

/// `deconstructLinearOrFreeDrawElement(element, elementsMap)`
/// (`utils.ts:129-204`): the straight segments and the curves of a line's,
/// arrow's or freedraw's collision shape
/// ([`generate_linear_collision_shape`]), in scene coordinates and
/// **rotated**. Nothing for any other element type.
///
/// Upstream caches the shape per element and version; the port computes it
/// afresh, which gives the same numbers.
pub fn deconstruct_linear_or_freedraw_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> (Vec<LineSegment<Global>>, Vec<Curve<Global>>) {
    deconstruct_linear_element(element, elements_map)
}

fn deconstruct_linear_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> (Vec<LineSegment<Global>>, Vec<Curve<Global>>) {
    let b = &element.base;
    let at = |x: f64, y: f64| -> GlobalPoint { point_from(b.x + x, b.y + y) };
    let ops = generate_linear_collision_shape(element, elements_map);
    let mut lines = Vec::new();
    let mut curves = Vec::new();
    for (idx, op) in ops.iter().enumerate() {
        let prev = idx.checked_sub(1).map(|i| {
            let data = ops[i].data();
            [data[data.len() - 2], data[data.len() - 1]]
        });
        match *op {
            Op::Move(_) => {}
            Op::LineTo([x, y]) => {
                if let Some([px, py]) = prev {
                    lines.push(line_segment(at(px, py), at(x, y)));
                }
            }
            Op::BCurveTo(d) => {
                if let Some([px, py]) = prev {
                    curves.push(curve(
                        at(px, py),
                        at(d[0], d[1]),
                        at(d[2], d[3]),
                        at(d[4], d[5]),
                    ));
                }
            }
        }
    }
    (lines, curves)
}

/// `generateLinearCollisionShape(element, elementsMap)` (`shape.ts:621-757`):
/// ops relative to the element's position and rotated about its centre
/// (`elementCenterPoint`).
///
/// - Lines and arrows: rough.js at roughness 0 with one stroke: an elbow
///   arrow's rounded path (unrotated), a sharp line's points, or a round
///   line's curve.
/// - Freedraw: the closed stroke outline ([`get_freedraw_outline_points`]),
///   a constant-width one first simplified by
///   [`CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE`]; nothing for an
///   outline of fewer than two points.
/// - Anything else: nothing.
pub fn generate_linear_collision_shape(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<Op> {
    if let ElementKind::Freedraw(fields) = &element.kind {
        return generate_freedraw_collision_shape(element, fields, elements_map);
    }
    if !matches!(element.kind, ElementKind::Line(_) | ElementKind::Arrow(_)) {
        return Vec::new();
    }
    let generator = RoughGenerator::new();
    let b = &element.base;
    let options = Options {
        seed: to_int32(b.seed),
        disable_multi_stroke: true,
        disable_multi_stroke_fill: true,
        roughness: 0.0,
        preserve_vertices: true,
        ..generator.default_options().clone()
    };
    let [x1, y1, x2, y2, _, _] = element_coords(element, elements_map, false);
    let center = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
    let angle = angle(element);

    // points array can be empty in the beginning, so it is important to add
    // initial position to it
    let own = points(element);
    let pts: &[LocalPoint] = if own.is_empty() { &[[0.0, 0.0]] } else { own };
    let local = |[x, y]: [f64; 2]| {
        let [px, py] = rotate([b.x + x, b.y + y], center, angle);
        [px - b.x, py - b.y]
    };

    if let ElementKind::Arrow(arrow) = &element.kind {
        if arrow.elbowed {
            let d = elbow_arrow_path(pts, ELBOW_ARROW_CORNER_RADIUS);
            return generator
                .path(&d, &options)
                .ok()
                .and_then(|shape| shape.sets.into_iter().next())
                .map(|set| set.ops)
                .unwrap_or_default();
        }
    }
    if b.roundness.is_none() {
        return pts
            .iter()
            .enumerate()
            .map(|(idx, &p)| {
                if idx == 0 {
                    Op::Move(local(p))
                } else {
                    Op::LineTo(local(p))
                }
            })
            .collect();
    }
    let Some(set) = generator
        .curve(pts, &options)
        .ok()
        .and_then(|shape| shape.sets.into_iter().next())
    else {
        return Vec::new();
    };
    set.ops
        .into_iter()
        .take(own.len())
        .enumerate()
        .map(|(i, op)| match op {
            // a rough.js curve is a move and then cubic curves
            _ if i == 0 => {
                let d = op.data();
                Op::Move(local([d[0], d[1]]))
            }
            Op::BCurveTo(d) => {
                let [a, c, e] = [
                    local([d[0], d[1]]),
                    local([d[2], d[3]]),
                    local([d[4], d[5]]),
                ];
                Op::BCurveTo([a[0], a[1], c[0], c[1], e[0], e[1]])
            }
            other => other,
        })
        .collect()
}

/// The freedraw case of `generateLinearCollisionShape` (`shape.ts:716-755`).
fn generate_freedraw_collision_shape(
    element: &Element,
    fields: &FreedrawFields,
    elements_map: &ElementsMap<'_>,
) -> Vec<Op> {
    let outline_points = get_freedraw_outline_points(element).unwrap_or_default();
    if outline_points.len() < 2 {
        return Vec::new();
    }
    let collision_outline = match fields.stroke_options.variability {
        // simplify only fails for a negative distance
        StrokeVariability::Constant => {
            simplify(&outline_points, CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE)
                .expect("a non-negative distance always simplifies")
        }
        StrokeVariability::Variable => outline_points,
    };
    if collision_outline.len() < 2 {
        return Vec::new();
    }
    // Close the outline polygon so its boundary never has a gap at the seam.
    let mut closed = collision_outline;
    let first = closed[0];
    let last = closed[closed.len() - 1];
    if first[0] != last[0] || first[1] != last[1] {
        closed.push(first);
    }

    let b = &element.base;
    let [x1, y1, x2, y2, _, _] = element_coords(element, elements_map, false);
    let center = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
    let angle = angle(element);
    closed
        .into_iter()
        .enumerate()
        .map(|(idx, [x, y])| {
            let [px, py] = rotate([b.x + x, b.y + y], center, angle);
            let p = [px - b.x, py - b.y];
            if idx == 0 {
                Op::Move(p)
            } else {
                Op::LineTo(p)
            }
        })
        .collect()
}
