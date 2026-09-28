//! A scene embedded in a PNG: `packages/excalidraw/data/image.ts`.
//!
//! Upstream stores the scene in a `tEXt` chunk inserted just before `IEND`
//! (`encodePngMetadata`, `image.ts:25-47`): keyword
//! `application/vnd.excalidraw+json` ([`MIME_TYPE_EXCALIDRAW`]), text
//! `JSON.stringify(encode({ text: sceneJSON, compress: true }))`, the payload
//! wrapper of [`crate::encode`] (`site/content/research/data-model.md`,
//! section 5). Reading (`decodePngMetadata`, `image.ts:49-71`) takes the
//! first `tEXt` chunk and accepts the wrapper or, from before the wrapper
//! existed, the raw scene JSON (`"type": "excalidraw"`).
//!
//! Upstream does the PNG work with three npm packages at the versions its
//! lock file pins, ported here function for function so that bytes, results
//! and error messages are theirs:
//!
//! - png-chunks-extract 1.0.0 ([`extract_chunks`]);
//! - png-chunks-encode 1.0.0 ([`encode_chunks`]);
//! - png-chunk-text 1.0.0 ([`encode_text_chunk`], [`decode_text_chunk`]).
//!
//! Their quirks are kept. png-chunks-extract reads past the end of a
//! truncated file as zero bytes (a JavaScript typed array stores
//! `undefined` as 0), so a cut file fails the CRC check of the chunk it cuts
//! (or, if the zero-padded chunk happens to match, reports the missing
//! `IEND`); anything after `IEND` is dropped and `IEND`'s own data is not
//! read. png-chunk-text keeps keyword and text as Latin-1 (one char per
//! byte), so a legacy scene whose JSON holds UTF-8 bytes reads as their
//! Latin-1 characters, exactly as upstream returns it.

use serde_json::{Map, Value};

use crate::constants::{EXPORT_DATA_TYPE_EXCALIDRAW, MIME_TYPE_EXCALIDRAW};
use crate::encode::checksum::{crc32, crc32_zeros};
use crate::encode::{decode, encode, to_byte_string, DecodeError, EncodedData, InflateError};
use crate::js;
use crate::json;

/// The PNG signature png-chunks-encode writes and png-chunks-extract checks.
const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// A PNG chunk as the png-chunks packages represent it: `{ name, data }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The four type bytes (`String.fromCharCode` of each upstream).
    pub name: [u8; 4],
    /// The chunk data, without length, type or CRC.
    pub data: Vec<u8>,
}

/// A decoded `tEXt` chunk: png-chunk-text's `{ keyword, text }`, each byte
/// one char (Latin-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChunk {
    pub keyword: String,
    pub text: String,
}

/// An error png-chunks-extract or png-chunk-text throws. `Display` is the
/// package's message, which upstream passes on unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngError {
    /// A signature byte other than those below is wrong.
    InvalidHeader,
    /// Signature byte 4, 5 or 7 (CR or LF) is wrong.
    InvalidHeaderLineEnding,
    /// The first chunk is not `IHDR`.
    IhdrMissing,
    /// A chunk's stored CRC is not the CRC-32 of its type and data.
    CrcMismatch { name: [u8; 4] },
    /// The file ended before an `IEND` chunk.
    NoIend,
    /// A NUL byte in the text of a `tEXt` chunk (decoding).
    NulInText,
    /// A keyword or text that is empty or has a char above U+00FF
    /// (encoding).
    NotLatin1,
    /// A keyword of 80 chars or more (encoding).
    KeywordTooLong(String),
    /// A NUL char in the keyword (encoding).
    NulInKeyword,
    /// A NUL char in the text (encoding).
    NulInContent,
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHeader => f.write_str("Invalid .png file header"),
            Self::InvalidHeaderLineEnding => f.write_str(
                "Invalid .png file header: possibly caused by DOS-Unix line ending conversion?",
            ),
            Self::IhdrMissing => f.write_str("IHDR header missing"),
            Self::CrcMismatch { name } => write!(
                f,
                "CRC values for {} header do not match, PNG file is likely corrupted",
                latin1(name)
            ),
            Self::NoIend => f.write_str(".png file ended prematurely: no IEND header was found"),
            Self::NulInText => f.write_str(
                "Invalid NULL character found. 0x00 character is not permitted in tEXt content",
            ),
            Self::NotLatin1 => f.write_str(
                "Only Latin-1 characters are permitted in PNG tEXt chunks. You might want to consider base64 encoding and/or zEXt compression",
            ),
            Self::KeywordTooLong(keyword) => write!(
                f,
                "Keyword \"{keyword}\" is longer than the 79-character limit imposed by the PNG specification"
            ),
            Self::NulInKeyword => f.write_str("0x00 character is not permitted in tEXt keywords"),
            Self::NulInContent => f.write_str("0x00 character is not permitted in tEXt content"),
        }
    }
}

