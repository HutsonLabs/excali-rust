//! `packages/math/src/triangle.ts`.

use crate::types::{Point, Space, Triangle};

/// `triangleIncludesPoint(triangle, p)`: whether `p` is inside or on the
/// triangle, by the signs of the three edge cross products (`triangle.ts:14`;
/// upstream's doc comment says points on the sides are excluded, but the code
/// includes them, and the port follows the code).
pub fn triangle_includes_point<S: Space>(triangle: Triangle<S>, p: Point<S>) -> bool {
    let Triangle(a, b, c) = triangle;
    let triangle_sign = |p1: Point<S>, p2: Point<S>, p3: Point<S>| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let d1 = triangle_sign(p, a, b);
    let d2 = triangle_sign(p, b, c);
    let d3 = triangle_sign(p, c, a);

    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;

    !(has_neg && has_pos)
}
