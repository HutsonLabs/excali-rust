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
//! name (`encode.ts:99-121`, `:123-144`), [`deflate`] reproduces pako's
//! compressed bytes exactly, and [`inflate`] / [`inflate_to_string`] follow
//! pako's decompression. The byte-string helpers are `encode.ts:14-39`.

mod bstring;
mod checksum;
mod deflate;
mod inflate;

use serde::{Deserialize, Serialize};

pub use bstring::{byte_string_to_bytes, byte_string_to_string, to_byte_string};
pub use deflate::deflate;
pub use inflate::{inflate, inflate_to_string, InflateError};

/// `EncodedData` (`encode.ts:87-94`). Field order is the order upstream's
/// object literal writes them (`encode.ts:115-120`), so [`EncodedData::to_json`]
/// matches `JSON.stringify(encode(...))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodedData {
    /// "version for potential migration purposes"; `"1"` when written by
    /// [`encode`], optional when read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Only `"bstring"` is known; [`decode`] rejects anything else.
    pub encoding: String,
    /// Whether `encoded` is zlib data. Absent reads as false, as upstream's
    /// `data.compressed` test treats `undefined`.
    #[serde(default)]
    pub compressed: bool,
    /// A byte string: one char (U+0000..U+00FF) per byte.
    pub encoded: String,
}

impl EncodedData {
    /// `JSON.stringify(data)`: compact, fields in declaration order, strings
    /// escaped as `JSON.stringify` escapes them.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("EncodedData serializes")
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
        encoding: "bstring".to_owned(),
        compressed: compress,
        encoded,
    }
}

/// Why [`decode`] failed. The `Display` text is what upstream's `decode`
/// throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// `decode: unknown encoding "<encoding>"`.
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
    if data.encoding != "bstring" {
        return Err(DecodeError::UnknownEncoding(data.encoding.clone()));
    }
    if data.compressed {
        Ok(inflate_to_string(&byte_string_to_bytes(&data.encoded))?)
    } else {
        Ok(byte_string_to_string(&data.encoded))
    }
}