impl std::error::Error for PngError {}

/// Why [`decode_png_metadata`] failed. `Display` is what upstream throws:
/// the png package's message, `INVALID` or `FAILED`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodePngMetadataError {
    /// Reading the chunks or the `tEXt` chunk failed; thrown as is.
    Png(PngError),
    /// No `tEXt` chunk, or its keyword is not [`MIME_TYPE_EXCALIDRAW`]
    /// (`throw new Error("INVALID")`).
    Invalid,
    /// The text is not a payload (`throw new Error("FAILED")`).
    Failed(Failure),
}

/// What upstream's `try` block caught before throwing `FAILED`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// `JSON.parse` threw; the message is serde_json's.
    Json(String),
    /// A JSON value that `"encoded" in value` throws on (not an object).
    NotAnObject,
    /// An object without `encoded` whose `type` is not `"excalidraw"`.
    NotAScene,
    /// `encoded` is a value upstream's byte-string helper throws on (`null`,
    /// a non-empty array, an object whose `length` is positive or not a
    /// valid array length).
    EncodedNotAByteString,
    /// The payload codec's `decode` threw.
    Decode(DecodeError),
}

impl std::fmt::Display for DecodePngMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => e.fmt(f),
            Self::Invalid => f.write_str("INVALID"),
            Self::Failed(_) => f.write_str("FAILED"),
        }
    }
}

impl std::error::Error for DecodePngMetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Png(e) => Some(e),
            Self::Failed(Failure::Decode(e)) => Some(e),
            _ => None,
        }
    }
}

