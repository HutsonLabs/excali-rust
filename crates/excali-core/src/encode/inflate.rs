//! zlib/gzip decompression with pako's `inflate` semantics
//! (`lib/inflate.js`, `lib/zlib/inflate.js`, pako 2.0.3), as upstream's
//! `decode` calls it: `inflate(bytes, { to: "string" })`
//! (`packages/excalidraw/data/encode.ts:138-141`).
//!
//! What pako does with those options, and this module reproduces:
//!
//! - `windowBits` defaults to 15 + 32: the wrapper is detected per stream, a
//!   gzip member (magic `1f 8b`) or a zlib stream. Header and trailer errors
//!   carry pako's messages (`incorrect header check`, `unknown compression
//!   method`, `invalid window size`, `need dictionary`, `unknown header flags
//!   set`, `header crc mismatch`, `incorrect data check`, `incorrect length
//!   check`).
//! - After a stream ends, if more input follows and its next byte is not 0,
//!   pako resets and inflates another stream into the same output
//!   (`Inflate.prototype.push`, "Skip snyc markers if more data follows").
//!   Trailing bytes starting with 0 are ignored.
//! - Input that ends before a stream does is not an error in pako: no result
//!   is produced and `inflate` returns `undefined`. Here that is
//!   [`InflateError::Incomplete`].
//! - With `to: "string"`, output is converted to a string 64 KiB at a time
//!   (`chunkSize`) by pako's own UTF-8 decoder (`lib/utils/strings.js`,
//!   `buf2string`), each chunk cut at `utf8border` so a sequence is not split.
//!   That decoder does not validate continuation bytes, does not strip a
//!   BOM, and drops an incomplete sequence at the very end of the output.
//!   [`inflate_to_string`] reproduces it code unit for code unit; the one
//!   difference is that a lone UTF-16 surrogate (which a JS string can hold
//!   and a Rust string cannot) becomes U+FFFD.
//!
//! The deflate block data itself is decoded by `flate2` (miniz_oxide). Any
//! valid stream decodes to the same bytes in every inflater; for corrupt
//! block data pako reports one of several messages (`invalid distance too
//! far back`, ...), and this module reports [`InflateError::InvalidData`].

use flate2::{Decompress, FlushDecompress, Status};

use super::checksum::{adler32, crc32};

/// Why [`inflate`] produced no output. The `Display` text of the header and
/// trailer variants is pako's `strm.msg`, which upstream's `decode` throws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    /// The input ended before the stream did. pako returns `undefined`.
    Incomplete,
    /// `incorrect header check`: neither a gzip magic nor a valid zlib header.
    IncorrectHeaderCheck,
    /// `unknown compression method`: CM is not 8 (deflate).
    UnknownCompressionMethod,
    /// `invalid window size`: the zlib header asks for more than 32 KiB.
    InvalidWindowSize,
    /// `need dictionary`: the zlib header requires a preset dictionary.
    NeedDictionary,
    /// `unknown header flags set`: reserved gzip FLG bits are set.
    UnknownHeaderFlags,
    /// `header crc mismatch`: the gzip FHCRC does not match.
    HeaderCrcMismatch,
    /// `incorrect data check`: the Adler-32 or CRC-32 trailer does not match.
    IncorrectDataCheck,
    /// `incorrect length check`: the gzip ISIZE trailer does not match.
    IncorrectLengthCheck,
    /// The deflate block data is corrupt.
    InvalidData,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Incomplete => "incomplete compressed data",
            Self::IncorrectHeaderCheck => "incorrect header check",
            Self::UnknownCompressionMethod => "unknown compression method",
            Self::InvalidWindowSize => "invalid window size",
            Self::NeedDictionary => "need dictionary",
            Self::UnknownHeaderFlags => "unknown header flags set",
            Self::HeaderCrcMismatch => "header crc mismatch",
            Self::IncorrectDataCheck => "incorrect data check",
            Self::IncorrectLengthCheck => "incorrect length check",
            Self::InvalidData => "invalid compressed data",
        })
    }
}

impl std::error::Error for InflateError {}

/// A cursor over the input; running out is [`InflateError::Incomplete`].
struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Input<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], InflateError> {
        let end = self.pos.checked_add(n).ok_or(InflateError::Incomplete)?;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or(InflateError::Incomplete)?;
        self.pos = end;
        Ok(bytes)
    }

    fn u16_le(&mut self) -> Result<u16, InflateError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_le(&mut self) -> Result<u32, InflateError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A zero-terminated gzip header field (FNAME, FCOMMENT), terminator
    /// included.
    fn zero_terminated(&mut self) -> Result<&'a [u8], InflateError> {
        let rest = &self.data[self.pos..];
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(InflateError::Incomplete)?;
        self.take(len + 1)
    }
}

