//! `packages/math/src/vector.ts`.

use crate::point::is_point;
use crate::types::{Point, Space, Unknown, Vector};

/// `vector(x, y)`.
pub const fn vector(x: f64, y: f64) -> Vector {
    Vector { x, y }
}

/// `vector(x, y, originX, originY)`: `[x - originX, y - originY]`.
pub fn vector_from_origin(x: f64, y: f64, origin_x: f64, origin_y: f64) -> Vector {
    vector(x - origin_x, y - origin_y)
}

/// `vectorFromPoint(p, origin)`: the vector from `origin` to `p` (upstream's
/// default origin is [`Point::ORIGIN`]).
pub fn vector_from_point<S: Space>(p: Point<S>, origin: Point<S>) -> Vector {
    vector(p.x - origin.x, p.y - origin.y)
}

/// `vectorFromPoint(p, origin, threshold, defaultValue)`: as
/// [`vector_from_point`], but a vector shorter than `threshold` becomes
/// `default_value` (upstream's default is `[0, 1]`). A threshold of `None`,
/// zero or NaN applies no threshold, as `threshold && ...` does upstream.
pub fn vector_from_point_with<S: Space>(
    p: Point<S>,
    origin: Point<S>,
    threshold: Option<f64>,
    default_value: Vector,
) -> Vector {
    let vec = vector_from_point(p, origin);

    if let Some(threshold) = threshold {
        let truthy = threshold != 0.0 && !threshold.is_nan();
        if truthy && vector_magnitude_sq(vec) < threshold * threshold {
            return default_value;
        }
    }

    vec
}

/// `vectorCross(a, b)`: the 2D cross product (signed parallelogram area).
pub fn vector_cross(a: Vector, b: Vector) -> f64 {
    a.x * b.y - b.x * a.y
}

/// `vectorDot(a, b)`.
pub fn vector_dot(a: Vector, b: Vector) -> f64 {
    a.x * b.x + a.y * b.y
}

/// `isVector(value)`: the same shape check as [`is_point`].
pub fn is_vector(v: &Unknown) -> bool {
    is_point(v)
}

/// `vectorAdd(a, b)`.
pub fn vector_add(a: Vector, b: Vector) -> Vector {
    vector(a.x + b.x, a.y + b.y)
}

/// `vectorSubtract(start, end)`: `start - end`.
pub fn vector_subtract(start: Vector, end: Vector) -> Vector {
    vector(start.x - end.x, start.y - end.y)
}

/// `vectorScale(v, scalar)`.
pub fn vector_scale(v: Vector, scalar: f64) -> Vector {
    vector(v.x * scalar, v.y * scalar)
}

/// `vectorMagnitudeSq(v)`.
pub fn vector_magnitude_sq(v: Vector) -> f64 {
    v.x * v.x + v.y * v.y
}

/// `vectorMagnitude(v)`: `Math.sqrt` of the squared magnitude (not
/// `Math.hypot`).
pub fn vector_magnitude(v: Vector) -> f64 {
    vector_magnitude_sq(v).sqrt()
}

/// `vectorNormalize(v)`: the unit vector, or `[0, 0]` for the zero vector.
pub fn vector_normalize(v: Vector) -> Vector {
    let m = vector_magnitude(v);

    if m == 0.0 {
        return vector(0.0, 0.0);
    }

    vector(v.x / m, v.y / m)
}

/// `vectorNormal(v)`: the right-hand normal `[v, -u]`.
pub fn vector_normal(v: Vector) -> Vector {
    vector(v.y, -v.x)
}
