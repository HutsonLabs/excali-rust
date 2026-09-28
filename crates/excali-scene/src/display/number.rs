//! Numbers as JavaScript writes them: what upstream's template literals and
//! `String(x)` print ([`number_to_string`]) and `Number.prototype.toFixed`
//! read back as a number ([`to_fixed`]), which rough.js applies to every op
//! when a drawable has `fixedDecimalPlaceDigits` (the SVG export's two
//! decimals, `MAX_DECIMALS_FOR_SVG_EXPORT`).

pub use excali_core::json::number_to_string;

/// `+x.toFixed(digits)` for `digits` in `0..=100`.
///
/// `toFixed` rounds the exact binary value to the nearest multiple of
/// `10^-digits`, taking the larger magnitude on a tie (Rust's `{:.N}` takes
/// the even one), keeps the sign of a negative number that rounds to zero,
/// and at `10^21` and above prints the number itself.
pub fn to_fixed(x: f64, digits: usize) -> f64 {
    if !x.is_finite() || x.abs() >= 1e21 {
        return x;
    }
    // Every finite double has a finite decimal expansion of at most 1074
    // fractional digits, and `{:.N}` prints it exactly.
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac) = exact.split_once('.').expect("fixed notation");
    let mut kept: Vec<u8> = format!("{int_part}{}", &frac[..digits]).into_bytes();
    if frac.as_bytes()[digits] >= b'5' {
        // Add one in the last kept place, carrying through nines.
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let kept = String::from_utf8(kept).expect("ascii digits");
    let (i, f) = kept.split_at(kept.len() - digits);
    let sign = if x < 0.0 { "-" } else { "" };
    let text = if digits == 0 {
        format!("{sign}{i}")
    } else {
        format!("{sign}{i}.{f}")
    };
    text.parse().expect("decimal")
}
