//! `Color::rgba` against the browser (ex-216).
//!
//! Upstream assigns element colours to `fillStyle` and `strokeStyle` as
//! stored (`renderElement.ts`; `applyDarkModeFilter` returns the colour
//! unchanged outside dark mode), so the canvas's CSS parser decides what a
//! colour paints. `tests/fixtures/css-colors.json` holds what headless
//! Chrome's canvas did with every case in
//! `scripts/fixtures/css_color_goldens.html`: the `fillStyle` read back
//! after assigning a sentinel and then the case (the sentinel when the
//! assignment was ignored), and the pixel `fillRect` painted.

use excali_scene::display::{Color, Rgba};
use serde_json::Value;

/// Forms Chrome's canvas accepts that the port's parser does not, each
/// documented in `display/css_color.rs`: CSS Color 5 and the math
/// functions beyond `calc()`, `min()`, `max()` and `clamp()`. They resolve
/// to the current style instead.
const GAPS: [&str; 3] = [
    "rgb(from red r g b)",
    "color-mix(in srgb, red, blue)",
    "rgb(round(127.4) 0 0)",
];

/// Components computed through a colour space conversion; Chrome's
/// matrices round differently in the last bit, which can move a channel by
/// one step.
fn converted(css: &str) -> bool {
    let css = css.to_ascii_lowercase();
    ["lab(", "lch(", "oklab(", "oklch(", "color("]
        .iter()
        .any(|f| css.starts_with(f))
}

/// `rgba(r, g, b, a)`: the channels Chrome serialises a translucent legacy
/// colour with.
fn legacy_channels(style: &str) -> Option<[u8; 3]> {
    let inner = style.strip_prefix("rgba(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(", ").collect();
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

#[test]
fn every_case_resolves_as_chrome_paints_it() {
    let data: Value =
        serde_json::from_str(include_str!("fixtures/css-colors.json")).expect("fixture parses");
    let sentinel = data["sentinel"].as_str().unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() > 400, "{} cases", cases.len());
    let mut failures = Vec::new();
    for case in cases {
        let css = case["css"].as_str().unwrap();
        let style = case["style"].as_str().unwrap();
        let pixel: Vec<u8> = case["pixel"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        let chrome_valid = style != sentinel;
        let ours = Color::new(css).rgba();
        if GAPS.contains(&css) {
            assert!(
                chrome_valid,
                "{css:?} is no longer a gap: Chrome rejects it"
            );
            assert_eq!(ours, None, "{css:?} is listed as a gap");
            continue;
        }
        let Some(ours) = ours else {
            if chrome_valid {
                failures.push(format!(
                    "{css:?}: Chrome paints {style}, the port ignores it"
                ));
            }
            continue;
        };
        if !chrome_valid {
            failures.push(format!(
                "{css:?}: Chrome ignores it, the port reads {ours:?}"
            ));
            continue;
        }
        // Alpha as the 8-bit canvas stores it.
        let alpha = (ours.a * 255.0).round() as u8;
        if alpha != pixel[3] {
            failures.push(format!(
                "{css:?}: alpha {} vs Chrome's {}",
                ours.a, pixel[3]
            ));
            continue;
        }
        let rgb = [ours.r, ours.g, ours.b];
        let (expected, tolerance) = if pixel[3] == 255 {
            (
                [pixel[0], pixel[1], pixel[2]],
                if converted(css) { 1 } else { 0 },
            )
        } else if let Some(channels) = legacy_channels(style) {
            (channels, 0)
        } else if pixel[3] >= 128 {
            // Premultiplied and back: Chrome's readback is off by up to 2.
            ([pixel[0], pixel[1], pixel[2]], 2)
        } else {
            continue;
        };
        if rgb
            .iter()
            .zip(expected)
            .any(|(a, b)| a.abs_diff(b) > tolerance)
        {
            failures.push(format!("{css:?}: {rgb:?} vs Chrome's {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_reviewed_divergences_from_tinycolor() {
    // Modern rgb() alpha: half-transparent, as the canvas paints it.
    assert_eq!(
        Color::new("rgb(255 0 0 / 50%)").rgba(),
        Some(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 0.5
        })
    );
    // Not colours on a canvas (tinycolor accepted them).
    assert_eq!(Color::new("ff0000").rgba(), None);
    assert_eq!(Color::new("hsv(0,100%,100%)").rgba(), None);
    // Colours on a canvas (tinycolor rejected them).
    assert_eq!(
        Color::new("hsl(120deg, 100%, 50%)").rgba(),
        Some(Rgba {
            r: 0,
            g: 255,
            b: 0,
            a: 1.0
        })
    );
    assert_eq!(
        Color::new("hwb(0 0% 0%)").rgba(),
        Some(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0
        })
    );
}
