//! The vendored fractional-indexing package, ported as
//! `excali_core::order_key`.
//!
//! Upstream: `packages/fractional-indexing/src/index.ts` at the pinned commit
//! 438d89861f53d8a90ad566113ecac1b83761098f (vendored from
//! `rocicorp/fractional-indexing`, CC0). The vendored package has no tests of
//! its own; its origin's suite is `src/test.js` of
//! <https://github.com/rocicorp/fractional-indexing> at tag v3.2.0 (fetched
//! 2026-09-28), ported case by case below. The vendored copy differs from
//! v3.2.0 in one rule: `validateOrderKey` also rejects any character outside
//! the digit alphabet (`index.ts:113-114`). That changes the outcome of the
//! suite's base-10 `generateNKeysBetween` cases (see
//! `base_10_n_keys_follow_the_vendored_character_check`).
//!
//! The format-validation cases of `packages/element/tests/fractionalIndex.test.ts:26-42`
//! are at the end.

use excali_core::order_key::{
    compare_js_strings, generate_key_between, generate_key_between_with, generate_n_keys_between,
    generate_n_keys_between_with, validate_order_key, validate_order_key_with, BASE_62_DIGITS,
};
use std::cmp::Ordering;

const BASE_10_DIGITS: &str = "0123456789";
const BASE_95_DIGITS: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

/// `test(a, b, exp)` of the rocicorp suite: the key, or the thrown message.
fn between(a: Option<&str>, b: Option<&str>) -> String {
    generate_key_between(a, b).unwrap_or_else(|e| e.to_string())
}

/// `testN(a, b, n, exp)`: base 10, keys joined by spaces, or the message.
fn n_between_base_10(a: Option<&str>, b: Option<&str>, n: usize) -> String {
    generate_n_keys_between_with(a, b, n, BASE_10_DIGITS)
        .map(|keys| keys.join(" "))
        .unwrap_or_else(|e| e.to_string())
}

/// `testBase95(a, b, exp)`.
fn between_base_95(a: Option<&str>, b: Option<&str>) -> String {
    generate_key_between_with(a, b, BASE_95_DIGITS).unwrap_or_else(|e| e.to_string())
}

#[test]
fn base_62_digits_are_upstreams() {
    // index.ts:5-6
    assert_eq!(
        BASE_62_DIGITS,
        "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
    );
}

