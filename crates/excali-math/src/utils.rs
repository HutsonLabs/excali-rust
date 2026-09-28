//! `packages/math/src/utils.ts`.

use crate::js;
use crate::types::Unknown;

/// The default tolerance for approximate comparisons (`utils.ts:1`).
/// Upstream writes it `10e-5`, which is `1e-4`.
pub const PRECISION: f64 = 10e-5;

/// `clamp(value, min, max)`: `Math.min(Math.max(value, min), max)`.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    js::min(js::max(value, min), max)
}

/// The `Math` rounding function [`round`] and [`round_to_step`] apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RoundingFn {
    /// `Math.round` (halves towards +infinity), upstream's default.
    #[default]
    Round,
    /// `Math.floor`.
    Floor,
    /// `Math.ceil`.
    Ceil,
}

impl RoundingFn {
    fn apply(self, x: f64) -> f64 {
        match self {
            RoundingFn::Round => js::round(x),
            RoundingFn::Floor => x.floor(),
            RoundingFn::Ceil => x.ceil(),
        }
    }
}

/// `round(value, precision, func)`: rounds to `precision` decimal places,
/// nudging by `Number.EPSILON` first (`utils.ts:7`).
pub fn round(value: f64, precision: f64, func: RoundingFn) -> f64 {
    let multiplier = 10f64.powf(precision);
    func.apply((value + f64::EPSILON) * multiplier) / multiplier
}

/// `roundToStep(value, step, func)`: rounds to a multiple of `step`
/// (`utils.ts:17`).
pub fn round_to_step(value: f64, step: f64, func: RoundingFn) -> f64 {
    let factor = 1.0 / step;
    func.apply(value * factor) / factor
}

/// `average(a, b)`.
pub fn average(a: f64, b: f64) -> f64 {
    (a + b) / 2.0
}

/// `isFiniteNumber(value)`: a number that is neither NaN nor infinite.
pub fn is_finite_number(value: &Unknown) -> bool {
    value.as_number().is_some_and(f64::is_finite)
}

/// `isCloseTo(a, b)` with the default [`PRECISION`].
pub fn is_close_to(a: f64, b: f64) -> bool {
    is_close_to_with(a, b, PRECISION)
}

/// `isCloseTo(a, b, precision)`: `|a - b| < precision`.
pub fn is_close_to_with(a: f64, b: f64, precision: f64) -> bool {
    (a - b).abs() < precision
}