// gzip FLG bits, as pako reads them in the high byte of `state.flags`.
const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;
const FRESERVED: u8 = 0xe0;

/// Decompress `data` as `pako.inflate(data)` does: one or more concatenated
/// zlib or gzip streams.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut input = Input { data, pos: 0 };
    let mut out = Vec::new();
    loop {
        inflate_stream(&mut input, &mut out)?;
        match data.get(input.pos) {
            Some(&b) if b != 0 => continue,
            _ => return Ok(out),
        }
    }
}

/// One stream: header (HEAD .. HCRC / DICTID), body, trailer (CHECK, LENGTH)
/// in `lib/zlib/inflate.js`.
fn inflate_stream(input: &mut Input<'_>, out: &mut Vec<u8>) -> Result<(), InflateError> {
    let start = input.pos;
    let hold = input.u16_le()?;
    let gzip = hold == 0x8b1f;
    if gzip {
        let method = input.take(1)?[0];
        let flg = input.take(1)?[0];
        if method != 8 {
            return Err(InflateError::UnknownCompressionMethod);
        }
        if flg & FRESERVED != 0 {
            return Err(InflateError::UnknownHeaderFlags);
        }
        input.take(4 + 2)?; // MTIME, XFL, OS
        if flg & FEXTRA != 0 {
            let xlen = input.u16_le()?;
            input.take(xlen as usize)?;
        }
        if flg & FNAME != 0 {
            input.zero_terminated()?;
        }
        if flg & FCOMMENT != 0 {
            input.zero_terminated()?;
        }
        if flg & FHCRC != 0 {
            let header_crc = crc32(0, &input.data[start..input.pos]);
            if u32::from(input.u16_le()?) != header_crc & 0xffff {
                return Err(InflateError::HeaderCrcMismatch);
            }
        }
    } else {
        let (cmf, flg) = (hold & 0xff, hold >> 8);
        if ((cmf << 8) + flg) % 31 != 0 {
            return Err(InflateError::IncorrectHeaderCheck);
        }
        if cmf & 0x0f != 8 {
            return Err(InflateError::UnknownCompressionMethod);
        }
        if (cmf >> 4) + 8 > 15 {
            return Err(InflateError::InvalidWindowSize);
        }
        if flg & 0x20 != 0 {
            // DICTID, then DICT returns Z_NEED_DICT.
            input.take(4)?;
            return Err(InflateError::NeedDictionary);
        }
    }

    let body_start = out.len();
    inflate_raw(input, out)?;
    let body = &out[body_start..];

    if gzip {
        if input.u32_le()? != crc32(0, body) {
            return Err(InflateError::IncorrectDataCheck);
        }
        if input.u32_le()? != body.len() as u32 {
            return Err(InflateError::IncorrectLengthCheck);
        }
    } else {
        let b = input.take(4)?;
        if u32::from_be_bytes([b[0], b[1], b[2], b[3]]) != adler32(1, body) {
            return Err(InflateError::IncorrectDataCheck);
        }
    }
    Ok(())
}

/// Decode raw deflate blocks from the input position to the end of the last
/// block, appending to `out` and leaving the input just past the blocks.
fn inflate_raw(input: &mut Input<'_>, out: &mut Vec<u8>) -> Result<(), InflateError> {
    let mut z = Decompress::new(false);
    let src = &input.data[input.pos..];
    loop {
        let consumed = z.total_in() as usize;
        if out.capacity() - out.len() < 32 * 1024 {
            out.reserve(out.len().max(64 * 1024));
        }
        let produced = out.len();
        let status = z
            .decompress_vec(&src[consumed..], out, FlushDecompress::None)
            .map_err(|_| InflateError::InvalidData)?;
        match status {
            Status::StreamEnd => break,
            // Output space is always available, so a call that makes no
            // progress has run out of input.
            Status::Ok | Status::BufError => {
                if z.total_in() as usize == consumed && out.len() == produced {
                    return Err(InflateError::Incomplete);
                }
            }
        }
    }
    input.pos += z.total_in() as usize;
    Ok(())
}

