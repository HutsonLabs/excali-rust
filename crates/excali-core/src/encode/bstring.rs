//! Byte strings: `packages/excalidraw/data/encode.ts:14-39`.
//!
//! Upstream stores binary data in JS strings with one code unit per byte
//! (U+0000..U+00FF). In Rust such a string is a `String` whose chars are all
//! at most U+00FF; its UTF-8 form is not the bytes, so these helpers convert.

/// `toByteString(bytes)`: one char per byte.
pub fn to_byte_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// `byteStringToArrayBuffer(byteString)`: each UTF-16 code unit of the
/// string stored into a `Uint8Array`, which keeps its low 8 bits. For a real
/// byte string that is the inverse of [`to_byte_string`]; anything else is
/// truncated the way upstream truncates it.
pub fn byte_string_to_bytes(byte_string: &str) -> Vec<u8> {
    byte_string
        .encode_utf16()
        .map(|u| (u & 0xff) as u8)
        .collect()
}

/// `byteStringToString(byteString)`: the bytes decoded by
/// `new TextDecoder("utf-8")` — a leading BOM is removed and every invalid
/// sequence becomes U+FFFD (the WHATWG decoder's maximal-subpart rule, which
/// `String::from_utf8_lossy` also follows).
pub fn byte_string_to_string(byte_string: &str) -> String {
    let bytes = byte_string_to_bytes(byte_string);
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    String::from_utf8_lossy(bytes).into_owned()
}
