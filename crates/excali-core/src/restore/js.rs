//! The bits of JS value semantics restore relies on, for JSON values:
//! truthiness (`||`), nullishness (`??`), `ToNumber`, `ToString` of arrays,
//! `String.prototype.trim` and number results written as `JSON.stringify`
//! writes them.
//!
//! A JSON value has no `undefined`: an absent key is `None` here. Strings
//! may be in the crate's sentinel form ([`crate::json`]); the functions
//! below give the same answer for a string and its sentinel form, since
//! sentinels are neither empty nor whitespace nor digits.

use serde_json::{Number, Value};

use crate::json;

/// `!!value` (ECMA-262 `ToBoolean`).
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// `value == null`: `null` or `undefined`.
pub(crate) fn nullish(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null))
}

/// `value || fallback`.
pub(crate) fn or(value: Option<&Value>, fallback: impl FnOnce() -> Option<Value>) -> Option<Value> {
    if truthy(value) {
        value.cloned()
    } else {
        fallback()
    }
}

/// `value ?? fallback`.
pub(crate) fn nullish_or(
    value: Option<&Value>,
    fallback: impl FnOnce() -> Option<Value>,
) -> Option<Value> {
    if nullish(value) {
        fallback()
    } else {
        value.cloned()
    }
}

/// A number result as a JSON value: integral values as integers (what
/// parsing the written number gives back), `-0` as `0`, and NaN and the
/// infinities as `null`, which is how `JSON.stringify` writes them.
pub(crate) fn number(x: f64) -> Value {
    const MAX_SAFE: f64 = 9_007_199_254_740_992.0;
    if x.fract() == 0.0 && x.abs() <= MAX_SAFE {
        // Exact: an integral f64 within 2^53 converts without rounding.
        return Value::Number(Number::from(x as i64));
    }
    Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// `Number(value)` for a JSON value (ECMA-262 `ToNumber` after
/// `ToPrimitive` with hint number): `null` 0, booleans 0 or 1, strings
/// parsed as `StringToNumber`, arrays through their `join(",")`, plain
/// objects NaN (`"[object Object]"`).
pub(crate) fn to_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => string_to_number(s),
        Value::Array(_) => string_to_number(&to_string(value)),
        Value::Object(_) => f64::NAN,
    }
}

