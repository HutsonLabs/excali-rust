//! The scene embedded in an exported SVG's `<metadata>`: upstream's
//! `encodeSvgBase64Payload` and `decodeSvgBase64Payload`
//! (`packages/excalidraw/scene/export.ts:510-563`,
//! `site/content/research/data-model.md` section 5).
//!
//! The writer appends four comments and a text node to `<metadata>`:
//!
//! ```text
//! <!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->BASE64<!-- payload-end -->
//! ```
//!
//! where `BASE64 = btoa(JSON.stringify(encode({ text: payload })))`: the
//! compressed [`crate::encode::EncodedData`] wrapper, whose JSON is already a
//! byte string. Version 1 files (no `payload-version` comment) hold the
//! base64 of UTF-8 text instead, usually raw scene JSON.

use serde_json::Value;

use crate::constants::EXPORT_DATA_TYPE_EXCALIDRAW;
use crate::encode::{
    base64_to_string, decode, encode, string_to_base64, to_byte_string, DecodeError, EncodedData,
    InflateError,
};
use crate::json;

/// `MIME_TYPES.excalidraw` (`packages/common/src/constants.ts`), the
/// `payload-type` of an embedded scene.
pub const PAYLOAD_MIME_TYPE: &str = "application/vnd.excalidraw+json";

const START: &str = "<!-- payload-start -->";
const END: &str = "<!-- payload-end -->";
const VERSION: &str = "<!-- payload-version:";

/// `createHTMLComment(text)` (`export.ts:286-291`) as serialized markup: the
/// text surrounded by spaces.
fn comment(text: &str) -> String {
    format!("<!-- {text} -->")
}

/// `encodeSvgBase64Payload({ payload, metadataElement })`
/// (`export.ts:510-529`): the markup it appends to the `<metadata>` element,
/// as `outerHTML` serializes it. `payload` is the scene JSON
/// (`serializeAsJSON(..., "local")`).
///
/// The text node is base64, which needs no escaping.
pub fn encode_svg_base64_payload(payload: &str) -> String {
    let base64 = svg_base64_payload(payload);
    let mut out = String::with_capacity(base64.len() + 128);
    out.push_str(&comment(&format!("payload-type:{PAYLOAD_MIME_TYPE}")));
    out.push_str(&comment("payload-version:2"));
    out.push_str(&comment("payload-start"));
    out.push_str(&base64);
    out.push_str(&comment("payload-end"));
    out
}

/// The text node [`encode_svg_base64_payload`] puts between
/// `payload-start` and `payload-end`: `btoa(JSON.stringify(encode({ text:
/// payload })))`, for a writer that builds the comments itself (the SVG
/// export, [`SVG_PAYLOAD_VERSION`]).
pub fn svg_base64_payload(payload: &str) -> String {
    let wrapper = encode(payload, true).to_json();
    // Every char of the wrapper's JSON is at most U+00FF: the fields are
    // ASCII and `encoded` is a byte string, so btoa cannot fail.
    string_to_base64(&wrapper, true).expect("EncodedData JSON is a byte string")
}

/// The `payload-version` the writer records: 2, a byte-string payload.
pub const SVG_PAYLOAD_VERSION: u32 = 2;

/// Why [`decode_svg_base64_payload`] produced no scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvgPayloadError {
    /// Upstream's `Error("INVALID")`: no `payload-type:application/vnd.excalidraw+json`
    /// in the SVG, or no payload between start and end comments. Upstream's
    /// loader reports it as "Image doesn't contain scene"
    /// (`packages/excalidraw/data/blob.ts:62-75`).
    Invalid,
    /// Upstream's `Error("FAILED")`: the payload is not base64, not JSON,
    /// neither an encoded wrapper nor scene JSON, or a wrapper `decode`
    /// rejects. The string is the underlying reason (upstream logs it with
    /// `console.error`); `Display` prints only `FAILED`.
    Failed(String),
    /// A compressed wrapper whose zlib stream ends early. pako gives up
    /// without throwing, so upstream's `decode` and
    /// `decodeSvgBase64Payload` return `undefined` instead of a scene.
    Incomplete,
}

impl std::fmt::Display for SvgPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => f.write_str("INVALID"),
            Self::Failed(_) => f.write_str("FAILED"),
            Self::Incomplete => f.write_str("undefined"),
        }
    }
}

