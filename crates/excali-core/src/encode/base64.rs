//! Base64: the browser's `btoa` / `atob` and `encode.ts`'s wrappers
//! (`packages/excalidraw/data/encode.ts:49-58`).
//!
//! `btoa` and `atob` work on byte strings (one char per byte, see
//! [`super::to_byte_string`]). `atob` is the WHATWG *forgiving-base64
//! decode*: ASCII whitespace is removed, padding is optional, and leftover
//! bits need not be zero.

use super::bstring::{byte_string_to_string, to_byte_string};

/// The `InvalidCharacterError` `DOMException` `btoa` and `atob` throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCharacterError;

impl std::fmt::Display for InvalidCharacterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InvalidCharacterError")
    }
}

impl std::error::Error for InvalidCharacterError {}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 of `bytes`, padded.
fn encode_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn sextet(c: char) -> Option<u32> {
    Some(match c {
        'A'..='Z' => c as u32 - 'A' as u32,
        'a'..='z' => c as u32 - 'a' as u32 + 26,
        '0'..='9' => c as u32 - '0' as u32 + 52,
        '+' => 62,
        '/' => 63,
        _ => return None,
    })
}

/// `btoa(data)`: base64 of a byte string. A char above U+00FF is an
/// [`InvalidCharacterError`].
pub fn btoa(data: &str) -> Result<String, InvalidCharacterError> {
    let bytes = data
        .chars()
        .map(|c| u8::try_from(u32::from(c)).map_err(|_| InvalidCharacterError))
        .collect::<Result<Vec<u8>, _>>()?;
    Ok(encode_bytes(&bytes))
}

/// `atob(data)`: the byte string base64 `data` holds, by the WHATWG
/// forgiving-base64 decode.
pub fn atob(data: &str) -> Result<String, InvalidCharacterError> {
    // 1. Remove ASCII whitespace (TAB, LF, FF, CR, SPACE).
    let mut chars: Vec<char> = data
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' '))
        .collect();
    // 2. If the length divides by 4, drop one or two trailing `=`.
    if chars.len().is_multiple_of(4) {
        for _ in 0..2 {
            if chars.last() == Some(&'=') {
                chars.pop();
            }
        }
    }
    // 3. A remainder of 1 cannot be decoded.
    if chars.len() % 4 == 1 {
        return Err(InvalidCharacterError);
    }
    // 4. Every remaining char must be in the alphabet (no `=`).
    let mut bytes = Vec::with_capacity(chars.len() / 4 * 3 + 2);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for c in chars {
        buffer = buffer << 6 | sextet(c).ok_or(InvalidCharacterError)?;
        bits += 6;
        if bits == 24 {
            bytes.extend_from_slice(&[(buffer >> 16) as u8, (buffer >> 8) as u8, buffer as u8]);
            buffer = 0;
            bits = 0;
        }
    }
    // 5. Leftover 12 or 18 bits give one or two bytes; the rest is dropped.
    match bits {
        12 => bytes.push((buffer >> 4) as u8),
        18 => bytes.extend_from_slice(&[(buffer >> 10) as u8, (buffer >> 2) as u8]),
        _ => {}
    }
    Ok(to_byte_string(&bytes))
}

/// `stringToBase64(str, isByteString)` (`encode.ts:49-51`): `btoa` of the
/// byte string, or of the UTF-8 bytes of `str` when it is not one.
pub fn string_to_base64(s: &str, is_byte_string: bool) -> Result<String, InvalidCharacterError> {
    if is_byte_string {
        btoa(s)
    } else {
        Ok(encode_bytes(s.as_bytes()))
    }
}

/// `base64ToString(base64, isByteString)` (`encode.ts:54-58`): the byte
/// string `atob` gives, or those bytes decoded as UTF-8 by `TextDecoder`
/// when the caller wants text.
pub fn base64_to_string(
    base64: &str,
    is_byte_string: bool,
) -> Result<String, InvalidCharacterError> {
    let bytes = atob(base64)?;
    Ok(if is_byte_string {
        bytes
    } else {
        byte_string_to_string(&bytes)
    })
}
