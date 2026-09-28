//! Port of `packages/element/tests/fractionalIndex.test.ts:44-877` at the
//! pinned commit 438d89861f53d8a90ad566113ecac1b83761098f: every
//! `testMovedIndicesSync` / `testInvalidIndicesSync` scenario, with the
//! same checks as upstream's `test()` helper (`:825-876`):
//!
//! - the input is invalid (unless the scenario says it is valid);
//! - after the sync the length and order are unchanged and every index is
//!   valid, including the bound text check;
//! - elements listed as unchanged keep their index and version; every other
//!   element gets a different index and exactly one version bump, even when
//!   the moved sync falls back to the full sync.
//!
//! The format-validation cases (`:26-42`) are in `tests/order_key.rs`.
//! Exact key strings are pinned by `tests/fractional_index_goldens.rs`.

use excali_core::element::{Element, ElementBase, ElementKind, FractionalIndex};
use excali_core::fractional_index::{
    sync_invalid_indices, sync_moved_indices, validate_fractional_indices, ChangeStamp,
    InvalidFractionalIndexError,
};
use excali_core::order_key::generate_key_between;
use std::collections::HashSet;

/// Counts calls; the values are what upstream's test env would not pin.
struct Stamp {
    calls: u32,
}

impl ChangeStamp for Stamp {
    fn version_nonce(&mut self) -> f64 {
        self.calls += 1;
        f64::from(self.calls)
    }
    fn updated(&mut self) -> f64 {
        1.0
    }
}

/// `API.createElement({ id, index })`: a rectangle at version 1.
fn element(id: &str, index: Option<&str>) -> Element {
    let mut base = ElementBase::new(id, 0.0, 0.0, 1.0, 1.0);
    base.index = index.map(|i| FractionalIndex(i.to_owned()));
    Element::new(base, ElementKind::Rectangle)
}

fn elements(list: &[(&str, Option<&str>)]) -> Vec<Element> {
    list.iter().map(|&(id, index)| element(id, index)).collect()
}

fn validate(elements: &[Element]) -> Result<(), InvalidFractionalIndexError> {
    validate_fractional_indices(elements, true)
}

/// Upstream's `test()` helper.
fn check(input: Vec<Element>, moved: Option<&[&str]>, unchanged: &[&str], valid_input: bool) {
    if !valid_input {
        let err = validate(&input).expect_err("the input must be invalid");
        assert_eq!(err.code(), "ELEMENT_HAS_INVALID_INDEX");
    }

    let mut synced = input.clone();
    let mut stamp = Stamp { calls: 0 };
    match moved {
        Some(ids) => {
            let moved: HashSet<String> = ids.iter().map(|s| (*s).to_owned()).collect();
            sync_moved_indices(&mut synced, &moved, &mut stamp).expect("sync moved");
        }
        None => sync_invalid_indices(&mut synced, &mut stamp).expect("sync invalid"),
    }

    assert_eq!(synced.len(), input.len());
    if let Err(e) = validate(&synced) {
        panic!("synced indices are invalid: {e}");
    }
    let unchanged: HashSet<&str> = unchanged.iter().copied().collect();
    let mut bumped = 0;
    for (synced, element) in synced.iter().zip(&input) {
        assert_eq!(synced.base.id, element.base.id, "order changed");
        if unchanged.contains(synced.base.id.as_str()) {
            assert_eq!(synced.base.index, element.base.index, "{}", element.base.id);
            assert_eq!(
                synced.base.version, element.base.version,
                "{}",
                element.base.id
            );
        } else {
            assert_ne!(synced.base.index, element.base.index, "{}", element.base.id);
            assert_eq!(
                synced.base.version,
                element.base.version + 1.0,
                "{} mutated once",
                element.base.id
            );
            assert_ne!(synced.base.version_nonce, element.base.version_nonce);
            bumped += 1;
        }
    }
    assert_eq!(stamp.calls, bumped, "one versionNonce per mutation");
}

