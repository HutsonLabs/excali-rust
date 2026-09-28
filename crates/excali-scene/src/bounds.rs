//! Element geometry from `packages/element/src/bounds.ts`.

use excali_core::element::Element;

/// `getDiamondPoints(element)` (`bounds.ts:522-535`): the diamond's
/// vertices in element coordinates, `[topX, topY, rightX, rightY, bottomX,
/// bottomY, leftX, leftY]`.
///
/// The top and bottom sit at `floor(width / 2) + 1` and the left and right
/// at `floor(height / 2) + 1`: upstream adds 1 so the numbers are never 0,
/// which rough.js would otherwise complain about.
pub fn get_diamond_points(element: &Element) -> [f64; 8] {
    let (width, height) = (element.base.width, element.base.height);
    let top_x = (width / 2.0).floor() + 1.0;
    let top_y = 0.0;
    let right_x = width;
    let right_y = (height / 2.0).floor() + 1.0;
    let bottom_x = top_x;
    let bottom_y = height;
    let left_x = 0.0;
    let left_y = right_y;
    [
        top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y,
    ]
}
