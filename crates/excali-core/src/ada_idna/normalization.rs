//! NFC as ada 4.0.0 does it (`src/normalization.cpp`): `is_already_nfc`,
//! then decomposition, canonical ordering and composition over its tables.
//! Ported as written, quirks included: `is_already_nfc` and `compose` only
//! join a Hangul LVT syllable with a trailing jamo T when the syllable
//! already has a T (`(s - SBASE) % TCOUNT != 0`), so an LV syllable and a
//! following T stay apart (`U+AC00 U+11A8`), where UAX #15 composes them.

use super::tables::{tables, Tables};

const SBASE: u32 = 0xAC00;
const TBASE: u32 = 0x11A7;
const VBASE: u32 = 0x1161;
const LBASE: u32 = 0x1100;
const LCOUNT: u32 = 19;
const VCOUNT: u32 = 21;
const TCOUNT: u32 = 28;
const NCOUNT: u32 = VCOUNT * TCOUNT;
const SCOUNT: u32 = LCOUNT * VCOUNT * TCOUNT;

fn is_s(c: u32) -> bool {
    (SBASE..SBASE + SCOUNT).contains(&c)
}

fn is_l(c: u32) -> bool {
    (LBASE..LBASE + LCOUNT).contains(&c)
}

fn is_v(c: u32) -> bool {
    (VBASE..VBASE + VCOUNT).contains(&c)
}

/// A trailing jamo that joins a syllable: `TBASE < c < TBASE + TCOUNT`.
fn is_t(c: u32) -> bool {
    c > TBASE && c < TBASE + TCOUNT
}

/// The length of `c`'s canonical decomposition in ada's table, 0 for none
/// or a compatibility one.
fn table_decomposition_length(t: &Tables, c: u32) -> usize {
    let d = t.decomposition(c);
    let length = usize::from(d[1] >> 2) - usize::from(d[0] >> 2);
    if length > 0 && d[0] & 1 != 0 {
        0
    } else {
        length
    }
}

/// `canonical_decomp_length`: 0 for a Hangul syllable.
fn canonical_decomp_length(t: &Tables, c: u32) -> usize {
    if is_s(c) || c >= 0x11_0000 {
        return 0;
    }
    table_decomposition_length(t, c)
}

/// `compute_decomposition_length`: whether anything decomposes, and how
/// many code points decomposition adds.
fn compute_decomposition_length(t: &Tables, input: &[u32]) -> (bool, usize) {
    let mut needed = false;
    let mut additional = 0;
    for &c in input {
        let length = if is_s(c) {
            if !(c - SBASE).is_multiple_of(TCOUNT) {
                3
            } else {
                2
            }
        } else if c < 0x11_0000 {
            table_decomposition_length(t, c)
        } else {
            0
        };
        if length != 0 {
            needed = true;
            additional += length - 1;
        }
    }
    (needed, additional)
}

/// `decompose`: in place, from the end.
fn decompose(t: &Tables, input: &mut Vec<u32>, additional: usize) {
    let original = input.len();
    input.resize(original + additional, 0);
    let mut out = input.len();
    for i in (0..original).rev() {
        let c = input[i];
        if is_s(c) {
            let s = c - SBASE;
            if !s.is_multiple_of(TCOUNT) {
                out -= 1;
                input[out] = TBASE + s % TCOUNT;
            }
            out -= 1;
            input[out] = VBASE + (s % NCOUNT) / TCOUNT;
            out -= 1;
            input[out] = LBASE + s / NCOUNT;
        } else if c < 0x11_0000 {
            let d = t.decomposition(c);
            let mut length = table_decomposition_length(t, c);
            if length > 0 {
                let base = usize::from(d[0] >> 2);
                if base + length > Tables::DECOMPOSITION_DATA_SIZE {
                    out -= 1;
                    input[out] = c;
                } else {
                    while length > 0 {
                        length -= 1;
                        out -= 1;
                        input[out] = t.decomposition_data(base + length);
                    }
                }
            } else {
                out -= 1;
                input[out] = c;
            }
        } else {
            out -= 1;
            input[out] = c;
        }
    }
}

/// `sort_marks`: canonical ordering by insertion.
fn sort_marks(t: &Tables, input: &mut [u32]) {
    for idx in 1..input.len() {
        let ccc = t.ccc(input[idx]);
        if ccc == 0 {
            continue;
        }
        let current = input[idx];
        let mut back = idx;
        while back != 0 && t.ccc(input[back - 1]) > ccc {
            input[back] = input[back - 1];
            back -= 1;
        }
        input[back] = current;
    }
}

/// The binary search of `compose` and `would_compose` for `next` among the
/// pairs of `composition_data[left..right]`: the index of the match,
/// `Err(())` where ada breaks out of its loop (a corrupt entry), `Ok(None)`
/// for no match.
fn find_composition(t: &Tables, entry: [u16; 2], next: u32) -> Result<Option<usize>, ()> {
    let mut left = i32::from(entry[0]);
    let mut right = i32::from(entry[1]);
    if left < 0 || right < left || right as usize > Tables::COMPOSITION_DATA_SIZE {
        return Err(());
    }
    while left + 2 < right {
        let middle = left + (((right - left) >> 1) & !1);
        if t.composition_data(middle as usize) <= next {
            left = middle;
        }
        if t.composition_data(middle as usize) >= next {
            right = middle;
        }
    }
    if ((left + 1) as usize) < Tables::COMPOSITION_DATA_SIZE
        && t.composition_data(left as usize) == next
    {
        Ok(Some(left as usize))
    } else {
        Ok(None)
    }
}