/// `testMovedIndicesSync`.
fn moved_sync(list: &[(&str, Option<&str>)], moved: &[&str], unchanged: &[&str], valid: bool) {
    check(elements(list), Some(moved), unchanged, valid);
}

/// `testInvalidIndicesSync`.
fn invalid_sync(list: &[(&str, Option<&str>)], unchanged: &[&str], valid: bool) {
    check(elements(list), None, unchanged, valid);
}

const VALID: bool = true;
const INVALID: bool = false;

#[test]
fn should_not_sync_empty_array() {
    moved_sync(&[], &[], &[], VALID);
    invalid_sync(&[], &[], VALID);
}

#[test]
fn should_not_sync_when_index_is_well_defined() {
    moved_sync(&[("A", Some("a1"))], &[], &["A"], VALID);
    invalid_sync(&[("A", Some("a1"))], &["A"], VALID);
}

#[test]
fn should_not_sync_when_indices_are_well_defined() {
    let list = [("A", Some("a1")), ("B", Some("a2")), ("C", Some("a3"))];
    moved_sync(&list, &[], &["A", "B", "C"], VALID);
    invalid_sync(&list, &["A", "B", "C"], VALID);
}

#[test]
fn should_sync_when_fractional_index_is_not_defined() {
    moved_sync(&[("A", None)], &["A"], &[], INVALID);
    invalid_sync(&[("A", None)], &[], INVALID);
}

#[test]
fn should_sync_when_fractional_index_is_malformed() {
    // "zd0032" has head "z", which requires length 28 per getIntegerLength,
    // but the string is far too short, so validateOrderKey throws for it
    invalid_sync(&[("A", Some("zd0032"))], &[], INVALID);
    invalid_sync(
        &[("A", Some("a1")), ("B", Some("zd0032")), ("C", Some("a3"))],
        &["A", "C"],
        INVALID,
    );
    invalid_sync(&[("A", Some("a!"))], &[], INVALID);
    invalid_sync(
        &[("A", Some("a1")), ("B", Some("a!")), ("C", Some("a2"))],
        &["A", "C"],
        INVALID,
    );
}

#[test]
fn should_sync_when_fractional_indices_are_duplicated() {
    let list = [("A", Some("a1")), ("B", Some("a1"))];
    invalid_sync(&list, &["A"], INVALID);
    invalid_sync(&list, &["A"], INVALID);
}

#[test]
fn should_sync_when_a_fractional_index_is_out_of_order() {
    let list = [("A", Some("a2")), ("B", Some("a1"))];
    moved_sync(&list, &["B"], &["A"], INVALID);
    moved_sync(&list, &["A"], &["B"], INVALID);
    invalid_sync(&list, &["A"], INVALID);
}

#[test]
fn should_sync_when_fractional_indices_are_out_of_order() {
    let list = [("A", Some("a3")), ("B", Some("a2")), ("C", Some("a1"))];
    moved_sync(&list, &["B", "C"], &["A"], INVALID);
    invalid_sync(&list, &["A"], INVALID);
}

#[test]
fn should_sync_when_incorrect_fractional_index_is_in_between_correct_ones() {
    let list = [("A", Some("a1")), ("B", Some("a0")), ("C", Some("a2"))];
    moved_sync(&list, &["B"], &["A", "C"], INVALID);
    invalid_sync(&list, &["A", "C"], INVALID);
}

#[test]
fn should_sync_when_incorrect_fractional_index_is_on_top_and_duplicated_below() {
    let list = [("A", Some("a1")), ("B", Some("a2")), ("C", Some("a1"))];
    moved_sync(&list, &["C"], &["A", "B"], INVALID);
    invalid_sync(&list, &["A", "B"], INVALID);
}

#[test]
fn should_sync_when_given_a_mix_of_duplicate_and_invalid_indices() {
    let list = [
        ("A", Some("a0")),
        ("B", Some("a2")),
        ("C", Some("a1")),
        ("D", Some("a1")),
        ("E", Some("a2")),
    ];
    moved_sync(&list, &["C", "D", "E"], &["A", "B"], INVALID);
    invalid_sync(&list, &["A", "B"], INVALID);
}

