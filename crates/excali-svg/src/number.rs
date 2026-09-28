//! Numbers in the markup: upstream writes attribute values with template
//! literals (`${width}`), which print a number as JavaScript's
//! `Number::toString` does ([`js`]), and rough.js writes path data with
//! `fixedDecimalPlaceDigits` decimals, `+x.toFixed(digits)` ([`fixed`]);
//! the SVG export asks for [`MAX_DECIMALS_FOR_SVG_EXPORT`].

use excali_scene::display::{number_to_string, to_fixed};

/// `MAX_DECIMALS_FOR_SVG_EXPORT` (`packages/common/src/constants.ts:399`):
/// the decimals of every rough.js path in an exported SVG
/// (`renderer/staticSvgScene.ts:60-74`).
pub const MAX_DECIMALS_FOR_SVG_EXPORT: usize = 2;

/// `String(x)`: shortest round-trip digits, exponent from `1e21` and below
/// `1e-6`, `-0` as `0`, `NaN`, `Infinity`.
pub fn js(x: f64) -> String {
    number_to_string(x)
}

/// `+x.toFixed(digits)`: `x` rounded to `digits` decimals, halves of the
/// exact binary value away from zero, as a number.
pub fn fixed(x: f64, digits: usize) -> f64 {
    to_fixed(x, digits)
}
