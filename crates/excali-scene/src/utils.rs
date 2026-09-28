//! Element geometry helpers from `packages/element/src/utils.ts`.

use excali_core::constants::{
    DEFAULT_ADAPTIVE_RADIUS, DEFAULT_PROPORTIONAL_RADIUS, LINE_CONFIRM_THRESHOLD,
};
use excali_core::element::{Element, LocalPoint, RoundnessType};
use excali_math::js;

/// `getCornerRadius(x, element)` (`utils.ts:528-549`): the corner radius
/// for a side of length `x`.
///
/// - legacy (1) and proportional (2) roundness: `x * 0.25`
///   (`DEFAULT_PROPORTIONAL_RADIUS`);
/// - adaptive (3): `x * 0.25` up to the cutoff `R / 0.25`, then the fixed
///   radius `R = roundness.value ?? 32` (`DEFAULT_ADAPTIVE_RADIUS`);
/// - no roundness: 0.
pub fn get_corner_radius(x: f64, element: &Element) -> f64 {
    let Some(roundness) = element.base.roundness else {
        return 0.0;
    };
    match roundness.kind {
        RoundnessType::ProportionalRadius | RoundnessType::Legacy => {
            x * DEFAULT_PROPORTIONAL_RADIUS
        }
        RoundnessType::AdaptiveRadius => {
            let fixed_radius_size = roundness.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
            let cutoff_size = fixed_radius_size / DEFAULT_PROPORTIONAL_RADIUS;
            if x <= cutoff_size {
                return x * DEFAULT_PROPORTIONAL_RADIUS;
            }
            fixed_radius_size
        }
    }
}

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
