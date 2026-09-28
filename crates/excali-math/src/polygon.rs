//! `packages/math/src/polygon.ts`.
//!
//! Upstream reads `polygon[0]` of an empty array in `polygonIsClosed` (and so
//! in `polygon`, `polygonFromPoints` and the area functions) and throws a
//! TypeError. The port does not panic there: an empty polygon is open, stays
//! empty when closed, and has zero area.

use std::f64::consts::PI;

use crate::point::points_equal_with;
use crate::types::{Point, Polygon, Space};
use crate::utils::PRECISION;

/// `polygon(...points)`: the points, closed by repeating the first one
/// unless they already are.
pub fn polygon<S: Space>(points: &[Point<S>]) -> Polygon<S> {
    polygon_close(points.to_vec())
}

/// `polygonFromPoints(points)`: as [`polygon`], taking the list.
pub fn polygon_from_points<S: Space>(points: Vec<Point<S>>) -> Polygon<S> {
    polygon_close(points)
}

/// `polygonIncludesPoint(point, polygon)`: the even-odd (crossing number)
/// test, meant for a closed polygon (`polygon.ts:18`).
pub fn polygon_includes_point<S: Space>(point: Point<S>, polygon: &[Point<S>]) -> bool {
    let x = point.x;
    let y = point.y;
    let mut inside = false;

    let n = polygon.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let xi = polygon[i].x;
        let yi = polygon[i].y;
        let xj = polygon[j].x;
        let yj = polygon[j].y;

        if ((yi > y && yj <= y) || (yi <= y && yj > y))
            && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi
        {
            inside = !inside;
        }
        j = i;
    }

    inside
}

/// `polygonIncludesPointNonZero(point, polygon)`: the non-zero winding rule
/// over the implicitly closed point list (`polygon.ts:44`).
pub fn polygon_includes_point_non_zero<S: Space>(point: Point<S>, polygon: &[Point<S>]) -> bool {
    let (x, y) = (point.x, point.y);
    let mut winding_number = 0_i64;

    for i in 0..polygon.len() {
        let j = (i + 1) % polygon.len();
        let (xi, yi) = (polygon[i].x, polygon[i].y);
        let (xj, yj) = (polygon[j].x, polygon[j].y);

        if yi <= y {
            if yj > y && (xj - xi) * (y - yi) - (x - xi) * (yj - yi) > 0.0 {
                winding_number += 1;
            }
        } else if yj <= y && (xj - xi) * (y - yi) - (x - xi) * (yj - yi) < 0.0 {
            winding_number -= 1;
        }
    }

    winding_number != 0
}

fn polygon_close<S: Space>(mut polygon: Vec<Point<S>>) -> Polygon<S> {
    if !polygon.is_empty() && !polygon_is_closed(&polygon) {
        polygon.push(polygon[0]);
    }
    Polygon(polygon)
}

/// `polygonIsClosed(polygon)` with the default [`PRECISION`].
pub fn polygon_is_closed<S: Space>(polygon: &[Point<S>]) -> bool {
    polygon_is_closed_with(polygon, PRECISION)
}

/// `polygonIsClosed(polygon, tolerance)`: the last point equals the first
/// within `tolerance`.
pub fn polygon_is_closed_with<S: Space>(polygon: &[Point<S>], tolerance: f64) -> bool {
    match (polygon.first(), polygon.last()) {
        (Some(first), Some(last)) => points_equal_with(*first, *last, tolerance),
        _ => false,
    }
}

/// `polygonSignedArea(polygon)` with the default [`PRECISION`].
pub fn polygon_signed_area<S: Space>(polygon: &[Point<S>]) -> f64 {
    polygon_signed_area_with(polygon, PRECISION)
}

/// `polygonSignedArea(polygon, tolerance)`: the shoelace area, positive when
/// the vertices wind counter-clockwise in a y-down system. A closing vertex
/// (equal to the first within `tolerance`) is ignored.
pub fn polygon_signed_area_with<S: Space>(polygon: &[Point<S>], tolerance: f64) -> f64 {
    let pts = if polygon_is_closed_with(polygon, tolerance) {
        &polygon[..polygon.len() - 1]
    } else {
        polygon
    };
    let mut sum = 0.0;
    let n = pts.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        sum += pts[j].x * pts[i].y - pts[i].x * pts[j].y;
        j = i;
    }
    sum / 2.0
}

