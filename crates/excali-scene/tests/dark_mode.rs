//! The dark-mode colour filter (ex-218): `applyDarkModeFilter`,
//! `removeDarkModeFilter` and `rgbToHex` (`packages/common/src/colors.ts`),
//! `COLOR_PALETTE` and `DARK_THEME_FILTER` (`common/src/constants.ts:204`).
//!
//! Fixture: `tests/fixtures/dark-mode.json`, upstream's own output at the
//! pinned commit (`tools/goldens/dark-mode.mjs`; its test checks the palette
//! results against upstream's vitest snapshot).

use excali_core::color::{
    apply_dark_mode_filter, remove_dark_mode_filter, rgb_to_hex, PaletteColor, COLOR_PALETTE,
};
use excali_core::constants::DARK_THEME_FILTER;
use excali_scene::display::ImageFilter;
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/dark-mode.json")).unwrap()
}

#[test]
fn dark_theme_filter_is_upstreams() {
    let fixture = fixture();
    assert_eq!(fixture["filter"].as_str(), Some(DARK_THEME_FILTER));
    assert_eq!(ImageFilter::DarkTheme.css(), DARK_THEME_FILTER);
}

#[test]
fn color_palette_is_upstreams() {
    let fixture = fixture();
    let mut palette = serde_json::Map::new();
    for (name, entry) in COLOR_PALETTE.entries() {
        let value = match entry {
            PaletteColor::Single(color) => json!(color),
            PaletteColor::Shades(shades) => json!(shades),
        };
        palette.insert(name.to_owned(), value);
    }
    let expected = fixture["palette"].as_object().unwrap();
    // Same entries in the same order.
    assert_eq!(
        palette.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    assert_eq!(&palette, expected);
    assert_eq!(COLOR_PALETTE.black, "#1e1e1e");
    assert_eq!(COLOR_PALETTE.red[4], "#e03131");
    assert_eq!(COLOR_PALETTE.bronze[0], "#f8f1ee");
}

#[test]
fn apply_dark_mode_filter_matches_upstream_for_the_full_palette() {
    let fixture = fixture();
    let cases = fixture["paletteFilter"].as_array().unwrap();
    assert_eq!(cases.len(), 63);
    let mut failures = Vec::new();
    for case in cases {
        let color = case["color"].as_str().unwrap();
        let dark = apply_dark_mode_filter(color, true);
        let remove = remove_dark_mode_filter(color);
        let restored = remove_dark_mode_filter(&dark);
        let redark = apply_dark_mode_filter(&restored, true);
        for (what, got) in [
            ("dark", &dark),
            ("remove", &remove),
            ("restored", &restored),
            ("redark", &redark),
        ] {
            if Some(got.as_str()) != case[what].as_str() {
                failures.push(format!("{color} {what}: {got}, upstream {}", case[what]));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn apply_dark_mode_filter_matches_upstream_in_every_notation() {
    let fixture = fixture();
    let cases = fixture["apply"].as_array().unwrap();
    assert!(cases.len() > 30);
    let mut failures = Vec::new();
    for case in cases {
        let color = case["color"].as_str().unwrap();
        let got = apply_dark_mode_filter(color, true);
        if Some(got.as_str()) != case["result"].as_str() {
            failures.push(format!("{color:?}: {got}, upstream {}", case["result"]));
        }
        let disabled = apply_dark_mode_filter(color, false);
        if Some(disabled.as_str()) != case["disabled"].as_str() {
            failures.push(format!("{color:?} disabled: {disabled}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn remove_dark_mode_filter_matches_upstream() {
    let fixture = fixture();
    let cases = fixture["remove"].as_array().unwrap();
    assert!(cases.len() > 256);
    let mut failures = Vec::new();
    for case in cases {
        let color = case["color"].as_str().unwrap();
        let got = remove_dark_mode_filter(color);
        if Some(got.as_str()) != case["result"].as_str() {
            failures.push(format!("{color:?}: {got}, upstream {}", case["result"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn rgb_to_hex_matches_upstream() {
    let fixture = fixture();
    let cases = fixture["rgbToHex"].as_array().unwrap();
    let mut failures = Vec::new();
    for case in cases {
        let args: Vec<Option<f64>> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_f64)
            .collect();
        let a = args.get(3).copied().flatten();
        let got = rgb_to_hex(args[0].unwrap(), args[1].unwrap(), args[2].unwrap(), a);
        if Some(got.as_str()) != case["result"].as_str() {
            failures.push(format!("{:?}: {got}, upstream {}", case["args"], case["result"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `colors.test.ts:14-261`, ported case by case.
#[test]
fn upstream_colors_test_cases() {
    assert_eq!(apply_dark_mode_filter("#000000", true), "#ededed");
    assert_eq!(apply_dark_mode_filter("#ffffff", true), "#121212");
    assert_eq!(apply_dark_mode_filter("#ff0000", true), "#ff9090");
    assert_eq!(apply_dark_mode_filter("#00ff00", true), "#008f00");
    assert_eq!(apply_dark_mode_filter("#0000ff", true), "#cdcdff");
    assert_eq!(apply_dark_mode_filter("red", true), "#ff9090");
    assert_eq!(apply_dark_mode_filter("rgb(255, 0, 0)", true), "#ff9090");
    assert_eq!(apply_dark_mode_filter("rgba(255, 0, 0, 0.5)", true), "#ff909080");
    assert_eq!(apply_dark_mode_filter("transparent", true), "#ededed00");
    assert_eq!(apply_dark_mode_filter("#f00", true), "#ff9090");
    assert_eq!(apply_dark_mode_filter("#ff0000ff", true), "#ff9090");
    assert!(apply_dark_mode_filter("#ff000080", true).ends_with("80"));
    assert!(apply_dark_mode_filter("#ff000000", true).ends_with("00"));
    assert_eq!(apply_dark_mode_filter(COLOR_PALETTE.black, true), "#d3d3d3");
    assert_eq!(apply_dark_mode_filter(COLOR_PALETTE.white, true), "#121212");
    assert_eq!(apply_dark_mode_filter(COLOR_PALETTE.transparent, true), "#ededed00");

    assert_eq!(remove_dark_mode_filter("#ededed"), "#000000");
    assert_eq!(remove_dark_mode_filter("#121212"), "#ffffff");
    for color in [
        COLOR_PALETTE.black,
        COLOR_PALETTE.white,
        COLOR_PALETTE.red[4],
        COLOR_PALETTE.green[4],
        COLOR_PALETTE.blue[4],
    ] {
        let filtered = apply_dark_mode_filter(color, true);
        let back = remove_dark_mode_filter(&filtered);
        assert_eq!(apply_dark_mode_filter(&back, true), filtered, "{color}");
    }

    assert_eq!(rgb_to_hex(0.0, 0.0, 0.0, None), "#000000");
    assert_eq!(rgb_to_hex(0.0, 0.0, 1.0, None), "#000001");
    assert_eq!(rgb_to_hex(15.0, 15.0, 15.0, None), "#0f0f0f");
    assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, Some(1.0)), "#ff0000");
    assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, Some(0.5)), "#ff000080");
    assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, Some(0.0)), "#ff000000");
    assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, Some(0.99)), "#ff0000fc");
    assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, Some(0.05)), "#ff00000d");
}
