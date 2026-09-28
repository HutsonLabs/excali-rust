//! The CSS `unicode-range` descriptor of a font face.
//!
//! Upstream registers each range-split font file with a `unicodeRange`
//! descriptor (`packages/excalidraw/fonts/*/index.ts`, passed to the
//! `FontFace` constructor in `fonts/ExcalidrawFontFace.ts`), and the browser
//! only uses a face for the code points its range lists
//! (CSS Fonts 4, "unicode-range": <https://www.w3.org/TR/css-fonts-4/#unicode-range-desc>).
//! A face without the descriptor covers U+0-10FFFF.

use std::fmt;

/// The highest Unicode code point.
pub const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// A set of code points: sorted, non-overlapping, inclusive ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnicodeRange {
    ranges: Vec<(u32, u32)>,
}

/// Why a `unicode-range` value is not valid CSS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnicodeRangeError {
    /// The offending comma-separated part, trimmed.
    pub part: String,
}

impl fmt::Display for UnicodeRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid unicode-range part {:?}", self.part)
    }
}

impl std::error::Error for UnicodeRangeError {}

impl UnicodeRange {
    /// U+0-10FFFF, the initial value of the descriptor.
    pub fn all() -> UnicodeRange {
        UnicodeRange {
            ranges: vec![(0, MAX_CODE_POINT)],
        }
    }

    /// Parse a descriptor value: comma-separated `<urange>`s, each
    /// `U+<hex>`, `U+<hex>-<hex>` or `U+<hex>?…` (a `?` stands for any hex
    /// digit), one to six digits, case-insensitive, whitespace around the
    /// commas. As the `FontFace` constructor does, any invalid part
    /// (a start above U+10FFFF, an end before its start, bad syntax)
    /// rejects the whole value; an end above U+10FFFF is clamped.
    pub fn parse(value: &str) -> Result<UnicodeRange, UnicodeRangeError> {
        let mut ranges = Vec::new();
        for part in value.split(',') {
            let part = part.trim_matches(|c: char| c.is_ascii_whitespace());
            let range = parse_urange(part).ok_or_else(|| UnicodeRangeError {
                part: part.to_owned(),
            })?;
            ranges.push(range);
        }
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            match merged.last_mut() {
                Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        Ok(UnicodeRange { ranges: merged })
    }

    /// Whether the set holds `code_point`.
    pub fn contains(&self, code_point: u32) -> bool {
        let i = self.ranges.partition_point(|&(_, end)| end < code_point);
        self.ranges
            .get(i)
            .is_some_and(|&(start, _)| start <= code_point)
    }

    /// The inclusive ranges, sorted and merged.
    pub fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }
}

fn hex(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

fn parse_urange(part: &str) -> Option<(u32, u32)> {
    let rest = part
        .strip_prefix("U+")
        .or_else(|| part.strip_prefix("u+"))?;
    let (start, end) = if let Some((a, b)) = rest.split_once('-') {
        (hex(a)?, hex(b)?)
    } else if rest.contains('?') {
        let digits = rest.trim_end_matches('?');
        let wild = rest.len() - digits.len();
        if rest.len() > 6 || digits.contains('?') {
            return None;
        }
        let base = if digits.is_empty() { 0 } else { hex(digits)? };
        let shift = 4 * u32::try_from(wild).ok()?;
        (base << shift, ((base + 1) << shift) - 1)
    } else {
        let v = hex(rest)?;
        (v, v)
    };
    if start > MAX_CODE_POINT || end < start {
        return None;
    }
    Some((start, end.min(MAX_CODE_POINT)))
}
