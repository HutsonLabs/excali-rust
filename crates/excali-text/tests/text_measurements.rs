//! `packages/element/src/textMeasurements.ts`: line splitting, tabs, empty
//! lines, width and height, the character-width cache and the minimum
//! widths, with a recording provider and with the port's font measurement.
//!
//! Upstream's own cases: `detectLineHeight` and `getLineHeightInPx` from
//! `packages/element/tests/textElement.test.ts:174-194`.

mod common;

use std::cell::RefCell;

use excali_core::element::FontFamily;
use excali_text::font_metadata::get_font_string;
use excali_text::text_measurements::{
    detect_line_height, get_approx_min_line_height, get_approx_min_line_width, get_line_width,
    get_max_char_width, get_min_char_width, get_min_text_element_width, get_text_height,
    get_text_width, is_measure_text_supported, measure_text, normalize_eol, normalize_text,
    CharCountTextMetrics, CharWidthCache, TextDimensions, TextMetricsProvider, DUMMY_TEXT,
};

/// Records every line measured; each line is 7 px per UTF-16 code unit.
#[derive(Default)]
struct Recorder {
    calls: RefCell<Vec<(String, String)>>,
}

impl TextMetricsProvider for Recorder {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        self.calls
            .borrow_mut()
            .push((text.to_owned(), font.to_owned()));
        text.encode_utf16().count() as f64 * 7.0
    }
}

impl Recorder {
    fn lines(&self) -> Vec<String> {
        self.calls.borrow().iter().map(|(t, _)| t.clone()).collect()
    }
}

/// Always 0, as a canvas without fonts would measure.
struct Zero;

impl TextMetricsProvider for Zero {
    fn get_line_width(&self, _text: &str, _font: &str) -> f64 {
        0.0
    }
}

const FONT: &str = "20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji";

#[test]
fn test_env_metric_is_ten_px_per_utf16_code_unit() {
    let m = CharCountTextMetrics;
    assert_eq!(m.get_line_width("", FONT), 0.0);
    assert_eq!(m.get_line_width("abc", FONT), 30.0);
    // An astral character is two code units, as `text.length` counts it.
    assert_eq!(m.get_line_width("a\u{1F600}", FONT), 30.0);
    assert_eq!(m.get_line_width("\u{4E2D}", FONT), 10.0);
}

#[test]
fn normalize_eol_and_tabs() {
    assert_eq!(normalize_eol("a\r\nb\rc\nd\r\r\n"), "a\nb\nc\nd\n\n");
    assert_eq!(normalize_text("a\tb\r\n\t"), "a        b\n        ");
}

#[test]
fn tab_measures_as_eight_spaces() {
    let r = Recorder::default();
    let width = get_text_width("a\tb", FONT, &r);
    assert_eq!(r.lines(), vec!["a        b"]);
    assert_eq!(width, 70.0);

    let store = common::store();
    let with_tab = measure_text("x\ty", FONT, 1.25, store);
    let with_spaces = measure_text("x        y", FONT, 1.25, store);
    assert_eq!(with_tab, with_spaces);
    let space = get_line_width(" ", FONT, store);
    assert!(space > 0.0);
    let x_y = get_line_width("xy", FONT, store);
    assert!((with_tab.width - (x_y + 8.0 * space)).abs() < 1e-9);
}

#[test]
fn empty_lines_measure_as_a_space() {
    let r = Recorder::default();
    let size = measure_text("", FONT, 1.25, &r);
    assert_eq!(r.lines(), vec![" "]);
    assert_eq!(
        size,
        TextDimensions {
            width: 7.0,
            height: 25.0
        }
    );

    let r = Recorder::default();
    let size = measure_text("\nab\n\n", FONT, 1.25, &r);
    assert_eq!(r.lines(), vec![" ", "ab", " ", " "]);
    assert_eq!(size.height, 100.0);
    assert_eq!(size.width, 14.0);

    // getTextWidth alone measures an empty line as "" (width 0).
    let r = Recorder::default();
    assert_eq!(get_text_width("", FONT, &r), 0.0);
    assert_eq!(r.lines(), vec![""]);

    let store = common::store();
    let empty = measure_text("", FONT, 1.25, store);
    assert_eq!(empty.width, get_line_width(" ", FONT, store));
    assert!(empty.width > 0.0);
    let blank_line = measure_text("\n", FONT, 1.25, store);
    assert_eq!(blank_line.width, empty.width);
    assert_eq!(blank_line.height, 50.0);
}

#[test]
fn measure_text_replaces_empty_lines_before_normalizing_eols() {
    // `text.split("\n")` runs first, so a "\r" line is not empty and is not
    // replaced; normalizing then turns it into an empty line measured as "".
    let r = Recorder::default();
    let size = measure_text("a\r\n\r\nb", FONT, 1.25, &r);
    assert_eq!(r.lines(), vec!["a", "", "b"]);
    assert_eq!(size.height, 75.0);
}

