//! `packages/math/src/segment.ts`.

use crate::js;
use crate::line::{line, lines_intersect_at};
use crate::point::{
    is_point, point_center, point_distance, point_from, point_from_vector, point_rotate_rads,
};
use crate::types::{LineSegment, Point, Radians, Space, Unknown};
use crate::utils::PRECISION;
use crate::vector::{vector_add, vector_cross, vector_from_point, vector_scale, vector_subtract};

/// `lineSegment(a, b)`.
pub const fn line_segment<S: Space>(a: Point<S>, b: Point<S>) -> LineSegment<S> {
    LineSegment(a, b)
}

/// `isLineSegment(value)`: a two-element array of points ([`is_point`]).
pub fn is_line_segment(segment: &Unknown) -> bool {
    match segment.as_array() {
        Some([a, b]) => is_point(a) && is_point(b),
        _ => false,
    }
}

/// `lineSegmentRotate(l, angle, origin)`: rotates both ends around `origin`,
/// or around the segment's midpoint when `origin` is `None`.
pub fn line_segment_rotate<S: Space>(
    l: LineSegment<S>,
    angle: Radians,
    origin: Option<Point<S>>,
) -> LineSegment<S> {
    line_segment(
        point_rotate_rads(l.0, origin.unwrap_or_else(|| point_center(l.0, l.1)), angle),
        point_rotate_rads(l.1, origin.unwrap_or_else(|| point_center(l.0, l.1)), angle),
    )
}

/// `segmentsIntersectAt(a, b)`: where two segments cross (`segment.ts:73`).
/// Half-open at the far ends (`t < 1`, `u < 1`), and `None` when `u` is
/// exactly 0 or the segments are parallel.
pub fn segments_intersect_at<S: Space>(a: LineSegment<S>, b: LineSegment<S>) -> Option<Point<S>> {
    let origin = Point::<S>::ORIGIN;
    let a0 = vector_from_point(a.0, origin);
    let a1 = vector_from_point(a.1, origin);
    let b0 = vector_from_point(b.0, origin);
    let b1 = vector_from_point(b.1, origin);
    let r = vector_subtract(a1, a0);
    let s = vector_subtract(b1, b0);
    let denominator = vector_cross(r, s);

    if denominator == 0.0 {
        return None;
    }

    let i = vector_subtract(
        vector_from_point(b.0, origin),
        vector_from_point(a.0, origin),
    );
    let u = vector_cross(i, r) / denominator;
    let t = vector_cross(i, s) / denominator;

    if u == 0.0 {
        return None;
    }

    let p = vector_add(a0, vector_scale(r, t));

    if (0.0..1.0).contains(&t) && (0.0..1.0).contains(&u) {
        return Some(point_from_vector(p, origin));
    }

    None
}

/// `pointOnLineSegment(point, line)` with the default [`PRECISION`].
pub fn point_on_line_segment<S: Space>(point: Point<S>, line: LineSegment<S>) -> bool {
    point_on_line_segment_with(point, line, PRECISION)
}

/// `pointOnLineSegment(point, line, threshold)`: whether the point is closer
/// than `threshold` to the segment.
pub fn point_on_line_segment_with<S: Space>(
    point: Point<S>,
    line: LineSegment<S>,
    threshold: f64,
) -> bool {
    let distance = distance_to_line_segment(point, line);

    if distance == 0.0 {
        return true;
    }

    distance < threshold
}

/// `distanceToLineSegment(point, line)`: the distance to the closest point of
/// the segment.
pub fn distance_to_line_segment<S: Space>(point: Point<S>, line: LineSegment<S>) -> f64 {
    point_distance(
        point,
        line_segment_point_at(line, line_segment_closest_parameter(point, line)),
    )
}

/// `lineSegmentPointAt(line, t)`: the point at parameter `t` (0 = start,
/// 1 = end), not clamped.
pub fn line_segment_point_at<S: Space>(line: LineSegment<S>, t: f64) -> Point<S> {
    let LineSegment(start, end) = line;
    let (x1, y1, x2, y2) = (start.x, start.y, end.x, end.y);

    point_from(x1 + t * (x2 - x1), y1 + t * (y2 - y1))
}

/// `lineSegmentIntersectionPoints(l, s)` with the default [`PRECISION`].
pub fn line_segment_intersection_points<S: Space>(
    l: LineSegment<S>,
    s: LineSegment<S>,
) -> Option<Point<S>> {
    line_segment_intersection_points_with(l, s, PRECISION)
}

/// `lineSegmentIntersectionPoints(l, s, threshold)`: the crossing of the two
/// segments' lines, if it lies within `threshold` of both segments.
pub fn line_segment_intersection_points_with<S: Space>(
    l: LineSegment<S>,
    s: LineSegment<S>,
    threshold: f64,
) -> Option<Point<S>> {
    let candidate = lines_intersect_at(line(l.0, l.1), line(s.0, s.1))?;

    if !point_on_line_segment_with(candidate, s, threshold)
        || !point_on_line_segment_with(candidate, l, threshold)
    {
        return None;
    }

    Some(candidate)
}

/// `lineSegmentsDistance(s1, s2)`: 0 when the segments intersect, else the
/// smallest end-point-to-segment distance.
pub fn line_segments_distance<S: Space>(s1: LineSegment<S>, s2: LineSegment<S>) -> f64 {
    if line_segment_intersection_points(s1, s2).is_some() {
        return 0.0;
    }

    js::min(
        js::min(
            distance_to_line_segment(s1.0, s2),
            distance_to_line_segment(s1.1, s2),
        ),
        js::min(
            distance_to_line_segment(s2.0, s1),
            distance_to_line_segment(s2.1, s1),
        ),
    )
}

/// `lineSegmentClosestParameter(point, line)`: the parameter in `[0, 1]` of
/// the segment point closest to `point` (0 for a zero-length segment).
pub fn line_segment_closest_parameter<S: Space>(point: Point<S>, line: LineSegment<S>) -> f64 {
    let (x, y) = (point.x, point.y);
    let LineSegment(start, end) = line;
    let (x1, y1, x2, y2) = (start.x, start.y, end.x, end.y);

    let a = x - x1;
    let b = y - y1;
    let c = x2 - x1;
    let d = y2 - y1;

    let dot = a * c + b * d;
    let len_sq = c * c + d * d;
    let mut param = 0.0;
    if len_sq != 0.0 {
        param = dot / len_sq;
    }

    js::max(0.0, js::min(1.0, param))
}