const MIXED_UNDEFINED: [(&str, Option<&str>); 10] = [
    ("A", None),
    ("B", None),
    ("C", Some("a0")),
    ("D", Some("a2")),
    ("E", None),
    ("F", Some("a3")),
    ("G", None),
    ("H", Some("a1")),
    ("I", Some("a2")),
    ("J", None),
];

#[test]
fn should_sync_when_given_a_mix_of_undefined_and_invalid_indices() {
    moved_sync(
        &MIXED_UNDEFINED,
        &["A", "B", "E", "G", "H", "I", "J"],
        &["C", "D", "F"],
        INVALID,
    );
    invalid_sync(&MIXED_UNDEFINED, &["C", "D", "F"], INVALID);
}

#[test]
fn should_sync_all_moved_elements_regardless_of_their_validity() {
    let list = [("A", Some("a2")), ("B", Some("a4"))];
    moved_sync(&list, &["A"], &["B"], VALID);
    moved_sync(&list, &["B"], &["A"], VALID);

    moved_sync(
        &[
            ("C", Some("a2")),
            ("D", Some("a3")),
            ("A", Some("a0")),
            ("B", Some("a1")),
        ],
        &["C", "D"],
        &["A", "B"],
        INVALID,
    );

    moved_sync(
        &[
            ("A", Some("a1")),
            ("B", Some("a2")),
            ("D", Some("a4")),
            ("C", Some("a3")),
            ("F", Some("a6")),
            ("E", Some("a5")),
            ("H", Some("a8")),
            ("G", Some("a7")),
            ("I", Some("a9")),
        ],
        &["D", "F", "H"],
        &["A", "B", "C", "E", "G", "I"],
        INVALID,
    );

    let list = [("A", Some("a1")), ("B", Some("a0")), ("C", Some("a2"))];
    moved_sync(&list, &["B", "C"], &["A"], INVALID);
    moved_sync(&list, &["A", "B"], &["C"], INVALID);

    moved_sync(
        &[
            ("A", Some("a0")),
            ("B", Some("a2")),
            ("C", Some("a1")),
            ("D", Some("a1")),
            ("E", Some("a2")),
        ],
        &["B", "D", "E"],
        &["A", "C"],
        INVALID,
    );

    moved_sync(
        &MIXED_UNDEFINED,
        &["A", "B", "D", "E", "F", "G", "J"],
        &["C", "H", "I"],
        INVALID,
    );
}

#[test]
fn should_generate_a_fraction_between_a_and_c() {
    // doing actual fractions, without jitter 'a1' becomes 'a1V'
    // as V is taken as the charset's middle-right value
    let list = [("A", Some("a1")), ("B", Some("a1")), ("C", Some("a2"))];
    moved_sync(&list, &["B"], &["A", "C"], INVALID);
    // as above, B will become fractional
    invalid_sync(&list, &["A", "C"], INVALID);
}

const DUPLICATED: [(&str, Option<&str>); 7] = [
    ("A", Some("a01")),
    ("B", Some("a01")),
    ("C", Some("a01")),
    ("D", Some("a01")),
    ("E", Some("a02")),
    ("F", Some("a02")),
    ("G", Some("a02")),
];

#[test]
fn should_generate_fractions_given_duplicated_indices() {
    moved_sync(
        &DUPLICATED,
        &["B", "C", "D", "E", "F"],
        &["A", "G"],
        INVALID,
    );
    moved_sync(
        &DUPLICATED,
        &["A", "C", "D", "E", "G"],
        &["B", "F"],
        INVALID,
    );
    moved_sync(
        &DUPLICATED,
        &["B", "C", "D", "F", "G"],
        &["A", "E"],
        INVALID,
    );
    // notice fallback considers first item (E) as a valid one
    invalid_sync(&DUPLICATED, &["A", "E"], INVALID);
}

