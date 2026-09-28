//! Elbow arrows drawn from their fixed points: the rounded path
//! `generateElbowArrowShape` builds (`packages/element/src/shape.ts:
//! 1018-1081`), and `validateElbowPoints`
//! (`packages/element/src/elbowArrow.ts:2293-2304`), which tells whether
//! every segment is horizontal or vertical.
//!
//! See `site/content/research/rendering.md` sections 2 (line / arrow) and 5
//! (elbow arrows). Routing the points (the A* over the non-uniform grid) is
//! not here; the shape is drawn from whatever points the arrow holds.

use excali_core::json::number_to_string;
use excali_math::js;

use crate::heading::heading_for_point_is_horizontal;

/// The corner radius `_generateElementShape` passes to
/// `generateElbowArrowShape` (`shape.ts:917`).
pub const ELBOW_ARROW_CORNER_RADIUS: f64 = 16.0;

/// An elbow arrow with a point coordinate whose absolute value is above
/// this (or is NaN) is not drawn: "Temporary fix for extremely big arrow
/// shapes" (`shape.ts:901-913`).
pub const ELBOW_ARROW_MAX_COORDINATE: f64 = 1e6;

/// `DEDUP_TRESHOLD` (`elbowArrow.ts:110`, upstream's spelling):
/// [`validate_elbow_points`]' tolerance.
pub const DEDUP_TRESHOLD: f64 = 1.0;

/// `validateElbowPoints(points)` with its default tolerance,
/// [`DEDUP_TRESHOLD`].
pub fn validate_elbow_points(points: &[[f64; 2]]) -> bool {
    validate_elbow_points_with_tolerance(points, DEDUP_TRESHOLD)
}

/// `validateElbowPoints(points, tolerance)` (`elbowArrow.ts:2293-2304`):
/// true when every segment moves less than `tolerance` along x or along y,
/// that is, runs horizontally or vertically. Fewer than two points have no
/// segments and are valid; a NaN difference is not below the tolerance.
pub fn validate_elbow_points_with_tolerance(points: &[[f64; 2]], tolerance: f64) -> bool {
    points.windows(2).all(|pair| {
        let [a, b] = [pair[0], pair[1]];
        (b[0] - a[0]).abs() < tolerance || (b[1] - a[1]).abs() < tolerance
    })
}

/// Whether `_generateElementShape` draws an elbow arrow over `points`:
/// every coordinate's absolute value is at most
/// [`ELBOW_ARROW_MAX_COORDINATE`] (`shape.ts:902-906`).
pub fn elbow_arrow_is_drawable(points: &[[f64; 2]]) -> bool {
    points.iter().all(|p| {
        p[0].abs() <= ELBOW_ARROW_MAX_COORDINATE && p[1].abs() <= ELBOW_ARROW_MAX_COORDINATE
    })
}

/// `pointDistance(a, b)`: `Math.hypot` of the difference.
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    js::hypot(b[0] - a[0], b[1] - a[1])
}

/// The point `corner` away from `point` towards `other`, along the axis the
/// segment between them runs on (`shape.ts:1033-1066`): along x when the
/// segment is horizontal, along y otherwise, and to the smaller coordinate
/// only when `other`'s is strictly smaller.
fn corner_point(point: [f64; 2], other: [f64; 2], horizontal: bool, corner: f64) -> [f64; 2] {
    let [x, y] = point;
    if horizontal {
        if other[0] < x {
            [x - corner, y]
        } else {
            [x + corner, y]
        }
    } else if other[1] < y {
        [x, y - corner]
    } else {
        [x, y + corner]
    }
}

/// `generateElbowArrowShape(points, radius)` (`shape.ts:1018-1081`): the SVG
/// path of an elbow arrow, numbers written as JavaScript writes them in a
/// template literal.
///
/// - It starts with `M` at the first point and ends with `L` to the last.
/// - Each inner point becomes a corner: `L` to the point `corner` before
///   it, then `Q` with the point itself as the control point, ending
///   `corner` after it, where
///   `corner = min(radius, |point - next| / 2, |point - prev| / 2)`.
/// - Whether the incoming and outgoing segments are horizontal comes from
///   `headingForPointIsHorizontal(point, prev)` and
///   `headingForPointIsHorizontal(next, point)`; the offset goes towards the
///   neighbour.
///
/// Callers pass at least one point (`_generateElementShape` substitutes
/// `[0, 0]` for none); with no points at all the path is empty.
pub fn elbow_arrow_path(points: &[[f64; 2]], radius: f64) -> String {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return String::new();
    };
    let n = number_to_string;
    let mut d = vec![format!("M {} {}", n(first[0]), n(first[1]))];
    for i in 1..points.len().saturating_sub(1) {
        let prev = points[i - 1];
        let next = points[i + 1];
        let point = points[i];
        let prev_is_horizontal = heading_for_point_is_horizontal(point, prev);
        let next_is_horizontal = heading_for_point_is_horizontal(next, point);
        let corner = js::min(
            js::min(radius, distance(point, next) / 2.0),
            distance(point, prev) / 2.0,
        );
        let [ax, ay] = corner_point(point, prev, prev_is_horizontal, corner);
        let [bx, by] = corner_point(point, next, next_is_horizontal, corner);
        d.push(format!("L {} {}", n(ax), n(ay)));
        d.push(format!(
            "Q {} {}, {} {}",
            n(point[0]),
            n(point[1]),
            n(bx),
            n(by)
        ));
    }
    d.push(format!("L {} {}", n(last[0]), n(last[1])));
    d.join(" ")
}
