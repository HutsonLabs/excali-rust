//! The text encoding wrapper: `packages/excalidraw/data/encode.ts`.
//!
//! Upstream embeds a scene in PNG `tEXt` chunks and SVG `<metadata>` as
//! `JSON.stringify(encode({ text }))`, where `encode` zlib-compresses the
//! text with pako and stores the result as a byte string
//! (`site/content/research/data-model.md`, section 5):
//!
//! ```text
//! { "version": "1", "encoding": "bstring", "compressed": true, "encoded": "x\u009c..." }
//! ```
//!
//! [`encode`] and [`decode`] are ports of upstream's functions of the same
//! name (`encode.ts:99-121`, `:123-144`); [`deflate`] and [`inflate`] /
//! [`inflate_to_string`] are ports of pako 2.0.3's compressor and
//! decompressor, so bytes, text and error messages are pako's. The
//! byte-string helpers are `encode.ts:14-39`; [`btoa`], [`atob`],
//! [`string_to_base64`] and [`base64_to_string`] are `encode.ts:49-58`.

mod base64;
mod bstring;
pub(crate) mod checksum;
mod deflate;
mod inflate;
mod inftrees;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub use base64::{atob, base64_to_string, btoa, string_to_base64, InvalidCharacterError};
pub use bstring::{byte_string_to_bytes, byte_string_to_string, to_byte_string};
pub use deflate::deflate;
pub use inflate::{inflate, inflate_to_string, inflate_to_utf16, InflateError};

/// `EncodedData` (`encode.ts:87-94`). Field order is the order upstream's
/// object literal writes them (`encode.ts:115-120`), so [`EncodedData::to_json`]
/// matches `JSON.stringify(encode(...))`.
///
/// Reading is as lenient as upstream's `decode`, which takes whatever
/// `JSON.parse` produced: `encoding` may be missing or any JSON value (only
/// the string `"bstring"` is known), `compressed` is tested for JavaScript
/// truthiness, and `version` is ignored. `encoded` must be a string: for
/// anything else upstream's result is an accident of its byte-string
/// helpers (a `TypeError`, or an empty buffer that inflates to `undefined`),
/// and here it is a deserialization error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodedData {
    /// "version for potential migration purposes"; `"1"` when written by
    /// [`encode`]. A non-string value reads as `None`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "string_or_none"
    )]
    pub version: Option<String>,
    /// `Some("bstring")` when written by [`encode`]. `None` when the field
    /// is absent (upstream's `undefined`); any other JSON value is kept as
    /// is, so [`decode`] can report it as upstream does.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "any_value"
    )]
    pub encoding: Option<Value>,
    /// Whether `encoded` is zlib data: the truthiness of the JSON value
    /// (absent, `false`, `0`, `""` and `null` are false).
    #[serde(default, deserialize_with = "truthy")]
    pub compressed: bool,
    /// A byte string: one char (U+0000..U+00FF) per byte.
    pub encoded: String,
}

/// A present field, whatever its JSON value (`null` included).
fn any_value<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

fn string_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        _ => None,
    })
}

/// JavaScript truthiness of a JSON value.
fn truthy<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => false,
        Value::Bool(b) => b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    })
}

/// `String(value)` for a value `JSON.parse` can produce, as a template
/// literal interpolates it.
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number(n),
        Value::String(s) => s.clone(),
        // Array.prototype.toString: join(","), null and undefined as "".
        Value::Array(items) => items
            .iter()
            .map(|i| match i {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// `Number.prototype.toString()` for a parsed JSON number: integers print
/// without a fraction (`2.0` parses to `2`); other values print as the
/// shortest round-trip decimal, which Rust and JavaScript write alike for
/// magnitudes from 1e-6 up to 1e21 (outside that JavaScript switches to
/// exponent notation; only the text of an error message differs).
fn js_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(f64::NAN);
    if f == f.trunc() && f.abs() < 1e21 {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

impl EncodedData {
    /// `JSON.stringify(data)`: compact, fields in declaration order, strings
    /// escaped as `JSON.stringify` escapes them.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("EncodedData serializes")
    }

    /// `encoding` if it is a string.
    pub fn encoding_str(&self) -> Option<&str> {
        self.encoding.as_ref().and_then(Value::as_str)
    }
}

/// `encode({ text, compress })` (`encode.ts:99-121`): the UTF-8 bytes of
/// `text`, zlib-compressed when `compress` is true (upstream's default), as
/// a byte string.
///
/// Upstream falls back to the uncompressed form if pako throws; this
/// compressor cannot fail, so `compressed` always equals `compress`.
pub fn encode(text: &str, compress: bool) -> EncodedData {
    let encoded = if compress {
        to_byte_string(&deflate(text.as_bytes()))
    } else {
        to_byte_string(text.as_bytes())
    };
    EncodedData {
        version: Some("1".to_owned()),
        encoding: Some(Value::from("bstring")),
        compressed: compress,
        encoded,
    }
}

/// Why [`decode`] failed. The `Display` text is what upstream's `decode`
/// throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// `decode: unknown encoding "<encoding>"`, with the encoding as
    /// JavaScript's `String()` prints it (`undefined` when absent).
    UnknownEncoding(String),
    /// pako could not inflate `encoded`. For [`InflateError::Incomplete`]
    /// upstream's `decode` returns `undefined` rather than throwing.
    Inflate(InflateError),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEncoding(e) => write!(f, "decode: unknown encoding \"{e}\""),
            Self::Inflate(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnknownEncoding(_) => None,
            Self::Inflate(e) => Some(e),
        }
    }
}

impl From<InflateError> for DecodeError {
    fn from(e: InflateError) -> Self {
        Self::Inflate(e)
    }
}

/// `decode(data)` (`encode.ts:123-144`): the text an [`EncodedData`] holds.
///
/// Uncompressed data is the byte string decoded as UTF-8 by `TextDecoder`
/// ([`byte_string_to_string`]); compressed data is inflated and decoded by
/// pako ([`inflate_to_string`]). The two decoders differ on invalid UTF-8,
/// exactly as upstream's do.
pub fn decode(data: &EncodedData) -> Result<String, DecodeError> {
    // `switch (data.encoding) { case "bstring": ... }`: strict equality.
    match &data.encoding {
        Some(Value::String(e)) if e == "bstring" => {}
        Some(other) => return Err(DecodeError::UnknownEncoding(js_string(other))),
        None => return Err(DecodeError::UnknownEncoding("undefined".to_owned())),
    }
    if data.compressed {
        Ok(inflate_to_string(&byte_string_to_bytes(&data.encoded))?)
    } else {
        Ok(byte_string_to_string(&data.encoded))
    }
}