impl From<PngError> for DecodePngMetadataError {
    fn from(e: PngError) -> Self {
        Self::Png(e)
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

// ---------------------------------------------------------------------------
// png-chunks-extract, png-chunks-encode, png-chunk-text

/// png-chunks-extract 1.0.0: the chunks of a PNG, up to and including
/// `IEND` (whose data is always empty).
///
/// Bytes past the end of `data` read as 0, as upstream's typed-array reads
/// do. A chunk whose declared length runs past the end is never allocated:
/// its CRC is continued over the implied zeros in O(log n), and it either
/// fails the CRC check or ends the file without `IEND`, as upstream would
/// after allocating it.
pub fn extract_chunks(data: &[u8]) -> Result<Vec<Chunk>, PngError> {
    let len = data.len() as u64;
    let at = |i: u64| -> u8 {
        if i < len {
            data[i as usize]
        } else {
            0
        }
    };
    let be32 = |i: u64| -> u32 { u32::from_be_bytes([at(i), at(i + 1), at(i + 2), at(i + 3)]) };

    for (i, &want) in SIGNATURE.iter().enumerate() {
        if at(i as u64) != want || i as u64 >= len {
            return Err(if matches!(i, 4 | 5 | 7) {
                PngError::InvalidHeaderLineEnding
            } else {
                PngError::InvalidHeader
            });
        }
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut idx: u64 = 8;
    while idx < len {
        let length = u64::from(be32(idx));
        idx += 4;
        let name = [at(idx), at(idx + 1), at(idx + 2), at(idx + 3)];
        idx += 4;

        if chunks.is_empty() && &name != b"IHDR" {
            return Err(PngError::IhdrMissing);
        }
        if &name == b"IEND" {
            chunks.push(Chunk {
                name,
                data: Vec::new(),
            });
            return Ok(chunks);
        }

        let start = idx;
        idx += length;
        let crc_actual = be32(idx);
        idx += 4;

        // The data present in the file, then the zeros read past its end.
        let present = &data[(start.min(len) as usize)..(idx - 4).min(len) as usize];
        let missing = length - present.len() as u64;
        let crc_expect = crc32_zeros(crc32(crc32(0, &name), present), missing);
        if crc_expect != crc_actual {
            return Err(PngError::CrcMismatch { name });
        }
        if idx > len {
            // Upstream pushes the zero-padded chunk, then leaves the loop
            // without having seen IEND.
            break;
        }
        chunks.push(Chunk {
            name,
            data: present.to_vec(),
        });
    }
    Err(PngError::NoIend)
}

/// png-chunks-encode 1.0.0: the signature, then each chunk as length, type,
/// data and the CRC-32 of type and data.
pub fn encode_chunks(chunks: &[Chunk]) -> Vec<u8> {
    let size = 8 + chunks.iter().map(|c| c.data.len() + 12).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&SIGNATURE);
    for chunk in chunks {
        // `uint32[0] = size`: the length modulo 2^32.
        out.extend_from_slice(&(chunk.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&chunk.name);
        out.extend_from_slice(&chunk.data);
        out.extend_from_slice(&crc32(crc32(0, &chunk.name), &chunk.data).to_be_bytes());
    }
    out
}

/// png-chunk-text 1.0.0 `encode(keyword, content)`: a `tEXt` chunk of the
/// keyword, a NUL and the content, one byte per char.
pub fn encode_text_chunk(keyword: &str, content: &str) -> Result<Chunk, PngError> {
    // `/^[\x00-\xFF]+$/`: non-empty, every UTF-16 code unit below 0x100.
    let latin1_ok = |s: &str| !s.is_empty() && s.chars().all(|c| u32::from(c) <= 0xff);
    if !latin1_ok(keyword) || !latin1_ok(content) {
        return Err(PngError::NotLatin1);
    }
    // Latin-1 only, so chars are UTF-16 code units.
    if keyword.chars().count() >= 80 {
        return Err(PngError::KeywordTooLong(keyword.to_owned()));
    }
    if keyword.contains('\0') {
        return Err(PngError::NulInKeyword);
    }
    if content.contains('\0') {
        return Err(PngError::NulInContent);
    }
    let mut data = Vec::with_capacity(keyword.len() + content.len() + 1);
    data.extend(keyword.chars().map(|c| u32::from(c) as u8));
    data.push(0);
    data.extend(content.chars().map(|c| u32::from(c) as u8));
    Ok(Chunk {
        name: *b"tEXt",
        data,
    })
}

/// png-chunk-text 1.0.0 `decode(data)`: the keyword up to the first NUL, the
/// text after it (empty without a NUL); a second NUL is an error.
pub fn decode_text_chunk(data: &[u8]) -> Result<TextChunk, PngError> {
    let (keyword, text) = match data.iter().position(|&b| b == 0) {
        Some(i) => (&data[..i], &data[i + 1..]),
        None => (data, &[][..]),
    };
    if text.contains(&0) {
        return Err(PngError::NulInText);
    }
    Ok(TextChunk {
        keyword: latin1(keyword),
        text: latin1(text),
    })
}

// ---------------------------------------------------------------------------
// image.ts

/// `getTEXtChunk` (`image.ts:15-23`): the first `tEXt` chunk, decoded, or
/// `None` when there is none.
pub fn get_text_chunk(png: &[u8]) -> Result<Option<TextChunk>, PngError> {
    extract_chunks(png)?
        .iter()
        .find(|c| &c.name == b"tEXt")
        .map(|c| decode_text_chunk(&c.data))
        .transpose()
}

/// `encodePngMetadata` (`image.ts:25-47`): `png` with a `tEXt` chunk holding
/// `metadata` (compressed, in the payload wrapper) inserted before its last
/// chunk, `IEND`.
///
/// The PNG is re-encoded from its chunks, so anything after `IEND` is
/// dropped and `IEND` is written with empty data. Fails only when `png` is
/// not a PNG png-chunks-extract accepts.
pub fn encode_png_metadata(png: &[u8], metadata: &str) -> Result<Vec<u8>, PngError> {
    let mut chunks = extract_chunks(png)?;
    let chunk = encode_text_chunk(MIME_TYPE_EXCALIDRAW, &encode(metadata, true).to_json())
        .expect("JSON.stringify of a byte-string wrapper is non-empty Latin-1 without NUL");
    // `chunks.splice(-1, 0, metadataChunk)`; extract_chunks always ends with
    // IEND, so there is a last chunk.
    let last = chunks.len() - 1;
    chunks.insert(last, chunk);
    Ok(encode_chunks(&chunks))
}

/// `decodePngMetadata` (`image.ts:49-71`): the scene text embedded in `png`.
///
/// - `Ok(Some(text))`: the payload decoded, or the legacy raw scene JSON as
///   the chunk holds it (Latin-1);
/// - `Ok(None)`: upstream returns `undefined`, which pako's inflate gives
///   for incomplete compressed data;
/// - `Err`: what upstream throws (see [`DecodePngMetadataError`]).
///
/// The text is read as `JSON.parse` reads it (duplicate keys: the last
/// wins; `\uXXXX` escapes of lone surrogates kept as code units), and the
/// wrapper as leniently as upstream's `decode`, including an `encoded` that
/// is not a string (see [`byte_string_of`]).
pub fn decode_png_metadata(png: &[u8]) -> Result<Option<String>, DecodePngMetadataError> {
    let chunk = match get_text_chunk(png)? {
        Some(c) if c.keyword == MIME_TYPE_EXCALIDRAW => c,
        _ => return Err(DecodePngMetadataError::Invalid),
    };
    let failed = |f: Failure| DecodePngMetadataError::Failed(f);
    let parsed = json::parse(&chunk.text).map_err(|e| failed(Failure::Json(e.to_string())))?;
    let Value::Object(map) = parsed else {
        return Err(failed(Failure::NotAnObject));
    };
    let Some(encoded) = map.get("encoded") else {
        // Legacy, un-encoded scene JSON.
        return if map.get("type").and_then(Value::as_str) == Some(EXPORT_DATA_TYPE_EXCALIDRAW) {
            Ok(Some(chunk.text))
        } else {
            Err(failed(Failure::NotAScene))
        };
    };

    // `decode(encodedData)`: the encoding is checked before `encoded` is
    // touched.
    let mut wrapper = Map::new();
    for key in ["encoding", "compressed"] {
        if let Some(v) = map.get(key) {
            wrapper.insert(key.to_owned(), json::decode(v));
        }
    }
    let encoding_ok = matches!(map.get("encoding"), Some(Value::String(s)) if s == "bstring");
    let encoded = if encoding_ok {
        byte_string_of(encoded).ok_or_else(|| failed(Failure::EncodedNotAByteString))?
    } else {
        String::new()
    };
    wrapper.insert("encoded".to_owned(), Value::String(encoded));
    let data: EncodedData = serde_json::from_value(Value::Object(wrapper))
        .expect("encoded is a string; the other fields read from any JSON");
    match decode(&data) {
        Ok(text) => Ok(Some(text)),
        Err(DecodeError::Inflate(InflateError::Incomplete)) => Ok(None),
        Err(e) => Err(failed(Failure::Decode(e))),
    }
}

/// `byteStringToArrayBuffer(encoded)` (`encode.ts:28-35`) for whatever
/// `JSON.parse` put in `encoded`, as a byte string of the bytes it yields,
/// or `None` where it throws.
///
/// A string gives the low byte of each UTF-16 code unit. For anything else
/// the buffer is `new ArrayBuffer(value.length)` and the loop calls
/// `value.charCodeAt`, which no non-string has: it throws on `null`
/// (`null.length`), whenever the loop runs (`0 < value.length`), when
/// ToIndex's ToPrimitive throws on the length (an object with an own
/// `toString` key, or an array holding one) and when `ArrayBuffer` rejects
/// the length; otherwise the buffer is empty.
fn byte_string_of(encoded: &Value) -> Option<String> {
    let length = match encoded {
        Value::String(s) => {
            let bytes: Vec<u8> = json::utf16_units(s)
                .iter()
                .map(|u| (u & 0xff) as u8)
                .collect();
            return Some(to_byte_string(&bytes));
        }
        Value::Null => return None,
        // `(5).length`, `true.length`: undefined.
        Value::Bool(_) | Value::Number(_) => f64::NAN,
        Value::Array(items) => items.len() as f64,
        // ToNumber of `value.length`, which throws (TypeError) where
        // ToPrimitive does: an own `toString` key.
        Value::Object(map) => js::to_number(map.get("length")).ok()?,
    };
    // ToIndex(length) throws below -1 (after truncation) and the loop runs
    // above 0; NaN and (-1, 0] leave an empty buffer and no loop.
    (length.is_nan() || (length > -1.0 && length <= 0.0)).then(String::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_string_of_non_strings() {
        let v = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        for (encoded, want) in [
            ("null", None),
            ("5", Some("")),
            ("true", Some("")),
            ("[]", Some("")),
            ("[\"a\"]", None),
            ("{}", Some("")),
            ("{\"length\":0}", Some("")),
            ("{\"length\":-0.9}", Some("")),
            ("{\"length\":-1}", None),
            ("{\"length\":0.5}", None),
            ("{\"length\":\"abc\"}", Some("")),
            ("{\"length\":[[]]}", Some("")),
            ("{\"length\":[1]}", None),
            ("{\"length\":{}}", Some("")),
            ("{\"length\":{\"toString\":1}}", None),
            ("{\"length\":[{\"toString\":1}]}", None),
            ("{\"length\":{\"valueOf\":1}}", Some("")),
        ] {
            assert_eq!(byte_string_of(&v(encoded)).as_deref(), want, "{encoded}");
        }
    }
}
