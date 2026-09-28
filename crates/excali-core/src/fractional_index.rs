//! Element fractional indices: a port of
//! `packages/element/src/fractionalIndex.ts` (see
//! `site/content/research/data-model.md`, section 6).
//!
//! The array order of the elements is the source of truth for rendering;
//! each element's `index` ([`FractionalIndex`]) must stay in sync with it:
//! well-formed ([`crate::order_key::validate_order_key`]) and strictly
//! between its neighbours' indices, compared as JS strings. The sync
//! functions repair invalid indices without touching valid ones, generating
//! keys with [`crate::order_key::generate_n_keys_between`], so they produce
//! upstream's exact key strings.
//!
//! Upstream mutates through `mutateElement` / `newElementWith`
//! (`mutateElement.ts:40-147, 149-180`), which bump `version`, draw a fresh
//! `versionNonce` from `randomInteger()` and set `updated` to
//! `getUpdatedTimestamp()`, and do nothing when the index is unchanged. The
//! caller supplies the nonce and timestamp through [`ChangeStamp`].

use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::element::{BoundElementType, Element, ElementKind};
use crate::json::js_number;
use crate::order_key::{
    compare_js_strings, generate_n_keys_between, validate_order_key, OrderKeyError,
};

use std::cmp::Ordering;

/// The values upstream draws on every element change: `versionNonce =
/// randomInteger()` and `updated = getUpdatedTimestamp()`
/// (`mutateElement.ts:142-144`).
pub trait ChangeStamp {
    /// A fresh random integer for `versionNonce`.
    fn version_nonce(&mut self) -> f64;
    /// Epoch milliseconds for `updated`.
    fn updated(&mut self) -> f64;
}

/// `InvalidFractionalIndexError` (`fractionalIndex.ts:23-25`). Upstream logs
/// the messages with `console.error` and throws an error without them; the
/// port returns them to the caller instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidFractionalIndexError {
    /// One message per violation, in upstream's wording and order.
    pub messages: Vec<String>,
}

impl InvalidFractionalIndexError {
    /// Upstream's error `code`.
    pub const CODE: &'static str = "ELEMENT_HAS_INVALID_INDEX";

    /// Upstream's error `code`.
    pub fn code(&self) -> &'static str {
        Self::CODE
    }
}

impl fmt::Display for InvalidFractionalIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.messages.join("\n\n"))
    }
}

impl std::error::Error for InvalidFractionalIndexError {}

// ---------------------------------------------------------------------------
// JS value helpers

/// The raw index at a JS array position: `elements[i]?.index`. Out of range
/// and `null` are both `None`.
fn raw_index(elements: &[Element], i: isize) -> Option<&str> {
    usize::try_from(i)
        .ok()
        .and_then(|i| elements.get(i))
        .and_then(index_of)
}

/// An element's `index`; `None` for `null`.
fn index_of(element: &Element) -> Option<&str> {
    element.base.index.as_ref().map(|i| i.0.as_str())
}

/// JS truthiness of an index: `null`, `undefined` and `""` are falsy.
fn truthy(index: Option<&str>) -> Option<&str> {
    index.filter(|s| !s.is_empty())
}

fn js_lt(a: &str, b: &str) -> bool {
    compare_js_strings(a, b) == Ordering::Less
}

/// `Number.prototype.toString` for any f64.
fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_owned()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        js_number(x)
    }
}

/// JS whitespace and line terminators, as `StringToNumber` trims them.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// ECMAScript `StringToNumber` (ECMA-262 §7.1.4.1.1).
fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(is_js_whitespace);
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut value = 0.0_f64;
            for c in digits.chars() {
                match c.to_digit(radix) {
                    Some(d) => value = value * f64::from(radix) + f64::from(d),
                    None => return f64::NAN,
                }
            }
            return value;
        }
    }
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if unsigned == "Infinity" {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    // StrUnsignedDecimalLiteral: digits [. digits] [exponent] | . digits [exponent]
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(p) => (&unsigned[..p], Some(&unsigned[p + 1..])),
        None => (unsigned, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all_digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = all_digits(int) && all_digits(frac) && !(int.is_empty() && frac.is_empty());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && all_digits(e)
    });
    if !mantissa_ok || !exponent_ok {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `a <= b` for two `index` values, where `None` is `null`: two strings
/// compare by code unit; otherwise both convert to numbers (`null` is 0).
fn loose_le(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => compare_js_strings(a, b) != Ordering::Greater,
        _ => {
            let number = |v: Option<&str>| v.map_or(0.0, string_to_number);
            number(a) <= number(b)
        }
    }
}

