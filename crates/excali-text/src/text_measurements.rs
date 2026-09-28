//! Text measurement: line splitting, width and height, and the pluggable
//! line-width provider.
//!
//! Upstream: `packages/element/src/textMeasurements.ts` (the whole module)
//! and `normalizeEOL` (`packages/common/src/utils.ts:1038-1040`).
//!
//! Upstream keeps the metrics provider and the per-character width cache in
//! module globals (`setCustomTextMetricsProvider`, `charWidth`); the port
//! passes them explicitly. The default provider upstream is the browser's
//! `canvas.measureText(line).width`; the port's is
//! [`crate::font_store::FontStore`], which measures advance widths from the
//! vendored font files. [`CharCountTextMetrics`] is the metric upstream's
//! own tests run under.

use std::collections::{BTreeMap, HashMap};

use excali_core::constants::{BOUND_TEXT_PADDING, DEFAULT_FONT_SIZE};
use excali_core::element::FontFamily;
use excali_core::json::parse_float;

use crate::font_metadata::{get_font_string, get_line_height_in_px};

/// `TextMetricsProvider` (`textMeasurements.ts:117-119`): the advance width
/// of one line of text in a CSS font string (`"20px Excalifont, Xiaolai,
/// sans-serif, Segoe UI Emoji"`, from [`get_font_string`]).
pub trait TextMetricsProvider {
    fn get_line_width(&self, text: &str, font: &str) -> f64;
}

impl<T: TextMetricsProvider + ?Sized> TextMetricsProvider for &T {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        (**self).get_line_width(text, font)
    }
}

/// The metric upstream's tests measure with: `CanvasTextMetricsProvider`
/// under `isTestEnv()` (`textMeasurements.ts:142-146`) returns the mocked
/// `measureText(text).width`, which is `text.length` (UTF-16 code units),
/// times 10.
#[derive(Clone, Copy, Debug, Default)]
pub struct CharCountTextMetrics;