const LENGTH: usize = 20_000;

fn ids() -> Vec<String> {
    (0..LENGTH).map(|i| format!("A_{i}")).collect()
}

#[test]
fn should_sync_all_empty_indices_of_20k_elements() {
    let ids = ids();
    let input: Vec<Element> = ids.iter().map(|id| element(id, None)).collect();
    let moved: Vec<&str> = ids.iter().map(String::as_str).collect();
    check(input.clone(), Some(&moved), &[], INVALID);
    check(input, None, &[], INVALID);
}

#[test]
fn should_sync_all_but_last_index_given_a_growing_array_of_20k_indices() {
    let ids = ids();
    let mut last: Option<String> = None;
    let input: Vec<Element> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            // going up from 'a0'
            last = Some(generate_key_between(last.as_deref(), None).unwrap());
            // assigning the last generated index, so sync can go down from
            // there; without jitter lastIndex is 'c4BZ' for the 20000th element
            element(
                id,
                if i == LENGTH - 1 {
                    last.as_deref()
                } else {
                    None
                },
            )
        })
        .collect();
    assert_eq!(last.as_deref(), Some("c4BZ"));
    let moved: Vec<&str> = ids[..LENGTH - 1].iter().map(String::as_str).collect();
    let last_id = format!("A_{}", LENGTH - 1);
    check(input.clone(), Some(&moved), &[&last_id], INVALID);
    check(input, None, &[&last_id], INVALID);
}

#[test]
fn should_sync_all_but_first_index_given_a_declining_array_of_20k_indices() {
    let ids = ids();
    let mut last: Option<String> = None;
    let input: Vec<Element> = ids
        .iter()
        .map(|id| {
            // going down from 'a0'
            last = Some(generate_key_between(None, last.as_deref()).unwrap());
            element(id, last.as_deref())
        })
        .collect();
    // without jitter lastIndex is 'XvoR' for the 20000th element
    assert_eq!(last.as_deref(), Some("XvoR"));
    let moved: Vec<&str> = ids[1..].iter().map(String::as_str).collect();
    check(input.clone(), Some(&moved), &["A_0"], INVALID);
    check(input, None, &["A_0"], INVALID);
}

#[test]
fn should_fallback_to_syncing_duplicated_indices_when_moved_elements_are_empty() {
    // the validation will throw as nothing was synced
    // therefore it will lead to triggering the fallback and fixing all invalid indices
    moved_sync(
        &[("A", Some("a1")), ("B", Some("a1")), ("C", Some("a1"))],
        &[],
        &["A"],
        INVALID,
    );
}

#[test]
fn should_fallback_to_syncing_undefined_and_invalid_indices_when_moved_elements_are_empty() {
    // since elements are invalid, this will fail the validation
    // leading to fallback fixing "B" and "C"
    moved_sync(
        &[("A", Some("a1")), ("B", None), ("C", Some("a0"))],
        &[],
        &["A"],
        INVALID,
    );
}

#[test]
fn should_fallback_to_syncing_unordered_indices_when_moved_element_is_invalid() {
    moved_sync(
        &[("A", Some("a1")), ("B", Some("a2")), ("C", Some("a1"))],
        &["A"],
        &["A", "B"],
        INVALID,
    );
}

#[test]
fn should_fallback_when_trying_to_generate_an_index_in_between_unordered_elements() {
    // 'B' is invalid, but so is 'C', which was not marked as moved
    // therefore it will try to generate a key between 'a2' and 'a1'
    // which it cannot do, thus will throw during generation and automatically fallback
    moved_sync(
        &[("A", Some("a2")), ("B", None), ("C", Some("a1"))],
        &["B"],
        &["A"],
        INVALID,
    );
}