// ---------------------------------------------------------------------------
// Bound text (typeChecks.ts:240-305, textElement.ts:326-330)

/// `hasBoundTextElement`: a text container (`isTextBindableContainer`,
/// locked or not) with a bound element of type text.
fn has_bound_text_element(element: &Element) -> bool {
    let container = matches!(
        element.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Diamond
            | ElementKind::Ellipse
            | ElementKind::Arrow(_)
    );
    container
        && element
            .base
            .bound_elements
            .as_ref()
            .is_some_and(|b| b.iter().any(|b| b.kind == BoundElementType::Text))
}

/// `getBoundTextElementId`: the first bound text's id, `None` when empty.
fn bound_text_element_id(container: &Element) -> Option<&str> {
    container
        .base
        .bound_elements
        .as_ref()?
        .iter()
        .find(|b| b.kind == BoundElementType::Text)
        .map(|b| b.id.as_str())
        .filter(|id| !id.is_empty())
}

/// `stringifyElement` (`fractionalIndex.ts:67-68`).
fn stringify_element(element: Option<&Element>) -> String {
    match element {
        None => ["undefined"; 6].join(":"),
        Some(e) => format!(
            "{}:{}:{}:{}:{}:{}",
            e.base.index.as_ref().map_or("null", |i| i.0.as_str()),
            e.base.id,
            e.element_type().as_str(),
            e.base.is_deleted,
            number_to_string(e.base.version),
            number_to_string(e.base.version_nonce),
        ),
    }
}

// ---------------------------------------------------------------------------
// fractionalIndex.ts

/// `isValidFractionalIndex` (`fractionalIndex.ts:396-428`): the index is
/// set, well-formed and strictly between its (set) neighbours.
fn is_valid_fractional_index(
    index: Option<&str>,
    predecessor: Option<&str>,
    successor: Option<&str>,
) -> bool {
    let Some(index) = truthy(index) else {
        return false;
    };
    if validate_order_key(index).is_err() {
        return false;
    }
    match (truthy(predecessor), truthy(successor)) {
        (Some(p), Some(s)) => js_lt(p, index) && js_lt(index, s),
        // first element
        (None, Some(s)) => js_lt(index, s),
        // last element
        (Some(p), None) => js_lt(p, index),
        // only element in the array
        (None, None) => true,
    }
}

/// `validateFractionalIndices` (`fractionalIndex.ts:49-143`): every index is
/// valid relative to its array neighbours; with
/// `include_bound_text_validation`, also every non-deleted container's
/// non-deleted bound text has a greater index (checked, never auto-fixed).
///
/// Upstream's `shouldThrow`, `ignoreLogs` and `reconciliationContext` only
/// decide whether to throw and what to log; the port returns the messages
/// and leaves both to the caller.
pub fn validate_fractional_indices(
    elements: &[Element],
    include_bound_text_validation: bool,
) -> Result<(), InvalidFractionalIndexError> {
    let mut messages = Vec::new();
    // arrayToMap: the last element with an id wins
    let elements_map: Option<HashMap<&str, &Element>> = include_bound_text_validation
        .then(|| elements.iter().map(|e| (e.base.id.as_str(), e)).collect());

    for (i, element) in elements.iter().enumerate() {
        let i = i as isize;
        let at = |k: isize| usize::try_from(k).ok().and_then(|k| elements.get(k));
        if !is_valid_fractional_index(
            raw_index(elements, i),
            raw_index(elements, i - 1),
            raw_index(elements, i + 1),
        ) {
            messages.push(format!(
                "Fractional indices invariant has been compromised: \"{}\", \"{}\", \"{}\"",
                stringify_element(at(i - 1)),
                stringify_element(Some(element)),
                stringify_element(at(i + 1)),
            ));
        }

        // disabled by default, as we don't fix it
        if let Some(map) = &elements_map {
            if has_bound_text_element(element) && !element.base.is_deleted {
                let container = element;
                let text = bound_text_element_id(container).and_then(|id| map.get(id));
                if let Some(text) = text {
                    if !text.base.is_deleted && loose_le(index_of(text), index_of(container)) {
                        messages.push(format!(
                            "Fractional indices invariant for bound elements has been compromised: \"{}\", \"{}\"",
                            stringify_element(Some(text)),
                            stringify_element(Some(container)),
                        ));
                    }
                }
            }
        }
    }

    if messages.is_empty() {
        Ok(())
    } else {
        Err(InvalidFractionalIndexError { messages })
    }
}

