//! `packages/math/src/point.ts`.

use crate::angle::degrees_to_radians;
use crate::js;
use crate::types::{Coord, Degrees, Point, Radians, Space, Unknown, Vector};
use crate::utils::{is_finite_number, PRECISION};
use crate::vector::{vector_from_point, vector_scale};

/// `pointFrom(x, y)`.
pub const fn point_from<S: Space>(x: f64, y: f64) -> Point<S> {
    Point::new(x, y)
}

/// `pointFrom({ x, y })`, the coordinate-object overload.
pub const fn point_from_coords<S: Space>(coords: Coord<S>) -> Point<S> {
    Point::new(coords.x, coords.y)
}

/// `pointFromArray(numbers)`: a point when there are exactly two numbers.
pub fn point_from_array<S: Space>(numbers: &[f64]) -> Option<Point<S>> {
    match numbers {
        [x, y] => Some(point_from(*x, *y)),
        _ => None,
    }
}

/// `pointFromPair([x, y])`.
pub const fn point_from_pair<S: Space>(pair: [f64; 2]) -> Point<S> {
    Point::new(pair[0], pair[1])
}

/// `pointFromVector(v, offset)`: the point `v` reaches from `offset`
/// (upstream's default offset is the origin, [`Point::ORIGIN`]).
pub fn point_from_vector<S: Space>(v: Vector, offset: Point<S>) -> Point<S> {
    point_from(offset.x + v.x, offset.y + v.y)
}

/// `isPoint(value)`: a two-element array of non-NaN numbers
/// (`point.ts:97`; infinities pass).
pub fn is_point(p: &Unknown) -> bool {
    match p.as_array() {
        Some([x, y]) => [x, y]
            .iter()
            .all(|c| c.as_number().is_some_and(|n| !n.is_nan())),
        _ => false,
    }
}

/// `pointsEqual(a, b)` with the default [`PRECISION`].
pub fn points_equal<S: Space>(a: Point<S>, b: Point<S>) -> bool {
    points_equal_with(a, b, PRECISION)
}

/// `pointsEqual(a, b, tolerance)`: both coordinates closer than `tolerance`.
pub fn points_equal_with<S: Space>(a: Point<S>, b: Point<S>, tolerance: f64) -> bool {
    (a.x - b.x).abs() < tolerance && (a.y - b.y).abs() < tolerance
}

/// `pointRotateRads(point, center, angle)`: rotates `point` around `center`.
/// A zero (or NaN) angle returns the point untouched, as `!angle` does
/// upstream (`point.ts:138`).
pub fn point_rotate_rads<S: Space>(point: Point<S>, center: Point<S>, angle: Radians) -> Point<S> {
    let angle = angle.0;
    if angle == 0.0 || angle.is_nan() {
        return point;
    }
    let (x, y) = (point.x, point.y);
    let (cx, cy) = (center.x, center.y);
    point_from(
        (x - cx) * js::cos(angle) - (y - cy) * js::sin(angle) + cx,
        (x - cx) * js::sin(angle) + (y - cy) * js::cos(angle) + cy,
    )
}

/// `pointRotateDegs(point, center, angle)`.
pub fn point_rotate_degs<S: Space>(point: Point<S>, center: Point<S>, angle: Degrees) -> Point<S> {
    point_rotate_rads(point, center, degrees_to_radians(angle))
}

/// `pointTranslate(p, v)`: `p + v`, optionally into another space (upstream's
/// `From`/`To` type parameters). Pass [`Vector::ZERO`] for upstream's default.
pub fn point_translate<From: Space, To: Space>(p: Point<From>, v: Vector) -> Point<To> {
    point_from(p.x + v.x, p.y + v.y)
}

/// `pointCenter(a, b)`: the midpoint.
pub fn point_center<S: Space>(a: Point<S>, b: Point<S>) -> Point<S> {
    point_from((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

/// `pointDistance(a, b)`: the Euclidean distance (`Math.hypot`).
pub fn point_distance<S: Space>(a: Point<S>, b: Point<S>) -> f64 {
    js::hypot(b.x - a.x, b.y - a.y)
}

/// `pointDistanceSq(a, b)`: the squared Euclidean distance.
pub fn point_distance_sq<S: Space>(a: Point<S>, b: Point<S>) -> f64 {
    let x_diff = b.x - a.x;
    let y_diff = b.y - a.y;

    x_diff * x_diff + y_diff * y_diff
}

/// `pointScaleFromOrigin(p, mid, multiplier)`: scales `p` away from `mid`.
pub fn point_scale_from_origin<S: Space>(p: Point<S>, mid: Point<S>, multiplier: f64) -> Point<S> {
    point_translate(mid, vector_scale(vector_from_point(p, mid), multiplier))
}

/// `isPointWithinBounds(p, q, r)`: whether `q` lies in the axis-aligned box
/// spanned by `p` and `r` (`point.ts:245`).
pub fn is_point_within_bounds<S: Space>(p: Point<S>, q: Point<S>, r: Point<S>) -> bool {
    q.x <= js::max(p.x, r.x)
        && q.x >= js::min(p.x, r.x)
        && q.y <= js::max(p.y, r.y)
        && q.y >= js::min(p.y, r.y)
}

/// `isValidPoint(value)`: a two-element array of finite numbers.
pub fn is_valid_point(point: &Unknown) -> bool {
    match point.as_array() {
        Some([x, y]) => is_finite_number(x) && is_finite_number(y),
        _ => false,
    }
}
