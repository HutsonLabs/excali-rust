//! Acceptance for ex-302: text measured from the vendored font files matches
//! the width upstream stored within 0.5 px.
//!
//! Fixture: `tests/fixtures/text-widths.json`, 991 text elements of the
//! ex-003 library corpus with the `width` and `height` the browser measured
//! (`measureText`, `packages/element/src/textMeasurements.ts:12-27`),
//! extracted by `scripts/fixtures/text_widths.py` (which documents the
//! selection). Each is measured as upstream does: `measureText(text,
//! getFontString(element), lineHeight)`.

mod common;

use std::collections::BTreeMap;

use excali_core::element::FontFamily;
use excali_text::font_metadata::get_font_string;
use excali_text::text_measurements::measure_text;
use serde_json::Value;

const TOLERANCE: f64 = 0.5;

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/text-widths.json")).unwrap();
    fixture["cases"].as_array().unwrap().clone()
}

#[test]
fn fixture_covers_every_vendored_element_family_with_texts() {
    let mut per_family: BTreeMap<u64, usize> = BTreeMap::new();
    for case in cases() {
        *per_family
            .entry(case["fontFamily"].as_u64().unwrap())
            .or_default() += 1;
    }
    // Virgil, Cascadia, Excalifont, Nunito, Lilita One, Comic Shanns: every
    // vendored element family the corpus has measured texts for.
    assert_eq!(
        per_family.keys().copied().collect::<Vec<_>>(),
        vec![1, 3, 5, 6, 7, 8]
    );
    assert_eq!(per_family.values().sum::<usize>(), 991);
}

#[test]
fn measured_widths_match_stored_widths_within_half_a_pixel() {
    let store = common::store();
    let mut failures = Vec::new();
    let mut worst: BTreeMap<u64, f64> = BTreeMap::new();
    for case in cases() {
        let family = case["fontFamily"].as_u64().unwrap();
        let font = get_font_string(
            case["fontSize"].as_f64().unwrap(),
            FontFamily(u32::try_from(family).unwrap()),
        );
        let text = case["text"].as_str().unwrap();
        let measured = measure_text(text, &font, case["lineHeight"].as_f64().unwrap(), store);
        let stored = case["width"].as_f64().unwrap();
        let deviation = (measured.width - stored).abs();
        let w = worst.entry(family).or_default();
        *w = w.max(deviation);
        if deviation > TOLERANCE {
            failures.push(format!(
                "{} item {} id {}: {text:?} in {font:?}: measured {} stored {stored}",
                case["source"], case["item"], case["id"], measured.width
            ));
        }
    }
    eprintln!("max deviation per fontFamily: {worst:?}");
    assert!(
        failures.is_empty(),
        "{} of the fixture texts deviate by more than {TOLERANCE} px:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn measured_heights_match_stored_heights() {
    let store = common::store();
    let mut failures = Vec::new();
    for case in cases() {
        let family = FontFamily(u32::try_from(case["fontFamily"].as_u64().unwrap()).unwrap());
        let font = get_font_string(case["fontSize"].as_f64().unwrap(), family);
        let text = case["text"].as_str().unwrap();
        let measured = measure_text(text, &font, case["lineHeight"].as_f64().unwrap(), store);
        let stored = case["height"].as_f64().unwrap();
        if (measured.height - stored).abs() > TOLERANCE {
            failures.push(format!(
                "{} id {}: {text:?}: measured height {} stored {stored}",
                case["source"], case["id"], measured.height
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