/// `orderByFractionalIndex` (`fractionalIndex.ts:150-169`): sort by index,
/// breaking ties by `id`, both compared as JS strings.
///
/// Upstream's comparator answers 1 whenever either element has no index
/// (`null` or `""`: "defensively keep the array order"), and also for an
/// equal index and id, so it is not a consistent order. The result is then
/// whatever `Array.prototype.sort` leaves, so the port sorts with
/// [`excali_math::js::sort`], a port of V8's TimSort, and matches upstream
/// for every input (`goldens/fractional-index.json`).
pub fn order_by_fractional_index(elements: &mut Vec<Element>) {
    let mut order: Vec<usize> = (0..elements.len()).collect();
    excali_math::js::sort(&mut order, |&x, &y| {
        let (a, b) = (&elements[x], &elements[y]);
        // in case the indices are not defined at runtime
        match (truthy(index_of(a)), truthy(index_of(b))) {
            (Some(ia), Some(ib)) => match compare_js_strings(ia, ib) {
                Ordering::Less => -1.0,
                Ordering::Greater => 1.0,
                // break ties based on the element id
                Ordering::Equal if js_lt(&a.base.id, &b.base.id) => -1.0,
                Ordering::Equal => 1.0,
            },
            // defensively keep the array order
            _ => 1.0,
        }
    });
    let mut old: Vec<Option<Element>> = std::mem::take(elements).into_iter().map(Some).collect();
    *elements = order
        .into_iter()
        .filter_map(|from| old[from].take())
        .collect();
}

/// `mutateElement(element, elementsMap, { index })`: a no-op when the index
/// is unchanged, otherwise sets it and bumps `version`, `versionNonce` and
/// `updated`. `newElementWith` makes the same change on a copy.
fn set_index(element: &mut Element, index: String, stamp: &mut impl ChangeStamp) {
    if element.base.index.as_ref().is_some_and(|i| i.0 == index) {
        return;
    }
    element.base.index = Some(crate::element::FractionalIndex(index));
    element.base.version += 1.0;
    element.base.version_nonce = stamp.version_nonce();
    element.base.updated = stamp.updated();
}

/// `syncMovedIndices` (`fractionalIndex.ts:175-216`): give the moved
/// elements (by id) indices between their unmoved neighbours. If that fails
/// (a bound is itself invalid) or leaves any index invalid, falls back to
/// [`sync_invalid_indices`]. Nothing is mutated unless the whole update is
/// valid.
///
/// An error is returned only when the fallback fails, where upstream would
/// throw.
pub fn sync_moved_indices(
    elements: &mut [Element],
    moved: &HashSet<String>,
    stamp: &mut impl ChangeStamp,
) -> Result<(), OrderKeyError> {
    let attempt = || -> Option<Vec<(usize, String)>> {
        let groups = moved_indices_groups(elements, moved);
        // throws on invalid moved elements
        let updates = generate_indices(elements, groups).ok()?;
        let mut candidates: Vec<Option<&str>> = elements.iter().map(index_of).collect();
        for (i, index) in &updates {
            candidates[*i] = Some(index);
        }
        // ensure next indices are valid before mutation; bound text is not
        // auto-fixed, hence not validated
        let at = |k: isize| {
            usize::try_from(k)
                .ok()
                .and_then(|k| candidates.get(k).copied().flatten())
        };
        let valid = (0..candidates.len() as isize)
            .all(|i| is_valid_fractional_index(at(i), at(i - 1), at(i + 1)));
        valid.then_some(updates)
    };
    match attempt() {
        Some(updates) => {
            // split mutation so we don't end up in an inconsistent state
            for (i, index) in updates {
                set_index(&mut elements[i], index, stamp);
            }
            Ok(())
        }
        // fallback to default sync
        None => sync_invalid_indices(elements, stamp),
    }
}

