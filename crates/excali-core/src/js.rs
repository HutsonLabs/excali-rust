//! The JavaScript semantics upstream code applies to untrusted JSON values
//! (ECMA-262): truthiness (`||`), nullishness (`??`), `ToString` (which can
//! throw), `ToNumber` and `StringToNumber`, `parseFloat`, `parseInt`,
//! `String.prototype.trim`, `ToUint16`, and number results written as
//! `JSON.stringify` writes them. Restore (elements, [`crate::restore`]),
//! `restoreAppState` ([`crate::app_state`]) and tinycolor2
//! ([`crate::color`]) share it, so they cannot drift apart on a conversion.
//!
//! Upstream reads imported values without checking their types, so a value
//! of the wrong type still goes through these conversions: `zoom: {value:
//! "5"}` is concatenated with `Number.EPSILON` as a string, and an object
//! with its own `toString` key throws when it is converted. The port
//! reproduces those results rather than guessing a "sane" one.
//!
//! A JSON value has no `undefined`: an absent key is `None` here. JSON
//! values have no functions either, so every own `toString` or `valueOf`
//! key of a parsed object is not callable, and `ToPrimitive` skips it
//! (section 7.1.1.1 `OrdinaryToPrimitive`). Strings may be in the crate's
//! sentinel form ([`crate::json`]); the conversions give the same answer
//! for a string and its sentinel form, since sentinels are neither empty
//! nor whitespace nor digits.

use serde_json::{Map, Number, Value};
use std::fmt;

/// A JavaScript `TypeError`, with V8's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeError: {}", self.0)
    }
}

impl std::error::Error for TypeError {}

/// `!!value` (section 7.1.2 `ToBoolean`) for a JSON value; `None` is
/// `undefined`.
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

pub(crate) use crate::json::number_to_string;

