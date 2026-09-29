//! JavaScript value semantics the history port needs on JSON values.
//!
//! Upstream compares element and app state properties with `!==` and, for
//! two objects, a shallow `isShallowEqual` (`delta.ts:447-496`). A JSON
//! value has no identity, so [`same_value`] compares by content at every
//! depth. For the values the store compares this gives upstream's answer
//! wherever upstream's shallow compare looks deep enough (flat objects
//! such as `selectedElementIds`, arrays of primitives such as `groupIds`,
//! and `points`, whose pairs `ElementsDelta.postProcess` compares one level
//! further); for nested values upstream would see as different because
//! their inner objects are different instances (a `startBinding` rebuilt
//! with the same content), the port sees them as equal.

use std::cmp::Ordering;

use serde_json::{Number, Value};

/// A number as a JSON value, as `JSON.stringify` writes it: integers
/// without a fraction, `-0` as `0`, NaN and the infinities as `null`.
pub(crate) fn num(x: f64) -> Value {
    const MAX_SAFE: f64 = 9_007_199_254_740_992.0;
    if x.fract() == 0.0 && x.abs() <= MAX_SAFE {
        return Value::Number(Number::from(x as i64));
    }
    Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// Content equality of two JSON values; numbers by value.
pub(crate) fn deep_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| deep_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| deep_equal(v, w)))
        }
        _ => a == b,
    }
}

/// Equality of two property values, `None` being `undefined`.
pub(crate) fn same_value(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => deep_equal(a, b),
        _ => false,
    }
}

/// `typeof value === "object" && value !== null`.
pub(crate) fn is_object(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

/// JavaScript truthiness, `None` being `undefined`.
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// `Array.prototype.sort()` without a comparator on strings: UTF-16 code
/// unit order.
pub(crate) fn compare_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Whether `key` is an array index, which JavaScript enumerates before the
/// other keys of an object, in ascending order.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|&i| i != u32::MAX)
}

/// The keys of a JavaScript object built by inserting `keys` in order:
/// array indices first, ascending, then the others in insertion order.
pub(crate) fn js_key_order<'a, I: IntoIterator<Item = &'a String>>(keys: I) -> Vec<String> {
    let mut indices: Vec<(u32, String)> = Vec::new();
    let mut others = Vec::new();
    for key in keys {
        match array_index(key) {
            Some(i) => indices.push((i, key.clone())),
            None => others.push(key.clone()),
        }
    }
    indices.sort_by_key(|(i, _)| *i);
    indices.into_iter().map(|(_, k)| k).chain(others).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_compare_by_value() {
        assert!(deep_equal(&json!(1), &json!(1.0)));
        assert!(!deep_equal(&json!([1, 2]), &json!([2, 1])));
        assert!(deep_equal(
            &json!({"a": 1, "b": [1]}),
            &json!({"b": [1.0], "a": 1})
        ));
        assert!(!same_value(None, Some(&Value::Null)));
    }

    #[test]
    fn truthiness_follows_js() {
        assert!(!truthy(Some(&json!(""))));
        assert!(!truthy(Some(&json!(0))));
        assert!(truthy(Some(&json!({}))));
        assert!(truthy(Some(&json!("a"))));
    }

    #[test]
    fn integer_keys_come_first() {
        let keys: Vec<String> = ["b", "10", "a", "2", "01"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(js_key_order(&keys), ["2", "10", "b", "a", "01"]);
    }

    #[test]
    fn numbers_are_written_like_json_stringify() {
        assert_eq!(num(2.0), json!(2));
        assert_eq!(num(-0.0), json!(0));
        assert_eq!(num(f64::NAN), Value::Null);
        assert_eq!(num(0.5), json!(0.5));
    }
}
