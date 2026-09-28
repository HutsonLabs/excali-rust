//! `packages/math/src/rectangle.ts`.

use crate::point::point_from;
use crate::segment::{line_segment, line_segment_intersection_points};
use crate::types::{LineSegment, Point, Rectangle, Space};

/// `rectangle(topLeft, bottomRight)`.
pub const fn rectangle<S: Space>(top_left: Point<S>, bottom_right: Point<S>) -> Rectangle<S> {
    Rectangle(top_left, bottom_right)
}

/// `rectangleFromNumberSequence(minX, minY, maxX, maxY)`.
pub const fn rectangle_from_number_sequence<S: Space>(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Rectangle<S> {
    rectangle(point_from(min_x, min_y), point_from(max_x, max_y))
}

/// `rectangleIntersectLineSegment(r, l)`: the points where `l` crosses the
/// top, right, bottom and left sides, in that order (a corner can appear
/// twice).
pub fn rectangle_intersect_line_segment<S: Space>(
    r: Rectangle<S>,
    l: LineSegment<S>,
) -> Vec<Point<S>> {
    let Rectangle(top_left, bottom_right) = r;
    let top_right = point_from(bottom_right.x, top_left.y);
    let bottom_left = point_from(top_left.x, bottom_right.y);
    [
        line_segment(top_left, top_right),
        line_segment(top_right, bottom_right),
        line_segment(bottom_right, bottom_left),
        line_segment(bottom_left, top_left),
    ]
    .into_iter()
    .filter_map(|s| line_segment_intersection_points(l, s))
    .collect()
}

/// `rectangleIntersectRectangle(a, b)`: whether the interiors overlap
/// (touching edges do not count).
pub fn rectangle_intersect_rectangle<S: Space>(
    rectangle1: Rectangle<S>,
    rectangle2: Rectangle<S>,
) -> bool {
    let Rectangle(min1, max1) = rectangle1;
    let Rectangle(min2, max2) = rectangle2;

    min1.x < max2.x && max1.x > min2.x && min1.y < max2.y && max1.y > min2.y
}
