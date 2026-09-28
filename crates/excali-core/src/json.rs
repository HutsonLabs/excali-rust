//! Untyped JSON round trip with upstream's output shape.
//!
//! Upstream writes `.excalidraw` files with `JSON.stringify(data, null, 2)`
//! (`packages/excalidraw/data/json.ts`, `serializeAsJSON`): two-space indent,
//! keys in insertion order, no trailing newline. This module reproduces that
//! shape for an arbitrary JSON value. The typed `Document` codec builds on it.

/// Error returned when the input is not valid JSON.
pub type Error = serde_json::Error;

/// Serialise a JSON value as `JSON.stringify(value, null, 2)` would.
pub fn to_string_pretty(value: &serde_json::Value) -> String {
    // Serialising a `Value` into a String cannot fail: every key is a string
    // and there is no I/O.
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Parse `text` and write it back in upstream's shape.
///
/// A file that upstream wrote comes back byte-identical (minus any trailing
/// newline an editor added), key order included.
pub fn round_trip(text: &str) -> Result<String, Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(to_string_pretty(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_containers_are_compact_like_json_stringify() {
        // JSON.stringify({a: [], b: {}}, null, 2) === '{\n  "a": [],\n  "b": {}\n}'
        assert_eq!(
            round_trip(r#"{"a":[],"b":{}}"#).unwrap(),
            "{\n  \"a\": [],\n  \"b\": {}\n}"
        );
    }

    #[test]
    fn nested_arrays_indent_two_spaces_per_level() {
        // JSON.stringify({p: [[0, 0], [10, 5]]}, null, 2)
        assert_eq!(
            round_trip(r#"{"p":[[0,0],[10,5]]}"#).unwrap(),
            "{\n  \"p\": [\n    [\n      0,\n      0\n    ],\n    [\n      10,\n      5\n    ]\n  ]\n}"
        );
    }

    #[test]
    fn non_ascii_is_written_raw_not_escaped() {
        // JSON.stringify("日本") === '"日本"'
        assert_eq!(round_trip(r#""日本""#).unwrap(), "\"日本\"");
    }
    // Every expected string below was produced by node v26.10.0 with
    // `JSON.stringify(JSON.parse(input), null, 2)` (or without the indent
    // arguments where the input is a scalar); JSON.stringify implements
    // ECMAScript Number::toString for numbers and well-formed JSON.stringify
    // (lone surrogates written as `\uXXXX` escapes) for strings.

    /// `(input, node output)` pairs for scalar numbers.
    const NUMBERS: &[(&str, &str)] = &[
        ("0.000001", "0.000001"),
        ("1e-6", "0.000001"),
        ("1.5e-7", "1.5e-7"),
        ("1e-7", "1e-7"),
        ("0.0000001234", "1.234e-7"),
        ("-2e-7", "-2e-7"),
        ("1e21", "1e+21"),
        ("1.25e+21", "1.25e+21"),
        ("999999999999999999999", "1e+21"),
        ("1e20", "100000000000000000000"),
        ("1.0", "1"),
        ("1E2", "100"),
        ("100", "100"),
        ("-0", "0"),
        ("-0.0", "0"),
        ("0.1", "0.1"),
        ("-1.5", "-1.5"),
        ("2.5E-3", "0.0025"),
        ("3.14159", "3.14159"),
        ("123456.789", "123456.789"),
        ("0.30000000000000004", "0.30000000000000004"),
        ("123e-20", "1.23e-18"),
        ("5e-324", "5e-324"),
        ("1.7976931348623157e308", "1.7976931348623157e+308"),
        ("123456789012345678901234", "1.2345678901234569e+23"),
        ("12345678901234567890", "12345678901234567000"),
        ("9007199254740993", "9007199254740992"),
        ("-9223372036854775808", "-9223372036854776000"),
        // Two shortest strings equally close to the value: ECMA-262 takes
        // the one with the even last digit.
        ("571516643625357.25", "571516643625357.2"),
        ("82075870703310.125", "82075870703310.12"),
        ("2806231691801.40625", "2806231691801.4062"),
    ];

    #[test]
    fn numbers_are_written_as_ecmascript_number_to_string() {
        for (input, node) in NUMBERS {
            assert_eq!(round_trip(input).unwrap(), *node, "input {input}");
        }
    }

    #[test]
    fn numbers_inside_a_document_match_node() {
        assert_eq!(
            round_trip(r#"{"p":[1.0,-0,0.000001,1.5e-7,1e21,123456789012345678901234]}"#)
                .unwrap(),
            "{\n  \"p\": [\n    1,\n    0,\n    0.000001,\n    1.5e-7,\n    1e+21,\n    1.2345678901234569e+23\n  ]\n}"
        );
    }

    #[test]
    fn to_string_pretty_formats_f64_values_like_node() {
        let value = serde_json::json!([1.0, -0.0, 0.000001, 1e21]);
        assert_eq!(
            to_string_pretty(&value),
            "[\n  1,\n  0,\n  0.000001,\n  1e+21\n]"
        );
    }

    #[test]
    fn lone_surrogate_escape_is_kept() {
        assert_eq!(
            round_trip(r#"{"x":"\ud800"}"#).unwrap(),
            "{\n  \"x\": \"\\ud800\"\n}"
        );
    }

    #[test]
    fn split_emoji_keeps_lone_halves_and_decodes_the_pair() {
        assert_eq!(
            round_trip(r#"{"x":"a\udc00b\ud83d\ude00c\ud83d"}"#).unwrap(),
            "{\n  \"x\": \"a\\udc00b\u{1F600}c\\ud83d\"\n}"
        );
    }

    #[test]
    fn lone_surrogates_in_keys_and_repeated_are_kept() {
        assert_eq!(
            round_trip(r#"{"\ud800k":"\ud800\ud800"}"#).unwrap(),
            "{\n  \"\\ud800k\": \"\\ud800\\ud800\"\n}"
        );
    }

    #[test]
    fn noncharacter_next_to_lone_surrogate_is_written_raw() {
        assert_eq!(
            round_trip(r#"{"x":"\ufdd0\ud800"}"#).unwrap(),
            "{\n  \"x\": \"\u{FDD0}\\ud800\"\n}"
        );
        assert_eq!(
            round_trip("[\"\u{FDD0}\u{E000}\"]").unwrap(),
            "[\n  \"\u{FDD0}\u{E000}\"\n]"
        );
    }

    #[test]
    fn escaped_backslash_before_u_is_not_a_surrogate_escape() {
        assert_eq!(
            round_trip(r#"["\\ud800","\"\ud800"]"#).unwrap(),
            "[\n  \"\\\\ud800\",\n  \"\\\"\\ud800\"\n]"
        );
    }

    #[test]
    fn string_escapes_match_node() {
        assert_eq!(
            round_trip(r#""\u0000\b\t\n\u000b\f\r\u001f\"\\/\u007f\u2028\u00e9""#).unwrap(),
            "\"\\u0000\\b\\t\\n\\u000b\\f\\r\\u001f\\\"\\\\/\u{7f}\u{2028}\u{e9}\""
        );
    }

    #[test]
    fn array_index_keys_come_first_like_a_js_object() {
        assert_eq!(
            round_trip(
                r#"{"b":1,"10":2,"2":3,"01":4,"4294967295":5,"4294967294":6,"-1":7,"a":{"z":0,"0":1}}"#
            )
            .unwrap(),
            "{\n  \"2\": 3,\n  \"10\": 2,\n  \"4294967294\": 6,\n  \"b\": 1,\n  \"01\": 4,\n  \"4294967295\": 5,\n  \"-1\": 7,\n  \"a\": {\n    \"0\": 1,\n    \"z\": 0\n  }\n}"
        );
    }

    #[test]
    fn duplicate_key_keeps_first_position_and_last_value() {
        assert_eq!(
            round_trip(r#"{"a":1,"a":2,"b":3}"#).unwrap(),
            "{\n  \"a\": 2,\n  \"b\": 3\n}"
        );
    }
}