/// `String(value)`: `ToString` (section 7.1.17) of a JSON value, `None`
/// being `undefined`. An object converts through `ToPrimitive`: with its
/// own (so not callable) `toString` key, no method gives a primitive and
/// the conversion throws; otherwise it is `"[object Object]"`. An array is
/// its items joined by `,`, `null` items as the empty string.
pub(crate) fn to_string(value: Option<&Value>) -> Result<String, TypeError> {
    match value {
        None => Ok("undefined".to_owned()),
        Some(Value::Null) => Ok("null".to_owned()),
        Some(Value::Bool(b)) => Ok(b.to_string()),
        Some(Value::Number(n)) => Ok(number_to_string(n.as_f64().unwrap_or(f64::NAN))),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Array(items)) => {
            let parts = items
                .iter()
                .map(|item| match item {
                    Value::Null => Ok(String::new()),
                    item => to_string(Some(item)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(parts.join(","))
        }
        Some(Value::Object(map)) => object_to_string(map),
    }
}

fn object_to_string(map: &Map<String, Value>) -> Result<String, TypeError> {
    // toString is tried first for the string hint, valueOf first for the
    // default and number hints; for a parsed object either order ends the
    // same way: an own key hides Object.prototype.toString, and valueOf
    // (own or inherited) never gives a primitive.
    if map.contains_key("toString") {
        Err(TypeError(
            "Cannot convert object to primitive value".to_owned(),
        ))
    } else {
        Ok("[object Object]".to_owned())
    }
}

/// `Number(value)` for a JSON value (section 7.1.4 `ToNumber` after
/// `ToPrimitive` with hint number), `None` being `undefined` (NaN): `null`
/// 0, booleans 0 or 1, strings parsed as [`string_to_number`], arrays and
/// objects through their string form (`join(",")`, `"[object Object]"`),
/// which throws for an object with its own `toString` key.
pub(crate) fn to_number(value: Option<&Value>) -> Result<f64, TypeError> {
    Ok(match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => string_to_number(s),
        Some(other @ (Value::Array(_) | Value::Object(_))) => {
            string_to_number(&to_string(Some(other))?)
        }
    })
}

/// `value + number` (section 13.15.3 `ApplyStringOrNumericBinaryOperator`)
/// for a JSON value that is not `null` or `undefined`: a string, array or
/// object concatenates with the number's string form, anything else adds.
/// The result is given as a number, as upstream's next operation (`*`)
/// converts it.
pub(crate) fn add_number(value: &Value, number: f64) -> Result<f64, TypeError> {
    let primitive = match value {
        Value::Bool(b) => return Ok(f64::from(u8::from(*b)) + number),
        Value::Number(n) => return Ok(n.as_f64().unwrap_or(f64::NAN) + number),
        Value::Null => return Ok(number),
        other => to_string(Some(other))?,
    };
    Ok(string_to_number(&(primitive + &number_to_string(number))))
}

/// `value + n` for a JSON value, as a value: a string, array or object
/// concatenates with `n`'s string form (so `"10" + 1` is `"101"`), anything
/// else adds. [`add_number`] is the same sum when a number is wanted next.
pub(crate) fn plus_number(value: &Value, n: f64) -> Result<Value, TypeError> {
    Ok(match value {
        Value::Null => number(n),
        Value::Bool(b) => number(f64::from(u8::from(*b)) + n),
        Value::Number(x) => number(x.as_f64().unwrap_or(f64::NAN) + n),
        other => Value::String(to_string(Some(other))? + &number_to_string(n)),
    })
}

/// `WhiteSpace` and `LineTerminator` (sections 12.2, 12.3) for a code
/// point or UTF-16 code unit: JS's `\s` and what `String.prototype.trim`,
/// `parseFloat` and `Number(string)` skip. Unlike Rust's
/// `char::is_whitespace` this has U+FEFF and not U+0085.
pub(crate) fn is_whitespace(unit: u32) -> bool {
    matches!(
        unit,
        0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20 | 0xA0 | 0x1680 | 0x2000
            ..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// [`is_whitespace`] for a `char`.
pub(crate) fn is_whitespace_char(c: char) -> bool {
    is_whitespace(u32::from(c))
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

/// The longest prefix of `s` that is a `StrDecimalLiteral` (section 7.1.4.1),
/// and its value, or `None` when no prefix is one.
fn decimal_prefix(s: &str) -> Option<(usize, f64)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let negative = match bytes.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let signed = |x: f64| if negative { -x } else { x };
    if s[i..].starts_with("Infinity") {
        return Some((i + "Infinity".len(), signed(f64::INFINITY)));
    }
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let int_len = digits(i);
    let int_part = &s[i..i + int_len];
    i += int_len;
    let mut frac_part = "";
    if bytes.get(i) == Some(&b'.') {
        let frac_len = digits(i + 1);
        if int_len > 0 || frac_len > 0 {
            frac_part = &s[i + 1..i + 1 + frac_len];
            i += 1 + frac_len;
        }
    }
    if int_len == 0 && frac_part.is_empty() {
        return None;
    }
    let mut exponent = String::from("0");
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        let exp_sign = match bytes.get(j) {
            Some(b'-') => {
                j += 1;
                "-"
            }
            Some(b'+') => {
                j += 1;
                ""
            }
            _ => "",
        };
        let exp_len = digits(j);
        if exp_len > 0 {
            exponent = format!("{exp_sign}{}", &s[j..j + exp_len]);
            i = j + exp_len;
        }
    }
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    let frac_part = if frac_part.is_empty() { "0" } else { frac_part };
    // Rust's parser rounds a decimal to the nearest double, as JS does; an
    // exponent too long for i32 still parses (to 0 or infinity).
    let text = format!("{int_part}.{frac_part}e{exponent}");
    let value = text.parse::<f64>().unwrap_or_else(|_| {
        if exponent.starts_with('-') {
            0.0
        } else {
            f64::INFINITY
        }
    });
    Some((i, signed(value)))
}

/// `parseFloat(string)` (section 19.2.4).
pub(crate) fn parse_float(s: &str) -> f64 {
    decimal_prefix(s.trim_start_matches(is_whitespace_char)).map_or(f64::NAN, |(_, x)| x)
}

/// `parseFloat(value)`: `ToString` first, which can throw.
pub(crate) fn parse_float_value(value: Option<&Value>) -> Result<f64, TypeError> {
    Ok(parse_float(&to_string(value)?))
}

/// `Number(string)` (section 7.1.4.1.1 `StringToNumber`).
pub(crate) fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(is_whitespace_char);
    if s.is_empty() {
        return 0.0;
    }
    let radix = match s.get(..2) {
        Some("0x" | "0X") => Some(16),
        Some("0o" | "0O") => Some(8),
        Some("0b" | "0B") => Some(2),
        _ => None,
    };
    if let Some(radix) = radix {
        let digits = &s[2..];
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return f64::NAN;
        }
        return match u128::from_str_radix(digits, radix) {
            // u128 -> f64 rounds to nearest, ties to even, as JS does.
            Ok(n) => n as f64,
            Err(_) => digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            }),
        };
    }
    match decimal_prefix(s) {
        Some((len, x)) if len == s.len() => x,
        _ => f64::NAN,
    }
}

