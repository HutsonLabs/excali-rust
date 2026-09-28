//! The wrapping port against upstream's own output:
//! `tests/fixtures/text-wrapping.json`, written by
//! `tools/goldens/text-wrapping.mjs` from `parseTokens` and
//! `getWrappedTextLines` (`packages/element/src/textWrapping.ts`) at the
//! pinned commit, for every input of `textWrapping.test.ts`, edge cases and
//! seeded random strings, under two line-width providers.

use excali_text::text_measurements::{CharCountTextMetrics, CharWidthCache, TextMetricsProvider};
use excali_text::text_wrapping::{get_wrapped_text_lines, parse_tokens, wrap_text};
use serde_json::Value;

fn goldens() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/text-wrapping.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("text-wrapping.json"))
        .expect("text-wrapping.json parses")
}

/// The fixture's `varied` metric: each UTF-16 code unit `u` is
/// `3 + (u * 7) % 11` wide, less 0.5 for every adjacent pair whose sum is a
/// multiple of 5, so a line is not the sum of its characters.
struct VariedMetrics;

impl TextMetricsProvider for VariedMetrics {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut width = 0.0;
        for (i, &u) in units.iter().enumerate() {
            width += f64::from(3 + (u32::from(u) * 7) % 11);
            if i > 0 && (u32::from(units[i - 1]) + u32::from(u)) % 5 == 0 {
                width -= 0.5;
            }
        }
        width
    }
}

fn provider(metric: &str) -> &'static dyn TextMetricsProvider {
    match metric {
        "chars10" => &CharCountTextMetrics,
        "varied" => &VariedMetrics,
        other => panic!("unknown metric {other}"),
    }
}

fn width(value: &Value) -> f64 {
    match value {
        Value::Number(n) => n.as_f64().expect("finite width"),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("width {other}"),
        },
        other => panic!("width {other}"),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

#[test]
fn the_fixture_is_upstream_output_at_the_pin() {
    let g = goldens();
    let config = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../site/config.toml"
    ))
    .expect("site/config.toml");
    let commit = g["upstream"].as_str().expect("upstream commit");
    assert!(
        config.contains(&format!("upstream_commit = \"{commit}\"")),
        "text-wrapping.json was generated at {commit}, not the pinned commit"
    );
    assert_eq!(g["font"], "10px Cascadia, Segoe UI Emoji");
}

#[test]
fn parse_tokens_matches_upstream() {
    let g = goldens();
    let cases = g["tokens"].as_array().expect("tokens");
    assert!(cases.len() > 150, "{} token cases", cases.len());
    let mut failures = Vec::new();
    for case in cases {
        let line = case["line"].as_str().expect("line");
        let want = strings(&case["tokens"]);
        let got = parse_tokens(line);
        if got != want {
            failures.push(format!("{line:?}\n  want {want:?}\n  got  {got:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} lines tokenize differently:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn wrapped_text_lines_match_upstream() {
    let g = goldens();
    let font = g["font"].as_str().expect("font");
    let wraps = g["wraps"].as_array().expect("wraps");
    let mut checked = 0;
    let mut failures = Vec::new();
    for wrap in wraps {
        let metric = wrap["metric"].as_str().expect("metric");
        let text = wrap["text"].as_str().expect("text");
        for case in wrap["cases"].as_array().expect("cases") {
            let max_width = width(&case["maxWidth"]);
            let want: Vec<(String, usize, usize)> = case["lines"]
                .as_array()
                .expect("lines")
                .iter()
                .map(|l| {
                    (
                        l[0].as_str().expect("text").to_owned(),
                        l[1].as_u64().expect("start") as usize,
                        l[2].as_u64().expect("end") as usize,
                    )
                })
                .collect();
            // A fresh cache per call: the generator clears upstream's
            // charWidth cache before each one.
            let got: Vec<(String, usize, usize)> = get_wrapped_text_lines(
                text,
                font,
                max_width,
                provider(metric),
                &mut CharWidthCache::new(),
            )
            .into_iter()
            .map(|l| (l.text, l.start, l.end))
            .collect();
            if got != want {
                failures.push(format!(
                    "{metric} {text:?} maxWidth {max_width}\n  want {want:?}\n  got  {got:?}"
                ));
            }
            // wrapText is the lines' texts joined by "\n".
            let joined = want
                .iter()
                .map(|(t, _, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let wrapped = wrap_text(
                text,
                font,
                max_width,
                provider(metric),
                &mut CharWidthCache::new(),
            );
            if wrapped != joined {
                failures.push(format!(
                    "wrap_text {metric} {text:?} maxWidth {max_width}\n  want {joined:?}\n  got  {wrapped:?}"
                ));
            }
            checked += 1;
        }
    }
    assert!(checked > 4000, "{checked} cases");
    assert!(
        failures.is_empty(),
        "{} of {checked} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Upstream's cache is shared across calls; the port's is the caller's.
/// Reusing one cache for every case gives the same lines as a fresh one
/// whenever no two measured characters share a first UTF-16 code unit
/// (the cache key), which holds for the BMP-only cases here.
#[test]
fn a_shared_cache_gives_the_same_lines_for_bmp_text() {
    let g = goldens();
    let font = g["font"].as_str().expect("font");
    let mut cache = CharWidthCache::new();
    for wrap in g["wraps"].as_array().expect("wraps") {
        let metric = wrap["metric"].as_str().expect("metric");
        if metric != "varied" {
            continue;
        }
        let text = wrap["text"].as_str().expect("text");
        if text.chars().any(|c| u32::from(c) > 0xFFFF) {
            continue;
        }
        for case in wrap["cases"].as_array().expect("cases") {
            let max_width = width(&case["maxWidth"]);
            let shared = get_wrapped_text_lines(text, font, max_width, &VariedMetrics, &mut cache);
            let fresh = get_wrapped_text_lines(
                text,
                font,
                max_width,
                &VariedMetrics,
                &mut CharWidthCache::new(),
            );
            assert_eq!(shared, fresh, "{text:?} at {max_width}");
        }
    }
}
