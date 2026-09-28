//! Element geometry helpers from `packages/element/src/utils.ts`.

use excali_core::constants::LINE_CONFIRM_THRESHOLD;
use excali_core::element::LocalPoint;
use excali_math::js;

/// `isPathALoop(points, zoomValue)` (`utils.ts:512-526`): a path of at
/// least three points whose last point is within
/// `LINE_CONFIRM_THRESHOLD / zoom` of its first. Upstream's default zoom
/// is 1.
pub fn is_path_a_loop(points: &[LocalPoint], zoom: f64) -> bool {
    if points.len() >= 3 {
        let (first, last) = (points[0], points[points.len() - 1]);
        // pointDistance (math/src/point.ts:195-200)
        let distance = js::hypot(last[0] - first[0], last[1] - first[1]);
        return distance <= LINE_CONFIRM_THRESHOLD / zoom;
    }
    false
}