/// `String(value)` for a JSON value; an array is `join(",")`, where `null`
/// items are empty.
pub(crate) fn to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => json::js_number(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => to_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// ECMA-262 `WhiteSpace` and `LineTerminator`, what `String.prototype.trim`
/// and `StringToNumber` strip. Unlike Rust's `char::is_whitespace` this has
/// U+FEFF and not U+0085.
pub(crate) fn is_whitespace(unit: u32) -> bool {
    matches!(
        unit,
        0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20 | 0xA0 | 0x1680 | 0x2000
            ..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// `String.prototype.trim` on UTF-16 code units.
pub(crate) fn trim_units(units: &[u16]) -> &[u16] {
    let start = units
        .iter()
        .position(|&u| !is_whitespace(u32::from(u)))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|&u| !is_whitespace(u32::from(u)))
        .map_or(start, |i| i + 1);
    &units[start..end]
}

/// ECMA-262 `StringToNumber`: surrounding whitespace ignored, empty is 0,
/// `0x`/`0o`/`0b` integers, `[+-]Infinity`, decimal literals with optional
/// sign, fraction and exponent; anything else NaN.
pub(crate) fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(|c: char| is_whitespace(u32::from(c)));
    if s.is_empty() {
        return 0.0;
    }
    let bytes = s.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'0' {
        let radix = match bytes[1] {
            b'x' | b'X' => Some(16),
            b'o' | b'O' => Some(8),
            b'b' | b'B' => Some(2),
            _ => None,
        };
        if let Some(radix) = radix {
            return non_decimal(&s[2..], radix);
        }
    }
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if unsigned == "Infinity" {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if !is_decimal_literal(unsigned) {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `StrUnsignedDecimalLiteral` without `Infinity`: digits with an optional
/// fraction, or a fraction alone, then an optional exponent.
fn is_decimal_literal(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let int = digits(&mut i);
    let mut frac = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        frac = digits(&mut i);
    }
    if int + frac == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// A `0x`/`0o`/`0b` literal's digits as a number; NaN when empty or when a
/// digit is out of range. Exact up to 128 bits.
fn non_decimal(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut exact: Option<u128> = Some(0);
    let mut approx = 0.0f64;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else {
            return f64::NAN;
        };
        exact = exact
            .and_then(|v| v.checked_mul(u128::from(radix)))
            .and_then(|v| v.checked_add(u128::from(d)));
        approx = approx * f64::from(radix) + f64::from(d);
    }
    // u128 to f64 rounds to nearest, ties to even, as the spec asks.
    exact.map_or(approx, |v| v as f64)
}

/// ECMA-262 `ToUint16`, what `String.fromCharCode` applies to its argument.
pub(crate) fn to_uint16(x: f64) -> u16 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(65536.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn truthiness_of_json_values() {
        for v in [json!(null), json!(false), json!(0), json!(-0.0), json!("")] {
            assert!(!truthy(Some(&v)), "{v}");
        }
        for v in [json!(true), json!(1), json!("0"), json!([]), json!({})] {
            assert!(truthy(Some(&v)), "{v}");
        }
        assert!(!truthy(None));
    }

    #[test]
    fn to_number_like_js() {
        // Number(x) in V8 for each input.
        let cases: &[(Value, f64)] = &[
            (json!(null), 0.0),
            (json!(true), 1.0),
            (json!(""), 0.0),
            (json!("  \n"), 0.0),
            (json!(" 12 "), 12.0),
            (json!("\u{FEFF}7\u{3000}"), 7.0),
            (json!("-8e1"), -80.0),
            (json!("+.5"), 0.5),
            (json!("5."), 5.0),
            (json!("0x10"), 16.0),
            (json!("0B101"), 5.0),
            (json!("0o17"), 15.0),
            (json!("-Infinity"), f64::NEG_INFINITY),
            (json!("065"), 65.0),
            (json!([]), 0.0),
            (json!([-3]), -3.0),
            (json!([["4"]]), 4.0),
            (json!([null]), 0.0),
        ];
        for (v, want) in cases {
            assert_eq!(to_number(v), *want, "{v}");
        }
        for v in [
            json!("-0x10"),
            json!("0x"),
            json!("inf"),
            json!("infinity"),
            json!("NaN"),
            json!("1_000"),
            json!("."),
            json!("1e"),
            json!("\u{85}1"),
            json!([1, 2]),
            json!({}),
            json!("ten"),
        ] {
            assert!(to_number(&v).is_nan(), "{v}");
        }
    }

    #[test]
    fn array_to_string_like_join() {
        assert_eq!(
            to_string(&json!([1, null, "a", [2, [3]], true, {}])),
            "1,,a,2,3,true,[object Object]"
        );
        assert_eq!(to_string(&json!([1e21, 1.5e-7])), "1e+21,1.5e-7");
    }

    #[test]
    fn trim_is_ecmascript_trim() {
        let units: Vec<u16> = "\u{FEFF}\u{A0} a \u{2029}".encode_utf16().collect();
        assert_eq!(trim_units(&units), &[0x61]);
        let nel: Vec<u16> = "\u{85}a".encode_utf16().collect();
        assert_eq!(trim_units(&nel), nel.as_slice());
        assert_eq!(trim_units(&[0x20, 0x20]), &[] as &[u16]);
    }

    #[test]
    fn uint16_wraps() {
        assert_eq!(to_uint16(65601.0), 65);
        assert_eq!(to_uint16(f64::INFINITY), 0);
        assert_eq!(to_uint16(f64::NAN), 0);
        assert_eq!(to_uint16(-1.0), 65535);
        assert_eq!(to_uint16(1e20), (1e20f64 % 65536.0) as u16);
    }

    #[test]
    fn numbers_written_like_json_stringify() {
        assert_eq!(number(-30.0), json!(-30));
        assert_eq!(number(-0.0), json!(0));
        assert_eq!(number(0.5), json!(0.5));
        assert_eq!(number(f64::NAN), Value::Null);
        assert_eq!(number(f64::INFINITY), Value::Null);
    }
}