/// `polygonArea(polygon)` with the default [`PRECISION`].
pub fn polygon_area<S: Space>(polygon: &[Point<S>]) -> f64 {
    polygon_area_with(polygon, PRECISION)
}

/// `polygonArea(polygon, tolerance)`: the absolute shoelace area.
pub fn polygon_area_with<S: Space>(polygon: &[Point<S>], tolerance: f64) -> f64 {
    polygon_signed_area_with(polygon, tolerance).abs()
}

/// `convexHull(points)`: Andrew's monotone chain. The hull vertices in
/// counter-clockwise order (y-down), without a repeated closing vertex;
/// fewer than three points, or a degenerate hull, come back as given.
pub fn convex_hull<S: Space>(points: &[Point<S>]) -> Vec<Point<S>> {
    if points.len() < 3 {
        return points.to_vec();
    }

    // Array.prototype.sort as V8 runs it: stable, no panic when NaN or
    // infinite coordinates make the comparator inconsistent, and then the
    // same permutation as V8 (goldens/js-sort.json).
    let mut sorted = points.to_vec();
    crate::js::sort(
        &mut sorted,
        |a, b| {
            if a.x == b.x {
                a.y - b.y
            } else {
                a.x - b.x
            }
        },
    );

    // Cross product of OA x OB. Negative means the turn O->A->B is clockwise.
    let cross = |o: Point<S>, a: Point<S>, b: Point<S>| {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    };

    let half = |pts: &mut dyn Iterator<Item = Point<S>>| {
        let mut chain: Vec<Point<S>> = Vec::new();
        for p in pts {
            while chain.len() >= 2
                && cross(chain[chain.len() - 2], chain[chain.len() - 1], p) <= 0.0
            {
                chain.pop();
            }
            chain.push(p);
        }
        chain.pop(); // shared with the other half's first vertex
        chain
    };

    let mut hull = half(&mut sorted.iter().copied());
    hull.extend(half(&mut sorted.iter().rev().copied()));

    if hull.len() >= 3 {
        hull
    } else {
        points.to_vec()
    }
}

/// `simplifyConvexPolygon(polygon, angleThreshold)`: drops the vertices of a
/// convex polygon that only add a shallow turn, merging each run of
/// near-collinear vertices into one. Starts at the sharpest vertex so the
/// result does not depend on where the polygon starts (`polygon.ts:152`).
pub fn simplify_convex_polygon<S: Space>(
    polygon: &[Point<S>],
    angle_threshold: f64,
) -> Vec<Point<S>> {
    if polygon.len() < 3 {
        return polygon.to_vec();
    }

    let n = polygon.len();
    let turn_at = |i: usize| {
        let prev = polygon[(i + n - 1) % n];
        let curr = polygon[i];
        let next = polygon[(i + 1) % n];
        let in_angle = (curr.y - prev.y).atan2(curr.x - prev.x);
        let out_angle = (next.y - curr.y).atan2(next.x - curr.x);
        normalize_turn(out_angle - in_angle).abs()
    };

    // Walk the hull accumulating turn, emitting a vertex whenever enough of it
    // has built up. Starting at the sharpest vertex keeps the result stable: a
    // real corner always ends a run rather than being split across the seam.
    let mut start = 0;
    for i in 1..n {
        if turn_at(i) > turn_at(start) {
            start = i;
        }
    }

    let mut simplified = Vec::new();
    let mut accumulated = 0.0;
    for k in 0..n {
        let i = (start + k) % n;
        accumulated += turn_at(i);
        if accumulated >= angle_threshold {
            simplified.push(polygon[i]);
            accumulated = 0.0;
        }
    }

    if simplified.len() >= 3 {
        simplified
    } else {
        polygon.to_vec()
    }
}

fn normalize_turn(angle: f64) -> f64 {
    let mut a = angle;
    while a > PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}
