//! `ada::idna::to_ascii` from ada 4.0.0, the IDNA of `new URL` in Node 26:
//! how a special URL's host outside ASCII becomes its ASCII hostname.
//!
//! The URL Standard's "domain to ASCII" is UTS 46 ToASCII, and the `url`
//! crate does that with ICU4X's Unicode 17 data. ada differs from it, and
//! upstream's `new URL` is ada, so this is a port of ada's own code
//! (`src/to_ascii.cpp`, `src/mapping.cpp`, `src/normalization.cpp`,
//! `src/punycode.cpp`, `src/validity.cpp` in the ada.cpp amalgamation)
//! over ada's own tables (`tables.zlib`, `layout.rs`; see
//! `scripts/fixtures/ada-idna-tables.py`). What that changes:
//!
//! - ada's mapping and normalization tables are IDNA/Unicode 17, but its
//!   combining-mark and bidi direction tables are Unicode 13. A label may
//!   start with a mark added later (`https://\u{1AD3}/` is `xn--trf`), a
//!   right-to-left letter added later may follow `a` (`https://a\u{10D50}/`
//!   is `xn--a-ho6i`), and such a mark is not an NSM, so `\u{5D0}\u{1AD3}`
//!   is not a valid right-to-left label.
//! - The Bidi rule applies to each label that is right-to-left on its own,
//!   not to every label of a domain with one (`https://1.\u{5D0}/` is
//!   `1.xn--4db`).
//! - ContextJ returns early: a joiner after a virama makes the label valid
//!   without the Bidi rule.
//! - ASCII labels are not checked, and an `xn--` label is decoded, mapped
//!   and normalized to check it is stable, then validated.
//! - Its NFC leaves a Hangul LV syllable and a following trailing jamo apart
//!   (see [`normalization`]).
//!
//! ada (https://github.com/ada-url/ada) is Apache-2.0 OR MIT; this port
//! and its tables are used under MIT, whose notice is in `LICENSE-MIT`
//! beside this file.

#[rustfmt::skip]
#[allow(dead_code)] // generated: the whole layout, some of it (id tables) unused
mod layout;
mod normalization;
mod punycode;
mod tables;
mod validity;

use layout::{
    IDNA_BLOCK_BITS, IDNA_BLOCK_MASK, IDNA_BLOCK_SIZE, IDNA_BOOL_FLAG, IDNA_DISALLOWED,
    IDNA_HIGH_IGNORED_END, IDNA_HIGH_IGNORED_START, IDNA_LOW_RANGE_END, IDNA_VALID,
};
use normalization::{is_already_nfc, normalize};
use punycode::{punycode_to_utf32, utf32_to_punycode};
use tables::{tables, Tables};
use validity::is_label_valid;

/// `max_domain_input_bytes`: longer input is refused.
const MAX_DOMAIN_INPUT_BYTES: usize = 16384;

/// `idna_lookup`: `IDNA_VALID`, `IDNA_DISALLOWED`, or an offset into the
/// UTF-8 mappings (0, the empty one, for an ignored code point).
fn idna_lookup(t: &Tables, cp: u32) -> u32 {
    if cp < IDNA_LOW_RANGE_END {
        let entry = u32::from(t.idna_stage1((cp >> IDNA_BLOCK_BITS) as usize));
        if entry & IDNA_BOOL_FLAG != 0 {
            let bit = (entry & !IDNA_BOOL_FLAG) * IDNA_BLOCK_SIZE + (cp & IDNA_BLOCK_MASK);
            let valid = (t.idna_bool_block((bit >> 6) as usize) >> (bit & 63)) & 1 != 0;
            return if valid { IDNA_VALID } else { IDNA_DISALLOWED };
        }
        return u32::from(t.idna_stage2((entry + (cp & IDNA_BLOCK_MASK)) as usize));
    }
    if (IDNA_HIGH_IGNORED_START..IDNA_HIGH_IGNORED_END).contains(&cp) {
        return 0;
    }
    IDNA_DISALLOWED
}

/// The code points of a NUL-terminated UTF-8 mapping (`utf8_next`).
fn push_mapping(bytes: &[u8], out: &mut Vec<u32>) {
    let mut i = 0;
    while bytes[i] != 0 {
        let b0 = u32::from(bytes[i]);
        let cont = |k: usize| u32::from(bytes[i + k]) & 0x3F;
        let (cp, len) = if b0 < 0x80 {
            (b0, 1)
        } else if b0 < 0xE0 {
            ((b0 & 0x1F) << 6 | cont(1), 2)
        } else if b0 < 0xF0 {
            ((b0 & 0x0F) << 12 | cont(1) << 6 | cont(2), 3)
        } else {
            (
                (b0 & 0x07) << 18 | cont(1) << 12 | cont(2) << 6 | cont(3),
                4,
            )
        };
        out.push(cp);
        i += len;
    }
}

