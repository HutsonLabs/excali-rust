//! `is_label_valid` as ada 4.0.0 has it (`src/validity.cpp`): a label must
//! not start with a combining mark, must pass ContextJ for U+200C and
//! U+200D, and, when it is right-to-left on its own (an R, AL or AN in it),
//! the Bidi rule of RFC 5893 section 2. Its combining-mark and direction
//! tables (in the blob) are Unicode 13; the ContextJ lists are in the
//! source (`VIRAMA`, `JOINING_R`, `JOINING_L`, `JOINING_D` in
//! [`layout`](super::layout)). Ported as written, including the early `true` after a virama
//! before a joiner, which skips the Bidi rule.

use super::layout::{JOINING_D, JOINING_L, JOINING_R, VIRAMA};
use super::tables::{tables, Tables};

// `enum direction` in validity.cpp.
const BN: u8 = 1;
const CS: u8 = 2;
const ES: u8 = 3;
const ON: u8 = 4;
const EN: u8 = 5;
const L: u8 = 6;
const R: u8 = 7;
const NSM: u8 = 8;
const AL: u8 = 9;
const AN: u8 = 10;
const ET: u8 = 11;

/// `find_direction`: the direction of `c` in ada's table, 0 (NONE) where
/// it has none.
fn find_direction(t: &Tables, c: u32) -> u8 {
    let (mut lo, mut hi) = (0, Tables::DIR_TABLE_COUNT);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if t.dir_final(mid) < c {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == Tables::DIR_TABLE_COUNT {
        return 0;
    }
    if c >= t.dir_start(lo) {
        t.dir_value(lo)
    } else {
        0
    }
}

/// Whether `c` is in ada's combining-mark ranges.
fn is_combining_mark(t: &Tables, c: u32) -> bool {
    // lower_bound on the range ends
    let (mut lo, mut hi) = (0, Tables::COMBINING_RANGE_COUNT);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if t.combining_range(mid)[1] < c {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo != Tables::COMBINING_RANGE_COUNT && c >= t.combining_range(lo)[0]
}

/// `is_label_valid`.
pub(super) fn is_label_valid(label: &[u32]) -> bool {
    let Some(&first) = label.first() else {
        return true;
    };
    let t = tables();
    if is_combining_mark(t, first) {
        return false;
    }

    let after_virama = |i: usize| i > 0 && VIRAMA.binary_search(&label[i - 1]).is_ok();
    for (i, &c) in label.iter().enumerate() {
        if c == 0x200C {
            if after_virama(i) {
                return true;
            }
            if i == 0 || i + 1 >= label.len() {
                return false;
            }
            let is_l_or_d =
                |x: &u32| JOINING_L.binary_search(x).is_ok() || JOINING_D.binary_search(x).is_ok();
            let is_r_or_d =
                |x: &u32| JOINING_R.binary_search(x).is_ok() || JOINING_D.binary_search(x).is_ok();
            return label[..i].iter().any(is_l_or_d) && label[i + 1..].iter().any(is_r_or_d);
        } else if c == 0x200D {
            return after_virama(i);
        }
    }

    let Some(last_non_nsm) = label.iter().rposition(|&c| find_direction(t, c) != NSM) else {
        return false;
    };

    let rtl_mask = (1u32 << R) | (1u32 << AL) | (1u32 << AN);
    let directions = label
        .iter()
        .fold(0u32, |m, &c| m | (1u32 << find_direction(t, c)));
    if directions & rtl_mask == 0 {
        return true;
    }

    if find_direction(t, label[0]) == L {
        // Evaluated as left-to-right.
        for &c in &label[..=last_non_nsm] {
            let d = find_direction(t, c);
            if !matches!(d, L | EN | ES | CS | ET | ON | BN | NSM) {
                return false;
            }
        }
        return matches!(find_direction(t, label[last_non_nsm]), L | EN);
    }

    // Evaluated as right-to-left.
    if !matches!(find_direction(t, label[0]), R | AL) {
        return false;
    }
    let (mut has_an, mut has_en) = (false, false);
    for (i, &c) in label[..=last_non_nsm].iter().enumerate() {
        let d = find_direction(t, c);
        if d == EN {
            has_en = true;
            if has_an {
                return false;
            }
        }
        if d == AN {
            has_an = true;
            if has_en {
                return false;
            }
        }
        if !matches!(d, R | AL | AN | EN | ES | CS | ET | ON | BN | NSM) {
            return false;
        }
        if i == last_non_nsm && !matches!(d, R | AL | AN | EN) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(s: &str) -> bool {
        is_label_valid(&s.chars().map(u32::from).collect::<Vec<_>>())
    }

    #[test]
    fn lists_are_sorted_for_binary_search() {
        assert!(VIRAMA.windows(2).all(|w| w[0] < w[1]));
        assert!(JOINING_R.windows(2).all(|w| w[0] <= w[1]));
        assert!(JOINING_D.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn marks_joiners_and_bidi() {
        assert!(!valid("\u{301}a"));
        assert!(valid("\u{1AD3}"));
        assert!(valid("a\u{10D70}"));
        assert!(!valid("a\u{5D0}"));
        assert!(!valid("\u{5D0}\u{1AD3}"));
        assert!(valid("\u{5D0}\u{301}"));
        assert!(valid("\u{628}\u{200C}\u{628}"));
        assert!(!valid("a\u{200C}b"));
        assert!(valid("\u{915}\u{94D}\u{200C}\u{5D0}a"));
        assert!(!valid("a\u{200D}b"));
    }
}