impl std::error::Error for SvgPayloadError {}

/// `decodeSvgBase64Payload({ svg })` (`export.ts:531-563`): the text of the
/// scene embedded in an SVG document (or any markup holding the metadata
/// comments).
pub fn decode_svg_base64_payload(svg: &str) -> Result<String, SvgPayloadError> {
    if !svg.contains(&format!("payload-type:{PAYLOAD_MIME_TYPE}")) {
        return Err(SvgPayloadError::Invalid);
    }
    let base64 = match_payload(svg).ok_or(SvgPayloadError::Invalid)?;
    // `versionMatch?.[1] || "1"`; `\d+` is never empty.
    let is_byte_string = match_version(svg).is_some_and(|v| v != "1");

    let failed = |reason: String| SvgPayloadError::Failed(reason);
    let json = base64_to_string(base64, is_byte_string).map_err(|e| failed(e.to_string()))?;
    let parsed = json::parse(&json).map_err(|e| failed(format!("SyntaxError: {e}")))?;
    let Value::Object(object) = parsed else {
        // `"encoded" in x` throws for primitives; an array has neither key.
        return Err(failed("not an object".to_owned()));
    };
    let Some(encoded) = object.get("encoded") else {
        // Legacy, un-encoded scene JSON.
        return match object.get("type") {
            Some(Value::String(t)) if t == EXPORT_DATA_TYPE_EXCALIDRAW => Ok(json),
            _ => Err(failed("not a scene".to_owned())),
        };
    };
    // `encoded` goes through byteStringToArrayBuffer, which keeps the low
    // byte of each UTF-16 code unit, lone surrogates included. Rebuild it
    // from the code units so the parse's lone-surrogate sentinels are not
    // mistaken for characters.
    let mut object = object.clone();
    if let Value::String(s) = encoded {
        let bytes: Vec<u8> = json::to_utf16(s).iter().map(|u| (u & 0xff) as u8).collect();
        object.insert("encoded".to_owned(), Value::String(to_byte_string(&bytes)));
    }
    let data: EncodedData =
        serde_json::from_value(Value::Object(object)).map_err(|e| failed(e.to_string()))?;
    match decode(&data) {
        Ok(text) => Ok(text),
        Err(DecodeError::Inflate(InflateError::Incomplete)) => Err(SvgPayloadError::Incomplete),
        Err(e) => Err(failed(e.to_string())),
    }
}

/// JavaScript's `\s`: `WhiteSpace` and `LineTerminator`.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JavaScript's `LineTerminator`, which `.` does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Whether `\s*<!-- payload-end -->` matches at the start of `rest`.
fn end_follows(rest: &str) -> bool {
    rest.trim_start_matches(is_js_space).starts_with(END)
}

/// Capture group 1 of `/<!-- payload-start -->\s*(.+?)\s*<!-- payload-end -->/`,
/// with the backtracking order of a JavaScript regex: the leftmost start
/// comment that can match, the greediest leading `\s*`, then the shortest
/// capture.
fn match_payload(svg: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = svg[from..].find(START) {
        let after = from + i + START.len();
        let rest = &svg[after..];
        let leading = rest.len() - rest.trim_start_matches(is_js_space).len();
        // Greedy `\s*` gives back one whitespace char at a time.
        let mut starts: Vec<usize> = rest[..leading]
            .char_indices()
            .map(|(j, _)| after + j)
            .collect();
        starts.push(after + leading);
        for &cap in starts.iter().rev() {
            // Lazy `.+?`: one char at a time, never a line terminator.
            for (j, c) in svg[cap..].char_indices() {
                if is_line_terminator(c) {
                    break;
                }
                let end = cap + j + c.len_utf8();
                if end_follows(&svg[end..]) {
                    return Some(&svg[cap..end]);
                }
            }
        }
        from = from + i + 1;
    }
    None
}

/// Capture group 1 of `/<!-- payload-version:(\d+) -->/`.
fn match_version(svg: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = svg[from..].find(VERSION) {
        let digits_at = from + i + VERSION.len();
        let rest = &svg[digits_at..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && rest[digits..].starts_with(" -->") {
            return Some(&rest[..digits]);
        }
        from = from + i + 1;
    }
    None
}