/// `map`: UTS 46 mapping by ada's table; `None` for a disallowed code point.
fn map(input: &[u32]) -> Option<Vec<u32>> {
    let t = tables();
    let mut out = Vec::with_capacity(input.len());
    for &x in input {
        let status = idna_lookup(t, x);
        if status == IDNA_DISALLOWED {
            return None;
        }
        if status == IDNA_VALID {
            out.push(x);
            continue;
        }
        if status as usize >= Tables::IDNA_UTF8_MAPPINGS_SIZE {
            return None;
        }
        push_mapping(t.idna_utf8_mapping(status as usize), &mut out);
    }
    Some(out)
}

fn is_ascii(s: &[u32]) -> bool {
    s.iter().all(|&c| c < 0x80)
}

/// `ada::idna::to_ascii`: the ASCII form of a domain (already
/// percent-decoded), `None` where ada returns false. An ASCII domain is
/// lower-cased and nothing else.
pub(crate) fn to_ascii(domain: &str) -> Option<String> {
    if domain.len() > MAX_DOMAIN_INPUT_BYTES {
        return None;
    }
    if domain.is_ascii() {
        return Some(domain.to_ascii_lowercase());
    }
    let working: Vec<u32> = domain.chars().map(u32::from).collect();
    let mut mapped = map(&working)?;
    if !is_ascii(&mapped) && !is_already_nfc(&mapped) {
        normalize(&mut mapped);
    }

    let mut out = String::with_capacity(mapped.len() + 8);
    // Labels by a single scan, as ada walks them: a dot ending `mapped`
    // is written, and the empty piece after it is not visited.
    let mut p = 0;
    while p < mapped.len() {
        let begin = p;
        while p < mapped.len() && mapped[p] != u32::from(b'.') {
            p += 1;
        }
        let label = &mapped[begin..p];
        let is_last = p == mapped.len();
        if !is_last {
            p += 1;
        }
        if label.is_empty() {
            // empty label
        } else if label.len() >= 4 && label[..4] == [0x78, 0x6E, 0x2D, 0x2D] {
            if !is_ascii(label) {
                return None;
            }
            let ascii: Vec<u8> = label.iter().map(|&c| c as u8).collect();
            out.push_str(std::str::from_utf8(&ascii).ok()?);
            let decoded = punycode_to_utf32(&ascii[4..])?;
            if is_ascii(&decoded) {
                return None;
            }
            let mut post_map = map(&decoded)?;
            if post_map != decoded {
                return None;
            }
            if !is_ascii(&post_map) && !is_already_nfc(&post_map) {
                normalize(&mut post_map);
                if post_map != decoded {
                    return None;
                }
            }
            if post_map.is_empty() || !is_label_valid(&post_map) {
                return None;
            }
        } else if is_ascii(label) {
            out.extend(label.iter().map(|&c| char::from(c as u8)));
        } else {
            if !is_label_valid(label) {
                return None;
            }
            out.push_str("xn--");
            if !utf32_to_punycode(label, &mut out) {
                return None;
            }
        }
        if !is_last {
            out.push('.');
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::to_ascii;

    fn ascii(domain: &str) -> Option<String> {
        to_ascii(domain)
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_owned())
    }

    // `url.domainToASCII` in Node 26.10 where it succeeds (it is ada's
    // to_ascii with the URL host checks after it).
    #[test]
    fn to_ascii_as_ada() {
        assert_eq!(ascii("ExAmple.COM"), some("example.com"));
        assert_eq!(ascii("\u{e9}"), some("xn--9ca"));
        assert_eq!(ascii("\u{1AD3}"), some("xn--trf"));
        assert_eq!(ascii("a\u{10D50}"), some("xn--a-ho6i"));
        assert_eq!(ascii("\u{5D0}\u{1AD3}"), None);
        assert_eq!(ascii("1.\u{5D0}"), some("1.xn--4db"));
        assert_eq!(ascii("\u{e9}."), some("xn--9ca."));
        assert_eq!(ascii(".\u{e9}"), some(".xn--9ca"));
        assert_eq!(ascii("\u{e9}..a"), some("xn--9ca..a"));
        assert_eq!(ascii("xn--e-ufa.\u{e9}"), some("xn--e-ufa.xn--9ca"));
        assert_eq!(ascii("xn--zz.\u{e9}"), None);
        assert_eq!(ascii("xn--.\u{e9}"), None);
        assert_eq!(ascii("\u{FFFD}"), None);
        // Everything ignored: an empty result, which the URL parser rejects.
        assert_eq!(ascii("\u{ad}"), some(""));
        // Longer than max_domain_input_bytes.
        assert_eq!(ascii(&"\u{e9}".repeat(8193)), None);
        assert_eq!(ascii(&"\u{e9}".repeat(8192)).map(|s| s.len()), Some(8198));
    }
}