impl TextMetricsProvider for CharCountTextMetrics {
    fn get_line_width(&self, text: &str, _font: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

/// `{ width, height }` from [`measure_text`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextDimensions {
    pub width: f64,
    pub height: f64,
}

/// `measureText(text, font, lineHeight)` (`textMeasurements.ts:12-27`).
/// Empty lines (split on `"\n"` only) are measured as `" "`, so a leading
/// or trailing empty line still counts; the height is
/// `parseFloat(font) * lineHeight * lines`.
pub fn measure_text(
    text: &str,
    font: &str,
    line_height: f64,
    provider: &dyn TextMetricsProvider,
) -> TextDimensions {
    let text = text
        .split('\n')
        .map(|line| if line.is_empty() { " " } else { line })
        .collect::<Vec<_>>()
        .join("\n");
    let font_size = parse_float(font);
    let height = get_text_height(&text, font_size, line_height);
    let width = get_text_width(&text, font, provider);
    TextDimensions { width, height }
}

/// `DUMMY_TEXT` (`textMeasurements.ts:29`).
pub const DUMMY_TEXT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// `getApproxMinLineWidth(font, lineHeight)` (`textMeasurements.ts:32-44`):
/// the widest character measured so far in `font` plus the padding on both
/// sides, or, before any character is cached, the widest of
/// [`DUMMY_TEXT`]'s characters.
pub fn get_approx_min_line_width(
    font: &str,
    line_height: f64,
    provider: &dyn TextMetricsProvider,
    char_widths: &CharWidthCache,
) -> f64 {
    let max_char_width = get_max_char_width(font, char_widths);
    if max_char_width == 0.0 {
        let one_per_line = DUMMY_TEXT
            .chars()
            .map(String::from)
            .collect::<Vec<_>>()
            .join("\n");
        return measure_text(&one_per_line, font, line_height, provider).width
            + BOUND_TEXT_PADDING * 2.0;
    }
    max_char_width + BOUND_TEXT_PADDING * 2.0
}

/// `getMinTextElementWidth(font, lineHeight)` (`textMeasurements.ts:46-51`):
/// the width of an empty text (measured as a space) plus the padding.
pub fn get_min_text_element_width(
    font: &str,
    line_height: f64,
    provider: &dyn TextMetricsProvider,
) -> f64 {
    measure_text("", font, line_height, provider).width + BOUND_TEXT_PADDING * 2.0
}

/// `isMeasureTextSupported()` (`textMeasurements.ts:53-62`): whether
/// [`DUMMY_TEXT`] in the default font measures wider than zero.
pub fn is_measure_text_supported(provider: &dyn TextMetricsProvider) -> bool {
    let font = get_font_string(DEFAULT_FONT_SIZE, FontFamily::DEFAULT);
    get_text_width(DUMMY_TEXT, &font, provider) > 0.0
}

/// `normalizeEOL(str)` (`utils.ts:1038-1040`): `\r\n` and lone `\r`
/// become `\n`.
pub fn normalize_eol(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// `normalizeText(text)` (`textMeasurements.ts:64-70`): EOLs normalized and
/// each tab replaced with eight spaces.
pub fn normalize_text(text: &str) -> String {
    normalize_eol(text).replace('\t', "        ")
}

fn split_into_lines(text: &str) -> Vec<String> {
    normalize_text(text)
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// `detectLineHeight(textElement)` (`textMeasurements.ts:76-85`): the
/// unitless line height of an element saved before `lineHeight` existed,
/// `height / lines / fontSize`.
pub fn detect_line_height(text: &str, height: f64, font_size: f64) -> f64 {
    let line_count = split_into_lines(text).len() as f64;
    height / line_count / font_size
}

/// `getApproxMinLineHeight(fontSize, lineHeight)`
/// (`textMeasurements.ts:98-104`): one line plus the padding on both sides.
pub fn get_approx_min_line_height(font_size: f64, line_height: f64) -> f64 {
    get_line_height_in_px(font_size, line_height) + BOUND_TEXT_PADDING * 2.0
}

/// `getLineWidth(text, font)` (`textMeasurements.ts:152-158`).
pub fn get_line_width(text: &str, font: &str, provider: &dyn TextMetricsProvider) -> f64 {
    provider.get_line_width(text, font)
}

/// `getTextWidth(text, font)` (`textMeasurements.ts:160-168`): the widest
/// line after [`normalize_text`]; an empty line measures 0 here.
pub fn get_text_width(text: &str, font: &str, provider: &dyn TextMetricsProvider) -> f64 {
    let mut width: f64 = 0.0;
    for line in split_into_lines(text) {
        width = js_max(width, get_line_width(&line, font, provider));
    }
    width
}

/// `getTextHeight(text, fontSize, lineHeight)` (`textMeasurements.ts:170-177`).
pub fn get_text_height(text: &str, font_size: f64, line_height: f64) -> f64 {
    let line_count = split_into_lines(text).len() as f64;
    get_line_height_in_px(font_size, line_height) * line_count
}

/// `Math.max(a, b)`: `NaN` if either is.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// `charWidth` (`textMeasurements.ts:179-210`): per font string, the width
/// of each character measured so far, keyed by its first UTF-16 code unit
/// (`char.charCodeAt(0)`).
#[derive(Clone, Debug, Default)]
pub struct CharWidthCache {
    fonts: HashMap<String, FontCache>,
}

#[derive(Clone, Debug, Default)]
struct FontCache {
    /// Array indices: the code units measured.
    widths: BTreeMap<u16, f64>,
    /// `cache[NaN]`: the empty string's `charCodeAt(0)` is `NaN`, a
    /// property the array methods never see.
    nan: Option<f64>,
}

impl CharWidthCache {
    pub fn new() -> CharWidthCache {
        CharWidthCache::default()
    }

    /// `charWidth.calculate(char, font)`: the cached width, measuring and
    /// caching it first when there is none or it is 0 (upstream tests the
    /// cached value for truthiness).
    pub fn calculate(&mut self, ch: &str, font: &str, provider: &dyn TextMetricsProvider) -> f64 {
        let cache = self.fonts.entry(font.to_owned()).or_default();
        let slot = match ch.encode_utf16().next() {
            Some(unit) => cache.widths.get(&unit).copied(),
            None => cache.nan,
        };
        match slot {
            Some(width) if width != 0.0 && !width.is_nan() => width,
            _ => {
                let width = get_line_width(ch, font, provider);
                match ch.encode_utf16().next() {
                    Some(unit) => {
                        cache.widths.insert(unit, width);
                    }
                    None => cache.nan = Some(width),
                }
                width
            }
        }
    }

    /// `charWidth.getCache(font)`: the cached widths in code-unit order, or
    /// `None` if nothing was ever measured or cleared for `font`.
    pub fn get_cache(&self, font: &str) -> Option<Vec<f64>> {
        self.fonts
            .get(font)
            .map(|c| c.widths.values().copied().collect())
    }

    /// `charWidth.clearCache(font)`: an empty cache for `font` (which
    /// [`CharWidthCache::get_cache`] then returns as empty, not `None`).
    pub fn clear_cache(&mut self, font: &str) {
        self.fonts.insert(font.to_owned(), FontCache::default());
    }
}

/// `getMinCharWidth(font)` (`textMeasurements.ts:212-220`): 0 when nothing
/// is cached for `font`, `Infinity` when its cache is empty.
pub fn get_min_char_width(font: &str, char_widths: &CharWidthCache) -> f64 {
    match char_widths.get_cache(font) {
        None => 0.0,
        Some(widths) => widths.into_iter().fold(f64::INFINITY, js_min),
    }
}

/// `getMaxCharWidth(font)` (`textMeasurements.ts:222-229`): 0 when nothing
/// is cached for `font`, `-Infinity` when its cache is empty.
pub fn get_max_char_width(font: &str, char_widths: &CharWidthCache) -> f64 {
    match char_widths.get_cache(font) {
        None => 0.0,
        Some(widths) => widths.into_iter().fold(f64::NEG_INFINITY, js_max),
    }
}

/// `Math.min(a, b)`: `NaN` if either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