#[test]
fn generate_key_between_matches_the_rocicorp_suite() {
    let cases: &[(Option<&str>, Option<&str>, &str)] = &[
        (None, None, "a0"),
        (None, Some("a0"), "Zz"),
        (None, Some("Zz"), "Zy"),
        (Some("a0"), None, "a1"),
        (Some("a1"), None, "a2"),
        (Some("a0"), Some("a1"), "a0V"),
        (Some("a1"), Some("a2"), "a1V"),
        (Some("a0V"), Some("a1"), "a0l"),
        (Some("Zz"), Some("a0"), "ZzV"),
        (Some("Zz"), Some("a1"), "a0"),
        (None, Some("Y00"), "Xzzz"),
        (Some("bzz"), None, "c000"),
        (Some("a0"), Some("a0V"), "a0G"),
        (Some("a0"), Some("a0G"), "a08"),
        (Some("b125"), Some("b129"), "b127"),
        (Some("a0"), Some("a1V"), "a1"),
        (Some("Zz"), Some("a01"), "a0"),
        (None, Some("a0V"), "a0"),
        (None, Some("b999"), "b99"),
        (
            None,
            Some("A00000000000000000000000000"),
            "invalid order key: A00000000000000000000000000",
        ),
        (
            None,
            Some("A000000000000000000000000001"),
            "A000000000000000000000000000V",
        ),
        (
            Some("zzzzzzzzzzzzzzzzzzzzzzzzzzy"),
            None,
            "zzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ),
        (
            Some("zzzzzzzzzzzzzzzzzzzzzzzzzzz"),
            None,
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzV",
        ),
        (Some("a00"), None, "invalid order key: a00"),
        (Some("a00"), Some("a1"), "invalid order key: a00"),
        (Some("0"), Some("1"), "invalid order key head: 0"),
        (Some("a1"), Some("a0"), "a1 >= a0"),
    ];
    for &(a, b, expected) in cases {
        assert_eq!(between(a, b), expected, "{a:?}, {b:?}");
    }
}

#[test]
fn base_10_n_keys_follow_the_vendored_character_check() {
    // The rocicorp suite expects "a0 a1 a2 a3 a4", "a5 ... b04", "Z5 ... Z9"
    // and "a01 ... a19" here. The vendored validateOrderKey rejects the head
    // letters, which are not base-10 digits (index.ts:113-114), so upstream
    // throws on the second key (or on the first bound). The goldens
    // (goldens/fractional-indexing.json) confirm upstream's output.
    assert_eq!(
        n_between_base_10(None, None, 5),
        "invalid order key: a0"
    );
    assert_eq!(
        n_between_base_10(Some("a4"), None, 10),
        "invalid order key: a4"
    );
    assert_eq!(
        n_between_base_10(None, Some("a0"), 5),
        "invalid order key: a0"
    );
    assert_eq!(
        n_between_base_10(Some("a0"), Some("a2"), 20),
        "invalid order key: a0"
    );
    // Neither bound set and at most one key: no validation runs.
    assert_eq!(n_between_base_10(None, None, 1), "a0");
    assert_eq!(n_between_base_10(None, None, 0), "");
}

#[test]
fn generate_n_keys_between_in_base_62_matches_the_suites_shape() {
    // The suite's four testN cases, in upstream's base 62: consecutive
    // integers when one side is open, short keys between two bounds.
    let keys = |a, b, n| generate_n_keys_between(a, b, n).unwrap().join(" ");
    assert_eq!(keys(None, None, 5), "a0 a1 a2 a3 a4");
    assert_eq!(
        keys(Some("a4"), None, 10),
        "a5 a6 a7 a8 a9 aA aB aC aD aE"
    );
    assert_eq!(keys(None, Some("a0"), 5), "Zv Zw Zx Zy Zz");
    assert_eq!(
        keys(Some("a0"), Some("a2"), 20),
        "a04 a08 a0G a0K a0O a0V a0Z a0d a0l a0t a1 a14 a18 a1G a1O a1V a1Z a1d a1l a1t"
    );
    assert_eq!(keys(None, None, 0), "");
}

#[test]
fn generate_key_between_in_base_95_matches_the_rocicorp_suite() {
    let a26 = " ".repeat(26);
    let cases: Vec<(Option<String>, Option<String>, String)> = vec![
        (Some("a00".into()), Some("a01".into()), "a00P".into()),
        (Some("a0/".into()), Some("a00".into()), "a0/P".into()),
        (None, None, "a ".into()),
        (Some("a ".into()), None, "a!".into()),
        (None, Some("a ".into()), "Z~".into()),
        (
            Some("a0 ".into()),
            Some("a0!".into()),
            "invalid order key: a0 ".into(),
        ),
        (None, Some(format!("A{a26}0")), format!("A{a26}(")),
        (Some("a~".into()), None, "b  ".into()),
        (Some("Z~".into()), None, "a ".into()),
        (Some("b   ".into()), None, "invalid order key: b   ".into()),
        (Some("a0".into()), Some("a0V".into()), "a0;".into()),
        (Some("a  1".into()), Some("a  2".into()), "a  1P".into()),
        (
            None,
            Some(format!("A{a26}")),
            format!("invalid order key: A{a26}"),
        ),
    ];
    for (a, b, expected) in cases {
        assert_eq!(
            between_base_95(a.as_deref(), b.as_deref()),
            expected,
            "{a:?}, {b:?}"
        );
    }
}

#[test]
fn errors_carry_upstreams_messages() {
    // index.ts:71, 87, 99, 115, 123, 224, 241, 262
    assert_eq!(
        validate_order_key("").unwrap_err().to_string(),
        "invalid order key head: undefined"
    );
    assert_eq!(
        validate_order_key("a").unwrap_err().to_string(),
        "invalid order key: a"
    );
    assert_eq!(
        validate_order_key("!0").unwrap_err().to_string(),
        "invalid order key: !0"
    );
    assert_eq!(
        validate_order_key_with("!0", BASE_95_DIGITS)
            .unwrap_err()
            .to_string(),
        "invalid order key head: !"
    );
    assert_eq!(
        generate_key_between(Some("a1"), Some("a1"))
            .unwrap_err()
            .to_string(),
        "a1 >= a1"
    );
    // Upstream quirk, kept: decrementing the integer just above the smallest
    // yields "A" + 26 zeros, a key validateOrderKey itself rejects
    // (index.ts:236-243).
    let above_smallest = format!("A{}1", "0".repeat(25));
    let smallest = format!("A{}", "0".repeat(26));
    assert_eq!(
        generate_key_between(None, Some(&above_smallest)).unwrap(),
        smallest
    );
    assert!(validate_order_key(&smallest).is_err());
    let largest = "z".repeat(27);
    let above = format!("{largest}V");
    assert_eq!(
        generate_key_between(Some(&largest), Some(&above)).unwrap(),
        format!("{largest}G")
    );
}

#[test]
fn keys_compare_as_js_strings() {
    // JS `<` compares UTF-16 code units; UTF-8 byte order differs for
    // characters above U+FFFF against U+E000..U+FFFF.
    assert_eq!(compare_js_strings("a0", "a1"), Ordering::Less);
    assert_eq!(compare_js_strings("Zz", "a0"), Ordering::Less);
    assert_eq!(compare_js_strings("a0", "a0V"), Ordering::Less);
    assert_eq!(compare_js_strings("a1", "a1"), Ordering::Equal);
    assert_eq!(compare_js_strings("\u{1F600}", "\u{FFFD}"), Ordering::Less);
    assert_eq!("\u{1F600}".cmp("\u{FFFD}"), Ordering::Greater);
}

// packages/element/tests/fractionalIndex.test.ts:26-42

#[test]
fn should_reject_malformed_base62_order_keys() {
    for key in ["a!", "a_", "a1!", "a1_", "zd0032"] {
        assert!(validate_order_key(key).is_err(), "{key}");
    }
}

#[test]
fn should_accept_valid_base62_order_keys() {
    let long = format!("{:z<28}", "z");
    for key in ["Zz", "a0", "a1", "a1V", long.as_str()] {
        assert!(validate_order_key(key).is_ok(), "{key}");
    }
}
