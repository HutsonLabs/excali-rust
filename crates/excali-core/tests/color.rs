//! Colour parsing (ex-106): `colorToHex` and `isTransparent` from
//! `packages/common/src/colors.ts`, both on tinycolor2 1.6.0, which
//! `restoreAppState` uses to dedupe colour top picks and to reset
//! transparent sticky-note colours.
//!
//! Fixture: the `colors` cases of `fixtures/app-state.json`, upstream's own
//! output (`tools/goldens/app-state.mjs`).

use excali_core::color::{apply_dark_mode_filter, color_to_hex, is_transparent, TinyColor};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/app-state.json");

#[test]
fn color_to_hex_and_is_transparent_match_upstream() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["colors"].as_array().unwrap();
    assert!(cases.len() > 90);
    let mut failures = Vec::new();
    for case in cases {
        let input = case["input"].as_str().unwrap();
        let hex = color_to_hex(input);
        if hex.as_deref() != case["hex"].as_str() {
            failures.push(format!(
                "colorToHex({input:?}) = {hex:?}, upstream {}",
                case["hex"]
            ));
        }
        let transparent = is_transparent(input);
        if Value::Bool(transparent) != case["transparent"] {
            failures.push(format!(
                "isTransparent({input:?}) = {transparent}, upstream {}",
                case["transparent"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn tinycolor_parses_the_css_notations() {
    let c = TinyColor::parse("rgba(10, 20, 30, .25)");
    assert!(c.is_valid());
    assert_eq!(c.to_rgb(), (10.0, 20.0, 30.0, 0.25));
    let c = TinyColor::parse("  Transparent ");
    assert!(c.is_valid());
    assert_eq!(c.alpha(), 0.0);
    let c = TinyColor::parse("#1234");
    assert_eq!(c.to_rgb(), (17.0, 34.0, 51.0, 68.0 / 255.0));
    let c = TinyColor::parse("hsl(120, 100%, 50%)");
    assert_eq!(c.to_rgb(), (0.0, 255.0, 0.0, 1.0));
    let c = TinyColor::parse("cornflowerblue");
    assert_eq!(c.to_rgb(), (100.0, 149.0, 237.0, 1.0));
    let c = TinyColor::parse("not-a-color");
    assert!(!c.is_valid());
    assert_eq!(c.alpha(), 1.0);
}

#[test]
fn hex_keeps_alpha_only_below_one() {
    assert_eq!(color_to_hex("rgba(0,0,0,1)").as_deref(), Some("#000000"));
    assert_eq!(
        color_to_hex("rgba(0,0,0,0.5)").as_deref(),
        Some("#00000080")
    );
    assert_eq!(color_to_hex("transparent").as_deref(), Some("#00000000"));
    assert_eq!(color_to_hex("rgba(0,0,0,2)").as_deref(), Some("#000000"));
    assert_eq!(color_to_hex("nope"), None);
}

/// `applyDarkModeFilter` (`colors.ts:86-125`): CSS `invert(93%)` then
/// `hue-rotate(180deg)`, computed numerically; the alpha is kept. Values
/// from upstream (`darkMode` in
/// `crates/excali-scene/tests/fixtures/rough-options.json`, which
/// excali-scene checks in full).
#[test]
fn apply_dark_mode_filter_matches_upstream_examples() {
    for (color, dark) in [
        ("#1e1e1e", "#d3d3d3"),
        ("#e03131", "#ff8383"),
        ("#ffffff", "#121212"),
        ("#000000", "#ededed"),
        ("#abc", "#41505e"),
        ("#11223344", "#c5d4e344"),
    ] {
        assert_eq!(apply_dark_mode_filter(color, true), dark, "{color}");
    }
    // `enable = false` returns the input untouched, even a non-colour.
    for color in ["#1e1e1e", "transparent", "not a colour", ""] {
        assert_eq!(apply_dark_mode_filter(color, false), color);
    }
}

#[test]
fn line_confirm_threshold_is_upstreams() {
    // constants.ts:30
    assert_eq!(excali_core::constants::LINE_CONFIRM_THRESHOLD, 8.0);
}
