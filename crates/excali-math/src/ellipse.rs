//! `packages/math/src/ellipse.ts`.

use crate::js;
use crate::point::{point_distance, point_from, point_from_vector, points_equal};
use crate::types::{Ellipse, Line, LineSegment, Point, Space};
use crate::utils::PRECISION;
use crate::vector::{vector, vector_add, vector_dot, vector_from_point, vector_scale};

/// `ellipse(center, halfWidth, halfHeight)`.
pub const fn ellipse<S: Space>(center: Point<S>, half_width: f64, half_height: f64) -> Ellipse<S> {
    Ellipse {
        center,
        half_width,
        half_height,
    }
}

/// `ellipseIncludesPoint(p, ellipse)`: inside or on the outline.
pub fn ellipse_includes_point<S: Space>(p: Point<S>, ellipse: Ellipse<S>) -> bool {
    let Ellipse {
        center,
        half_width,
        half_height,
    } = ellipse;
    let normalized_x = (p.x - center.x) / half_width;
    let normalized_y = (p.y - center.y) / half_height;

    normalized_x * normalized_x + normalized_y * normalized_y <= 1.0
}

/// `ellipseTouchesPoint(point, ellipse)` with the default [`PRECISION`].
pub fn ellipse_touches_point<S: Space>(point: Point<S>, ellipse: Ellipse<S>) -> bool {
    ellipse_touches_point_with(point, ellipse, PRECISION)
}

/// `ellipseTouchesPoint(point, ellipse, threshold)`: within `threshold` of
/// the outline.
pub fn ellipse_touches_point_with<S: Space>(
    point: Point<S>,
    ellipse: Ellipse<S>,
    threshold: f64,
) -> bool {
    ellipse_distance_from_point(point, ellipse) <= threshold
}

/// `ellipseDistanceFromPoint(p, ellipse)`: the shortest distance from `p` to
/// the outline, by three iterations of the first-quadrant closest-point
/// solver (`ellipse.ts:88`); exact for circles.
pub fn ellipse_distance_from_point<S: Space>(p: Point<S>, ellipse: Ellipse<S>) -> f64 {
    let Ellipse {
        center,
        half_width: a,
        half_height: b,
    } = ellipse;
    let origin = Point::<S>::ORIGIN;
    let translated = vector_add(
        vector_from_point(p, origin),
        vector_scale(vector_from_point(center, origin), -1.0),
    );

    // A circle's outline is equidistant along any ray from the center, so this
    // is exact, and it avoids the iteration below, which is 0/0 at the center
    if a == b {
        return (js::hypot(translated.x, translated.y) - a).abs();
    }

    let px = translated.x.abs();
    let py = translated.y.abs();

    let mut tx = 0.707_f64;
    let mut ty = 0.707_f64;

    for _ in 0..3 {
        let x = a * tx;
        let y = b * ty;

        let ex = ((a * a - b * b) * js::pow(tx, 3.0)) / a;
        let ey = ((b * b - a * a) * js::pow(ty, 3.0)) / b;

        let rx = x - ex;
        let ry = y - ey;

        let qx = px - ex;
        let qy = py - ey;

        let r = js::hypot(ry, rx);
        let q = js::hypot(qy, qx);

        if q == 0.0 {
            break;
        }

        tx = js::min(1.0, js::max(0.0, ((qx * r) / q + ex) / a));
        ty = js::min(1.0, js::max(0.0, ((qy * r) / q + ey) / b));
        let t = js::hypot(ty, tx);
        tx /= t;
        ty /= t;
    }

    // Map the first-quadrant solution back to the point's quadrant. A point on
    // an axis (coordinate exactly 0) can use either side, but not 0, which
    // would collapse the closest point onto the axis
    let min_x = a * tx * if translated.x < 0.0 { -1.0 } else { 1.0 };
    let min_y = b * ty * if translated.y < 0.0 { -1.0 } else { 1.0 };

    point_distance(
        point_from_vector(translated, origin),
        point_from(min_x, min_y),
    )
}

/// `ellipseSegmentInterceptPoints(e, s)`: up to two points where the segment
/// crosses the outline, in segment order.
pub fn ellipse_segment_intercept_points<S: Space>(
    e: Ellipse<S>,
    s: LineSegment<S>,
) -> Vec<Point<S>> {
    let rx = e.half_width;
    let ry = e.half_height;

    let dir = vector_from_point(s.1, s.0);
    let diff = vector(s.0.x - e.center.x, s.0.y - e.center.y);
    let m_dir = vector(dir.x / (rx * rx), dir.y / (ry * ry));
    let m_diff = vector(diff.x / (rx * rx), diff.y / (ry * ry));

    let a = vector_dot(dir, m_dir);
    let b = vector_dot(dir, m_diff);
    let c = vector_dot(diff, m_diff) - 1.0;
    let d = b * b - a * c;

    let at = |t: f64| point_from(s.0.x + (s.1.x - s.0.x) * t, s.0.y + (s.1.y - s.0.y) * t);
    let mut intersections = Vec::new();

    if d > 0.0 {
        let t_a = (-b - d.sqrt()) / a;
        let t_b = (-b + d.sqrt()) / a;

        if (0.0..=1.0).contains(&t_a) {
            intersections.push(at(t_a));
        }

        if (0.0..=1.0).contains(&t_b) {
            intersections.push(at(t_b));
        }
    } else if d == 0.0 {
        let t = -b / a;
        if (0.0..=1.0).contains(&t) {
            intersections.push(at(t));
        }
    }

    intersections
}

/// `ellipseLineIntersectionPoints(ellipse, line)`: where the infinite line
/// crosses the outline; one point when it touches, none when it misses.
pub fn ellipse_line_intersection_points<S: Space>(
    ellipse: Ellipse<S>,
    line: Line<S>,
) -> Vec<Point<S>> {
    let Ellipse {
        center,
        half_width,
        half_height,
    } = ellipse;
    let Line(g, h) = line;
    let (cx, cy) = (center.x, center.y);
    let x1 = g.x - cx;
    let y1 = g.y - cy;
    let x2 = h.x - cx;
    let y2 = h.y - cy;
    let a = js::pow(x2 - x1, 2.0) / js::pow(half_width, 2.0)
        + js::pow(y2 - y1, 2.0) / js::pow(half_height, 2.0);
    let b = 2.0
        * ((x1 * (x2 - x1)) / js::pow(half_width, 2.0)
            + (y1 * (y2 - y1)) / js::pow(half_height, 2.0));
    let c = js::pow(x1, 2.0) / js::pow(half_width, 2.0)
        + js::pow(y1, 2.0) / js::pow(half_height, 2.0)
        - 1.0;
    let t1 = (-b + (js::pow(b, 2.0) - 4.0 * a * c).sqrt()) / (2.0 * a);
    let t2 = (-b - (js::pow(b, 2.0) - 4.0 * a * c).sqrt()) / (2.0 * a);
    let candidates: Vec<Point<S>> = [
        point_from(x1 + t1 * (x2 - x1) + cx, y1 + t1 * (y2 - y1) + cy),
        point_from(x1 + t2 * (x2 - x1) + cx, y1 + t2 * (y2 - y1) + cy),
    ]
    .into_iter()
    .filter(|p| !p.x.is_nan() && !p.y.is_nan())
    .collect();

    if candidates.len() == 2 && points_equal(candidates[0], candidates[1]) {
        return vec![candidates[0]];
    }

    candidates
}