/// `syncInvalidIndices` (`fractionalIndex.ts:223-235`): find every run of
/// invalid indices and fill it with keys between the run's valid bounds,
/// mutating those elements. Valid indices are kept; in edge cases an element
/// that was not moved gets a new index, as the array alone cannot say which
/// elements moved.
///
/// An error (nothing mutated) is returned where upstream would throw.
pub fn sync_invalid_indices(
    elements: &mut [Element],
    stamp: &mut impl ChangeStamp,
) -> Result<(), OrderKeyError> {
    let groups = invalid_indices_groups(elements);
    let updates = generate_indices(elements, groups)?;
    for (i, index) in updates {
        set_index(&mut elements[i], index, stamp);
    }
    Ok(())
}

/// Upstream's `SceneElementsMap`: elements by id in JS `Map` order (first
/// insertion of each id).
pub type SceneElementsMap = IndexMap<String, Element>;

/// `syncInvalidIndicesImmutable` (`fractionalIndex.ts:242-254`): the same
/// updates as [`sync_invalid_indices`], applied to copies and returned as
/// upstream's map: `arrayToMap(elements)` (each id at its first position,
/// holding its last element), then every updated copy set over its id in
/// group order. With duplicate ids the map can hold an earlier element than
/// `arrayToMap` alone would. `newElementWith` returns the element itself when
/// the index is unchanged, and that element is still set.
///
/// An error is returned where upstream would throw.
pub fn sync_invalid_indices_immutable(
    elements: &[Element],
    stamp: &mut impl ChangeStamp,
) -> Result<SceneElementsMap, OrderKeyError> {
    let groups = invalid_indices_groups(elements);
    let updates = generate_indices(elements, groups)?;
    let mut synced: SceneElementsMap = elements
        .iter()
        .map(|e| (e.base.id.clone(), e.clone()))
        .collect();
    for (i, index) in updates {
        let mut copy = elements[i].clone();
        set_index(&mut copy, index, stamp);
        // Map.set: an existing key keeps its position
        synced.insert(copy.base.id.clone(), copy);
    }
    Ok(synced)
}

/// `getMovedIndicesGroups` (`fractionalIndex.ts:261-289`): contiguous runs
/// of moved elements, each preceded by its lower bound position and
/// followed by its upper bound position.
fn moved_indices_groups(elements: &[Element], moved: &HashSet<String>) -> Vec<Vec<isize>> {
    let mut groups = Vec::new();
    let len = elements.len();
    let mut i = 0;
    while i < len {
        if moved.contains(&elements[i].base.id) {
            // the lower bound position first
            let mut group = vec![i as isize - 1, i as isize];
            i += 1;
            while i < len && moved.contains(&elements[i].base.id) {
                group.push(i as isize);
                i += 1;
            }
            // the upper bound position last
            group.push(i as isize);
            groups.push(group);
        } else {
            i += 1;
        }
    }
    groups
}

