//! The JavaScript conversions upstream code applies to untrusted JSON values
//! (ECMA-262 section 7.1): truthiness, `ToString` (which can throw),
//! `StringToNumber`, `parseFloat` and `parseInt`.
//!
//! `restoreAppState` and tinycolor2 read imported values without checking
//! their types, so a value of the wrong type still goes through these
//! conversions: `zoom: {value: "5"}` is concatenated with `Number.EPSILON`
//! as a string, an object with its own `toString` key throws when it is
//! converted. The port reproduces those results rather than guessing a
//! "sane" one.
//!
//! JSON values have no functions, so every own `toString` or `valueOf` key
//! of a parsed object is not callable, and `ToPrimitive` skips it
//! (section 7.1.1.1 `OrdinaryToPrimitive`).

use serde_json::{Map, Value};
use std::fmt;

use crate::json;

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

/// `Number::toString(x)` (section 6.1.6.1.20), non-finite values included.
pub(crate) fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_owned()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        json::js_number(x)
    }
}

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

/// `WhiteSpace` and `LineTerminator` (sections 12.2, 12.3): JS's `\s` and
/// what `String.prototype.trim`, `parseFloat` and `Number(string)` skip.
pub(crate) fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
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
    decimal_prefix(s.trim_start_matches(is_whitespace)).map_or(f64::NAN, |(_, x)| x)
}

/// `parseFloat(value)`: `ToString` first, which can throw.
pub(crate) fn parse_float_value(value: Option<&Value>) -> Result<f64, TypeError> {
    Ok(parse_float(&to_string(value)?))
}

/// `Number(string)` (section 7.1.4.1.1 `StringToNumber`).
pub(crate) fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(is_whitespace);
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

/// A JSON number for `x` as `JSON.stringify` would read it back: integral
/// values in the safe range as integers, others as floats, NaN and the
/// infinities as `null` (what `JSON.stringify` writes for them).
pub(crate) fn number_value(x: f64) -> Value {
    const MAX_SAFE: f64 = 9_007_199_254_740_992.0;
    if !x.is_finite() {
        Value::Null
    } else if x.fract() == 0.0 && x.abs() < MAX_SAFE {
        // -0 is written 0 by JSON.stringify.
        if x < 0.0 {
            Value::from(x as i64)
        } else {
            Value::from(x as u64)
        }
    } else {
        Value::from(x)
    }
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
}