#[test]
fn width_is_the_widest_line_and_height_counts_lines() {
    let r = Recorder::default();
    let size = measure_text("ab\nabcd\nabc", FONT, 1.25, &r);
    assert_eq!(size.width, 28.0);
    assert_eq!(size.height, 75.0);
    assert!(r.calls.borrow().iter().all(|(_, f)| f == FONT));
    assert_eq!(get_text_height("a\nb", 36.0, 1.35), 36.0 * 1.35 * 2.0);
    assert_eq!(get_text_height("a\r\nb\rc", 10.0, 1.0), 30.0);
}

#[test]
fn height_uses_parse_float_of_the_font_string() {
    let r = Recorder::default();
    assert_eq!(
        measure_text("a", "16.5px Virgil", 1.25, &r).height,
        16.5 * 1.25
    );
    assert!(measure_text("a", "Virgil", 1.25, &r).height.is_nan());
}

#[test]
fn detect_line_height_upstream_case() {
    // textElement.test.ts:174-186: seven lines, fontSize 20, height 175.
    let text = "Excalidraw is a\nvirtual \nopensource \nwhiteboard for \nsketching \nhand-drawn like\ndiagrams";
    assert_eq!(detect_line_height(text, 175.0, 20.0), 1.25);
    assert_eq!(detect_line_height("a\r\nb", 50.0, 20.0), 1.25);
}

#[test]
fn approx_min_line_height() {
    assert_eq!(get_approx_min_line_height(20.0, 1.25), 35.0);
}

#[test]
fn char_width_cache() {
    let r = Recorder::default();
    let mut cache = CharWidthCache::new();
    assert_eq!(cache.get_cache(FONT), None);
    assert_eq!(get_min_char_width(FONT, &cache), 0.0);
    assert_eq!(get_max_char_width(FONT, &cache), 0.0);

    assert_eq!(cache.calculate("a", FONT, &r), 7.0);
    assert_eq!(cache.calculate("\u{1F600}", FONT, &r), 14.0);
    assert_eq!(cache.calculate("a", FONT, &r), 7.0);
    // Cached: measured once each.
    assert_eq!(r.lines(), vec!["a", "\u{1F600}"]);
    // Keyed by the first UTF-16 code unit (0x61, 0xD83D), in that order.
    assert_eq!(cache.get_cache(FONT), Some(vec![7.0, 14.0]));
    assert_eq!(get_min_char_width(FONT, &cache), 7.0);
    assert_eq!(get_max_char_width(FONT, &cache), 14.0);
    // Another font has its own cache.
    assert_eq!(cache.get_cache("10px Virgil"), None);

    // The empty string's key is NaN, which the array never lists.
    assert_eq!(cache.calculate("", FONT, &r), 0.0);
    assert_eq!(cache.get_cache(FONT), Some(vec![7.0, 14.0]));

    cache.clear_cache(FONT);
    assert_eq!(cache.get_cache(FONT), Some(vec![]));
    assert_eq!(get_min_char_width(FONT, &cache), f64::INFINITY);
    assert_eq!(get_max_char_width(FONT, &cache), f64::NEG_INFINITY);
}

#[test]
fn zero_widths_are_measured_again() {
    let z = Zero;
    let mut cache = CharWidthCache::new();
    let r = Recorder::default();
    assert_eq!(cache.calculate("a", FONT, &z), 0.0);
    // `!cached` is true for 0, so the next call measures again.
    assert_eq!(cache.calculate("a", FONT, &r), 7.0);
    assert_eq!(r.lines(), vec!["a"]);
}

#[test]
fn approx_min_line_width() {
    let r = Recorder::default();
    let mut cache = CharWidthCache::new();
    // Nothing cached: DUMMY_TEXT one character per line, widest + 2 * 5.
    assert_eq!(get_approx_min_line_width(FONT, 1.25, &r, &cache), 17.0);
    assert_eq!(r.lines().len(), DUMMY_TEXT.len());
    assert!(r.lines().iter().all(|l| l.chars().count() == 1));

    cache.calculate("ab", FONT, &r);
    assert_eq!(get_approx_min_line_width(FONT, 1.25, &r, &cache), 24.0);

    // A cleared cache is -Infinity wide, not 0: upstream then returns
    // -Infinity + 10.
    cache.clear_cache(FONT);
    assert_eq!(
        get_approx_min_line_width(FONT, 1.25, &r, &cache),
        f64::NEG_INFINITY
    );
}

#[test]
fn min_text_element_width_is_a_space_plus_padding() {
    let r = Recorder::default();
    assert_eq!(get_min_text_element_width(FONT, 1.25, &r), 17.0);
    let store = common::store();
    assert_eq!(
        get_min_text_element_width(FONT, 1.25, store),
        get_line_width(" ", FONT, store) + 10.0
    );
}

#[test]
fn measure_text_support() {
    let r = Recorder::default();
    assert!(is_measure_text_supported(&r));
    assert_eq!(
        r.calls.borrow().clone(),
        vec![(
            DUMMY_TEXT.to_owned(),
            get_font_string(20.0, FontFamily::EXCALIFONT)
        )]
    );
    assert!(!is_measure_text_supported(&Zero));
    assert!(is_measure_text_supported(common::store()));
    assert_eq!(DUMMY_TEXT, "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
}