#[test]
fn should_fallback_when_trying_to_generate_an_index_in_between_duplicate_indices() {
    // missed "E" therefore upper bound for 'B' is a01, while lower bound is 'a02'
    // therefore, similarly to above, it will fail during key generation and lead to fallback
    moved_sync(
        &[
            ("A", Some("a01")),
            ("B", None),
            ("C", None),
            ("D", Some("a01")),
            ("E", Some("a01")),
            ("F", Some("a01")),
            ("G", None),
            ("I", Some("a03")),
            ("H", None),
        ],
        &["B", "C", "D", "F", "G", "H"],
        &["A", "I"],
        INVALID,
    );
}

// ---------------------------------------------------------------------------
// Beyond the upstream test file

#[test]
fn order_by_fractional_index_sorts_by_index_then_id_as_v8_does() {
    use excali_core::fractional_index::order_by_fractional_index;
    let ids =
        |list: &[Element]| -> Vec<String> { list.iter().map(|e| e.base.id.clone()).collect() };

    // fractionalIndex.ts:153-164: by index, ties by id (JS string order)
    let mut list = elements(&[
        ("b", Some("a2")),
        ("B", Some("a1")),
        ("a", Some("a1")),
        ("c", Some("Zz")),
    ]);
    order_by_fractional_index(&mut list);
    assert_eq!(ids(&list), ["c", "B", "a", "b"]);

    // with an element without an index (null or "") the comparator answers
    // 1 both ways; the order is what V8's TimSort leaves (checked in node)
    let mut list = elements(&[
        ("x", Some("a3")),
        ("n", None),
        ("y", Some("a1")),
        ("e", Some("")),
        ("z", Some("a2")),
    ]);
    order_by_fractional_index(&mut list);
    assert_eq!(ids(&list), ["x", "n", "y", "e", "z"]);
}

#[test]
fn sync_keeps_versions_when_the_generated_index_equals_the_old_one() {
    // mutateElement is a no-op for an unchanged value (mutateElement.ts:83-93)
    let mut list = elements(&[("A", Some("a0"))]);
    let moved: HashSet<String> = ["A".to_owned()].into();
    let mut stamp = Stamp { calls: 0 };
    sync_moved_indices(&mut list, &moved, &mut stamp).unwrap();
    assert_eq!(list[0].base.index, Some(FractionalIndex("a0".into())));
    assert_eq!(list[0].base.version, 1.0);
    assert_eq!(stamp.calls, 0);
}

#[test]
fn immutable_sync_leaves_the_input_alone() {
    use excali_core::fractional_index::sync_invalid_indices_immutable;
    let input = elements(&[("A", Some("a1")), ("B", Some("a1")), ("C", None)]);
    let mut stamp = Stamp { calls: 0 };
    let synced = sync_invalid_indices_immutable(&input, &mut stamp).unwrap();
    assert_eq!(
        input,
        elements(&[("A", Some("a1")), ("B", Some("a1")), ("C", None)])
    );
    let got: Vec<Option<&str>> = synced
        .iter()
        .map(|e| e.base.index.as_ref().map(|i| i.0.as_str()))
        .collect();
    assert_eq!(got, [Some("a1"), Some("a2"), Some("a3")]);
    assert_eq!(stamp.calls, 2);
}

#[test]
fn validation_messages_use_upstreams_wording() {
    let list = elements(&[("A", Some("a2")), ("B", Some("a1"))]);
    let err = validate_fractional_indices(&list, false).unwrap_err();
    assert_eq!(
        err.messages,
        [
            "Fractional indices invariant has been compromised: \"undefined:undefined:undefined:undefined:undefined:undefined\", \"a2:A:rectangle:false:1:0\", \"a1:B:rectangle:false:1:0\"",
            "Fractional indices invariant has been compromised: \"a2:A:rectangle:false:1:0\", \"a1:B:rectangle:false:1:0\", \"undefined:undefined:undefined:undefined:undefined:undefined\"",
        ]
    );
    assert_eq!(err.to_string(), err.messages.join("\n\n"));
    assert_eq!(err.code(), InvalidFractionalIndexError::CODE);
}