/// `parseInt(x, 10)` for a number: the integer prefix of `String(x)`.
pub(crate) fn parse_int_of_number(x: f64) -> f64 {
    let s = number_to_string(x);
    let (negative, rest) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.as_str()),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let value = digits.parse::<f64>().unwrap_or(f64::NAN);
    if negative {
        -value
    } else {
        value
    }
}

/// `value` as a JS number when it is one (`typeof value === "number"`).
pub(crate) fn as_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

/// `isFiniteNumber(value)` (`packages/math/src/utils.ts:28-30`).
pub(crate) fn is_finite_number(value: Option<&Value>) -> bool {
    as_number(value).is_some_and(f64::is_finite)
}

/// `value` when [`is_finite_number`] holds for it.
pub(crate) fn finite_number(value: Option<&Value>) -> Option<f64> {
    as_number(value).filter(|x| x.is_finite())
}

/// `a === b` for JSON values, `None` being `undefined`: numbers by value
/// (NaN unequal to itself), other primitives by type and value, and an
/// object or array never equal (JS compares those by identity, and two
/// parsed values are never the same object).
pub(crate) fn strictly_equal(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(x)), Some(Value::Number(y))) => x.as_f64() == y.as_f64(),
        (Some(Value::Array(_) | Value::Object(_)), _)
        | (_, Some(Value::Array(_) | Value::Object(_))) => false,
        (Some(a), Some(b)) => a == b,
        _ => false,
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
    fn to_string_follows_ecmascript() {
        assert_eq!(to_string(None).unwrap(), "undefined");
        assert_eq!(to_string(Some(&json!(null))).unwrap(), "null");
        assert_eq!(to_string(Some(&json!(1.5e-7))).unwrap(), "1.5e-7");
        assert_eq!(
            to_string(Some(&json!([1, null, [2, "a"], {}]))).unwrap(),
            "1,,2,a,[object Object]"
        );
        assert_eq!(
            to_string(Some(&json!({"valueOf": 1}))).unwrap(),
            "[object Object]"
        );
        assert!(to_string(Some(&json!({"toString": 1}))).is_err());
        assert!(to_string(Some(&json!([{"toString": 1}]))).is_err());
    }

    #[test]
    fn number_parsing_follows_ecmascript() {
        assert_eq!(parse_float("  3.5abc"), 3.5);
        assert_eq!(parse_float("-.5e1x"), -5.0);
        assert_eq!(parse_float("1e"), 1.0);
        assert_eq!(parse_float("1.e2"), 100.0);
        assert_eq!(parse_float("-Infinityx"), f64::NEG_INFINITY);
        assert!(parse_float(".").is_nan());
        assert!(parse_float("abc").is_nan());
        assert_eq!(string_to_number(" 12 "), 12.0);
        assert_eq!(string_to_number(""), 0.0);
        assert_eq!(string_to_number("0x1f"), 31.0);
        assert!(string_to_number("12px").is_nan());
        assert!(string_to_number("-0x1").is_nan());
        assert!(string_to_number("infinity").is_nan());
        assert_eq!(
            string_to_number("52.220446049250313e-16"),
            5.222044604925031e-15
        );
        assert_eq!(parse_int_of_number(12877.5), 12877.0);
        assert_eq!(parse_int_of_number(2.55e-7), 2.0);
    }

    #[test]
    fn addition_concatenates_strings() {
        let eps = f64::EPSILON;
        assert_eq!(add_number(&json!(true), eps).unwrap(), 1.0 + eps);
        assert_eq!(add_number(&json!("5"), eps).unwrap(), 5.222044604925031e-15);
        assert!(add_number(&json!({}), eps).unwrap().is_nan());
        assert!(add_number(&json!({"toString": 0}), eps).is_err());
    }

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
            assert_eq!(to_number(Some(v)).unwrap(), *want, "{v}");
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
            assert!(to_number(Some(&v)).unwrap().is_nan(), "{v}");
        }
    }

    #[test]
    fn array_to_string_like_join() {
        assert_eq!(
            to_string(Some(&json!([1, null, "a", [2, [3]], true, {}]))).unwrap(),
            "1,,a,2,3,true,[object Object]"
        );
        assert_eq!(
            to_string(Some(&json!([1e21, 1.5e-7]))).unwrap(),
            "1e+21,1.5e-7"
        );
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

    #[test]
    fn to_number_throws_where_to_primitive_does() {
        assert!(to_number(None).unwrap().is_nan());
        assert!(to_number(Some(&json!({"valueOf": -1}))).unwrap().is_nan());
        for v in [
            json!({"toString": 1}),
            json!([{"toString": 1}]),
            json!([[{"toString": null}]]),
        ] {
            assert_eq!(
                to_number(Some(&v)),
                Err(TypeError(
                    "Cannot convert object to primitive value".to_owned()
                )),
                "{v}"
            );
        }
    }

    #[test]
    fn whitespace_is_ecmascript_whitespace() {
        for c in [
            '\u{9}', '\u{b}', ' ', '\u{a0}', '\u{2000}', '\u{200a}', '\u{feff}', '\u{3000}',
        ] {
            assert!(is_whitespace(u32::from(c)), "{c:?}");
        }
        for c in ['\u{85}', '\u{200b}', '\u{180e}', 'a'] {
            assert!(!is_whitespace(u32::from(c)), "{c:?}");
        }
        assert_eq!(parse_float("\u{feff}\u{3000}2"), 2.0);
        assert!(parse_float("\u{85}2").is_nan());
    }

    #[test]
    fn plus_number_concatenates_or_adds_like_js() {
        let cases = [
            (json!(1), json!(2)),
            (json!("10"), json!("101")),
            (json!(true), json!(2)),
            (json!(null), json!(1)),
            (json!([1, 2]), json!("1,21")),
            (json!({}), json!("[object Object]1")),
            (json!(1e308), json!(1e308)),
        ];
        for (value, want) in cases {
            assert_eq!(plus_number(&value, 1.0).unwrap(), want, "{value} + 1");
        }
        assert_eq!(plus_number(&json!(1e308), 1e308).unwrap(), Value::Null);
        assert!(plus_number(&json!({"toString": 1}), 1.0).is_err());
    }

    #[test]
    fn strict_equality_and_finite_numbers() {
        assert!(strictly_equal(None, None));
        assert!(strictly_equal(Some(&json!(1)), Some(&json!(1.0))));
        assert!(strictly_equal(Some(&json!("a")), Some(&json!("a"))));
        assert!(!strictly_equal(Some(&json!(1)), Some(&json!("1"))));
        assert!(!strictly_equal(Some(&json!(null)), None));
        assert!(!strictly_equal(Some(&json!([])), Some(&json!([]))));
        assert!(!strictly_equal(Some(&json!({})), Some(&json!({}))));
        assert_eq!(finite_number(Some(&json!(2.5))), Some(2.5));
        assert_eq!(finite_number(Some(&json!("2"))), None);
        assert_eq!(finite_number(None), None);
    }
}
