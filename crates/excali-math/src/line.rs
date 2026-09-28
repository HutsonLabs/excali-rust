//! `packages/math/src/line.ts`.

use crate::point::point_from;
use crate::types::{Line, Point, Space};

/// `line(a, b)`: the infinite line through two points.
pub const fn line<S: Space>(a: Point<S>, b: Point<S>) -> Line<S> {
    Line(a, b)
}

/// `linesIntersectAt(a, b)`: where two infinite lines cross, or `None` when
/// they are parallel (`line.ts:23`).
pub fn lines_intersect_at<S: Space>(a: Line<S>, b: Line<S>) -> Option<Point<S>> {
    let a1 = a.1.y - a.0.y;
    let b1 = a.0.x - a.1.x;
    let a2 = b.1.y - b.0.y;
    let b2 = b.0.x - b.1.x;
    let d = a1 * b2 - a2 * b1;
    if d != 0.0 {
        let c1 = a1 * a.0.x + b1 * a.0.y;
        let c2 = a2 * b.0.x + b2 * b.0.y;
        return Some(point_from((c1 * b2 - c2 * b1) / d, (a1 * c2 - a2 * c1) / d));
    }

    None
}
