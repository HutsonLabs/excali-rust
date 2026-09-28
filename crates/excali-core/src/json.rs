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
}