/// `would_compose`: whether composition would change `input`.
fn would_compose(t: &Tables, input: &[u32]) -> bool {
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        if is_l(c) {
            if i + 1 < input.len() && is_v(input[i + 1]) {
                return true;
            }
            i += 1;
            continue;
        }
        if is_s(c) {
            if !(c - SBASE).is_multiple_of(TCOUNT) && i + 1 < input.len() && is_t(input[i + 1]) {
                return true;
            }
            i += 1;
            continue;
        }
        if c < 0x11_0000 {
            let entry = t.composition(c);
            let mut previous_ccc: i32 = -1;
            let mut j = i;
            while j + 1 < input.len() {
                let ccc = t.ccc(input[j + 1]);
                if entry[1] != entry[0] && previous_ccc < i32::from(ccc) {
                    match find_composition(t, entry, input[j + 1]) {
                        Err(()) => break,
                        Ok(Some(_)) => return true,
                        Ok(None) => {}
                    }
                }
                if ccc == 0 {
                    break;
                }
                previous_ccc = i32::from(ccc);
                j += 1;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// `is_already_nfc`: no singleton decomposition, marks in canonical order,
/// and nothing that would compose.
pub(super) fn is_already_nfc(input: &[u32]) -> bool {
    if input.is_empty() {
        return true;
    }
    let t = tables();
    if input.iter().any(|&c| canonical_decomp_length(t, c) == 1) {
        return false;
    }
    let mut previous = 0;
    for &c in input {
        let ccc = t.ccc(c);
        if ccc != 0 && previous > ccc {
            return false;
        }
        previous = ccc;
    }
    !would_compose(t, input)
}

/// `compose`, in place.
fn compose(t: &Tables, input: &mut Vec<u32>) {
    let mut i = 0;
    let mut w = 0;
    while i < input.len() {
        input[w] = input[i];
        let c = input[i];
        if is_l(c) {
            if i + 1 < input.len() && is_v(input[i + 1]) {
                input[w] = SBASE + ((c - LBASE) * VCOUNT + input[i + 1] - VBASE) * TCOUNT;
                i += 1;
                if i + 1 < input.len() && is_t(input[i + 1]) {
                    i += 1;
                    input[w] += input[i] - TBASE;
                }
            }
        } else if is_s(c) {
            if !(c - SBASE).is_multiple_of(TCOUNT) && i + 1 < input.len() && is_t(input[i + 1]) {
                i += 1;
                input[w] += input[i] - TBASE;
            }
        } else if c < 0x11_0000 {
            let mut entry = t.composition(c);
            let starter = w;
            let mut previous_ccc: i32 = -1;
            while i + 1 < input.len() {
                let next = input[i + 1];
                let ccc = t.ccc(next);
                if entry[1] != entry[0] && previous_ccc < i32::from(ccc) {
                    match find_composition(t, entry, next) {
                        Err(()) => break,
                        Ok(Some(at)) => {
                            let composed = t.composition_data(at + 1);
                            input[starter] = composed;
                            entry = t.composition(composed);
                            i += 1;
                            continue;
                        }
                        Ok(None) => {}
                    }
                }
                if ccc == 0 {
                    break;
                }
                previous_ccc = i32::from(ccc);
                w += 1;
                input[w] = next;
                i += 1;
            }
        }
        i += 1;
        w += 1;
    }
    if w < i {
        input.truncate(w);
    }
}

/// `normalize`: NFC unless `is_already_nfc` says it is.
pub(super) fn normalize(input: &mut Vec<u32>) {
    if is_already_nfc(input) {
        return;
    }
    let t = tables();
    let (needed, additional) = compute_decomposition_length(t, input);
    if needed {
        decompose(t, input, additional);
    }
    sort_marks(t, input);
    compose(t, input);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nfc(s: &str) -> String {
        let mut v: Vec<u32> = s.chars().map(u32::from).collect();
        normalize(&mut v);
        v.into_iter().map(|c| char::from_u32(c).unwrap()).collect()
    }

    #[test]
    fn composes_and_orders() {
        assert_eq!(nfc("e\u{301}"), "\u{e9}");
        assert_eq!(nfc("a\u{302}\u{323}"), "\u{1EAD}");
        assert_eq!(nfc("a\u{323}\u{302}"), "\u{1EAD}");
        assert_eq!(nfc("\u{1100}\u{1161}\u{11A8}"), "\u{AC01}");
        assert_eq!(nfc("\u{AC01}"), "\u{AC01}");
        // As ada's normalize leaves it (to_ascii maps U+212B to U+E5 first).
        assert_eq!(nfc("\u{212B}"), "\u{212B}");
        assert_eq!(nfc("A\u{30A}"), "\u{C5}");
        assert!(is_already_nfc(&[0xE9]));
        assert!(!is_already_nfc(&[0x65, 0x301]));
    }

    /// ada's quirk: an LV syllable and a trailing T are left apart.
    #[test]
    fn lv_and_t_stay_apart() {
        assert!(is_already_nfc(&[0xAC00, 0x11A8]));
        assert_eq!(nfc("\u{AC00}\u{11A8}"), "\u{AC00}\u{11A8}");
    }
}
