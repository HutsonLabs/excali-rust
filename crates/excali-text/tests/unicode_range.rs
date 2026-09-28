//! The CSS `unicode-range` descriptor (CSS Fonts 4, "unicode-range"):
//! the syntax upstream's face descriptors use, wildcards, and the values
//! the `FontFace` constructor rejects.

use excali_text::font_metadata::GOOGLE_FONTS_RANGES;
use excali_text::unicode_range::{UnicodeRange, MAX_CODE_POINT};

fn ranges(value: &str) -> Vec<(u32, u32)> {
    UnicodeRange::parse(value).unwrap().ranges().to_vec()
}

#[test]
fn single_code_points_and_ranges() {
    assert_eq!(ranges("U+3bb"), vec![(0x3BB, 0x3BB)]);
    assert_eq!(ranges("U+20-7e"), vec![(0x20, 0x7E)]);
    assert_eq!(ranges("u+0000-00FF"), vec![(0, 0xFF)]);
    assert_eq!(
        ranges("U+d7eb-d7fb,U+f900-f9b7"),
        vec![(0xD7EB, 0xD7FB), (0xF900, 0xF9B7)]
    );
}

#[test]
fn whitespace_sorting_and_merging() {
    assert_eq!(
        ranges("U+0301, U+0400-045F,U+0300 , U+0490-0491"),
        vec![(0x300, 0x301), (0x400, 0x45F), (0x490, 0x491)]
    );
    assert_eq!(ranges("U+10-20, U+15-30, U+31"), vec![(0x10, 0x31)]);
}

#[test]
fn wildcards() {
    assert_eq!(ranges("U+4??"), vec![(0x400, 0x4FF)]);
    assert_eq!(ranges("U+??????"), vec![(0, MAX_CODE_POINT)]);
    assert_eq!(ranges("U+1F6??"), vec![(0x1F600, 0x1F6FF)]);
}

#[test]
fn end_above_max_is_clamped() {
    assert_eq!(ranges("U+10FF00-1FFFFF"), vec![(0x10FF00, MAX_CODE_POINT)]);
}

#[test]
fn invalid_values_are_rejected() {
    for bad in [
        "",
        "20-7e",
        "U+",
        "U+1234567",
        "U+7e-20",
        "U+110000",
        "U+4?4",
        "U+12g",
        "U+20,,U+30",
        "U+1-",
    ] {
        let err = UnicodeRange::parse(bad).unwrap_err();
        assert!(!err.to_string().is_empty(), "{bad:?}");
    }
}

#[test]
fn contains() {
    let r = UnicodeRange::parse(GOOGLE_FONTS_RANGES.latin).unwrap();
    for cp in [0, 0x41, 0xFF, 0x131, 0x2000, 0x206F, 0xFFFD] {
        assert!(r.contains(cp), "{cp:#x}");
    }
    for cp in [0x100, 0x130, 0x1FFF, 0x2070, 0xFFFE, 0x10FFFF] {
        assert!(!r.contains(cp), "{cp:#x}");
    }
    let all = UnicodeRange::all();
    assert!(all.contains(0) && all.contains(MAX_CODE_POINT));
    assert_eq!(all.ranges(), &[(0, MAX_CODE_POINT)]);
}

#[test]
fn every_google_fonts_range_parses() {
    for value in [
        GOOGLE_FONTS_RANGES.latin,
        GOOGLE_FONTS_RANGES.latin_ext,
        GOOGLE_FONTS_RANGES.cyrilic_ext,
        GOOGLE_FONTS_RANGES.cyrilic,
        GOOGLE_FONTS_RANGES.vietnamese,
    ] {
        assert!(UnicodeRange::parse(value).is_ok(), "{value}");
    }
}