/// `_utf8len` in `lib/utils/strings.js`: sequence length by lead byte, with
/// 0xFE marked invalid (1).
fn utf8len(b: u8) -> usize {
    match b {
        254 => 1,
        252.. => 6,
        248.. => 5,
        240.. => 4,
        224.. => 3,
        192.. => 2,
        _ => 1,
    }
}

/// `utf8border(buf, max)`: the largest prefix length of `buf` that does not
/// end inside a sequence.
fn utf8border(buf: &[u8]) -> usize {
    let max = buf.len();
    let mut pos = max as isize - 1;
    while pos >= 0 && buf[pos as usize] & 0xc0 == 0x80 {
        pos -= 1;
    }
    if pos <= 0 {
        return max;
    }
    let pos = pos as usize;
    if pos + utf8len(buf[pos]) > max {
        pos
    } else {
        max
    }
}

/// `buf2string(buf, len)`: pako's UTF-8 decoder, appending UTF-16 code units.
fn buf2string(buf: &[u8], out: &mut Vec<u16>) {
    let len = buf.len();
    let mut i = 0;
    while i < len {
        let mut c = u32::from(buf[i]);
        i += 1;
        if c < 0x80 {
            out.push(c as u16);
            continue;
        }
        let mut c_len = utf8len(c as u8);
        if c_len > 4 {
            out.push(0xfffd);
            i += c_len - 1;
            continue;
        }
        c &= match c_len {
            2 => 0x1f,
            3 => 0x0f,
            _ => 0x07,
        };
        while c_len > 1 && i < len {
            c = (c << 6) | (u32::from(buf[i]) & 0x3f);
            i += 1;
            c_len -= 1;
        }
        if c_len > 1 {
            out.push(0xfffd);
            continue;
        }
        if c < 0x10000 {
            out.push(c as u16);
        } else {
            let c = c - 0x10000;
            out.push((0xd800 | ((c >> 10) & 0x3ff)) as u16);
            out.push((0xdc00 | (c & 0x3ff)) as u16);
        }
    }
}

/// pako's `Inflate` output buffer size (`chunkSize`).
const CHUNK_SIZE: usize = 64 * 1024;

/// `pako.inflate(data, { to: "string" })` as UTF-16 code units: the output
/// is decoded one 64 KiB buffer at a time, each cut at `utf8border` and the
/// cut-off tail carried into the next buffer. At the end of the stream the
/// last buffer is cut the same way and its tail is dropped.
fn decode_chunks(bytes: &[u8]) -> Vec<u16> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut pos = 0;
    while pos < bytes.len() {
        let end = (pos + CHUNK_SIZE).min(bytes.len());
        let n = utf8border(&bytes[pos..end]);
        buf2string(&bytes[pos..pos + n], &mut units);
        if end == bytes.len() {
            break;
        }
        pos += n;
    }
    units
}

/// Decompress and decode as `pako.inflate(data, { to: "string" })`; see the
/// module documentation for pako's decoder quirks.
pub fn inflate_to_string(data: &[u8]) -> Result<String, InflateError> {
    let bytes = inflate(data)?;
    Ok(String::from_utf16_lossy(&decode_chunks(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8border_cases() {
        assert_eq!(utf8border(b"abc"), 3);
        assert_eq!(utf8border(&[b'a', 0xe2, 0x82]), 1);
        assert_eq!(utf8border(&[b'a', 0xe2, 0x82, 0xac]), 4);
        // Only continuation bytes, or a lead byte at 0: the whole buffer.
        assert_eq!(utf8border(&[0x80, 0x80]), 2);
        assert_eq!(utf8border(&[0xe2, 0x82]), 2);
    }

    #[test]
    fn buf2string_cases() {
        let mut out = Vec::new();
        buf2string("é😀".as_bytes(), &mut out);
        assert_eq!(out, "é😀".encode_utf16().collect::<Vec<_>>());
        out.clear();
        // Continuation bytes are masked, not validated.
        buf2string(&[0xc3, 0x28], &mut out);
        assert_eq!(out, [0xe8]);
        out.clear();
        // Truncated at the end of the buffer.
        buf2string(&[0xe2, 0x82], &mut out);
        assert_eq!(out, [0xfffd]);
    }

    #[test]
    fn final_chunk_drops_an_incomplete_sequence() {
        assert_eq!(decode_chunks(&[b'a', b'b', 0xe2, 0x82]), [0x61, 0x62]);
        assert_eq!(decode_chunks(&[0xe2, 0x82]), [0xfffd]);
        assert!(decode_chunks(&[]).is_empty());
    }
}