/// `getInvalidIndicesGroups` (`fractionalIndex.ts:296-394`): runs of
/// invalid indices, each preceded by the found lower bound position and
/// followed by the found upper bound position (not necessarily adjacent).
fn invalid_indices_groups(elements: &[Element]) -> Vec<Vec<isize>> {
    let len = elements.len() as isize;
    let value = |k: isize| raw_index(elements, k);
    // once found, a bound cannot be lower than that, so it is cached
    let mut lower_bound_index: isize = -1;
    let mut upper_bound_index: isize = 0;

    // maybe valid lower bound
    let lower_bound = |index: isize, lower_bound_index: isize| -> isize {
        let lower = truthy(value(lower_bound_index));
        // iterating left to right, so no additional looping is needed
        let candidate = truthy(value(index - 1));
        match (lower, candidate) {
            // first lower bound, or the next one
            (None, Some(_)) => index - 1,
            (Some(l), Some(c)) if js_lt(l, c) => index - 1,
            // cache hit! take the last lower bound
            _ => lower_bound_index,
        }
    };
    // always valid upper bound
    let upper_bound = |index: isize, upper_bound_index: isize| -> isize {
        let upper = truthy(value(upper_bound_index));
        // cache hit! don't let it find the upper bound again
        if upper.is_some() && index < upper_bound_index {
            return upper_bound_index;
        }
        // start from the current upper bound
        let mut i = upper_bound_index + 1;
        while i < len {
            let candidate = truthy(value(i));
            match (upper, candidate) {
                (None, Some(_)) => return i,
                (Some(u), Some(c)) if js_lt(u, c) => return i,
                _ => {}
            }
            i += 1;
        }
        // reached the end, sky is the limit
        i
    };

    let mut groups = Vec::new();
    let mut i: isize = 0;
    while i < len {
        lower_bound_index = lower_bound(i, lower_bound_index);
        upper_bound_index = upper_bound(i, upper_bound_index);

        if !is_valid_fractional_index(value(i), value(lower_bound_index), value(upper_bound_index))
        {
            // the lower bound position first
            let mut group = vec![lower_bound_index, i];
            i += 1;
            while i < len {
                let next_lower = lower_bound(i, lower_bound_index);
                let next_upper = upper_bound(i, upper_bound_index);
                if is_valid_fractional_index(value(i), value(next_lower), value(next_upper)) {
                    break;
                }
                // assign bounds only for the moved elements
                lower_bound_index = next_lower;
                upper_bound_index = next_upper;
                group.push(i);
                i += 1;
            }
            // the upper bound position last
            group.push(upper_bound_index);
            groups.push(group);
        } else {
            i += 1;
        }
    }
    groups
}

/// `generateIndices` (`fractionalIndex.ts:430-459`): keys for each group's
/// inner positions, between its bounds' current indices, in group order.
fn generate_indices(
    elements: &[Element],
    groups: Vec<Vec<isize>>,
) -> Result<Vec<(usize, String)>, OrderKeyError> {
    let mut updates = Vec::new();
    for group in groups {
        let (Some(&lower), Some(&upper)) = (group.first(), group.last()) else {
            continue;
        };
        let inner = &group[1..group.len() - 1];
        let keys = generate_n_keys_between(
            raw_index(elements, lower),
            raw_index(elements, upper),
            inner.len(),
        )?;
        for (&position, key) in inner.iter().zip(keys) {
            updates.push((position as usize, key));
        }
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::{loose_le, string_to_number};

    #[test]
    fn string_to_number_follows_ecmascript() {
        for (s, n) in [
            ("", 0.0),
            ("  \n", 0.0),
            ("1", 1.0),
            (" -1.5e1 ", -15.0),
            ("+.5", 0.5),
            ("5.", 5.0),
            ("0x1F", 31.0),
            ("0b101", 5.0),
            ("0o17", 15.0),
            ("1e-400", 0.0),
            ("-0", 0.0),
        ] {
            assert_eq!(string_to_number(s), n, "{s:?}");
        }
        assert_eq!(string_to_number("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(string_to_number("Infinity"), f64::INFINITY);
        for s in [
            "a1", "inf", "nan", "infinity", "1e", ".", "0x", "-0x1", "1_0", "e5", "1.2.3",
        ] {
            assert!(string_to_number(s).is_nan(), "{s:?}");
        }
    }

    #[test]
    fn loose_le_compares_like_js() {
        // two strings: code-unit order
        assert!(loose_le(Some("a0"), Some("a1")));
        assert!(loose_le(Some("a1"), Some("a1")));
        assert!(!loose_le(Some("a2"), Some("a1")));
        // null converts to 0, a key to NaN
        assert!(loose_le(None, None));
        assert!(!loose_le(None, Some("a1")));
        assert!(!loose_le(Some("a1"), None));
        assert!(loose_le(Some(""), None));
        assert!(loose_le(None, Some(" ")));
        assert!(loose_le(Some("-1"), None));
        assert!(!loose_le(Some("1"), None));
    }
}
