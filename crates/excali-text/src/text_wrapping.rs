//! Soft wrapping: `packages/element/src/textWrapping.ts`.

use crate::text_measurements::{CharWidthCache, TextMetricsProvider};

pub use crate::font_assets::contains_cjk;

/// `WrappedTextLine` (`textWrapping.ts:409-418`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedTextLine {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// `parseTokens(line)` (`textWrapping.ts:382-390`).
pub fn parse_tokens(_line: &str) -> Vec<String> {
    Vec::new()
}

/// `wrapText(text, font, maxWidth)` (`textWrapping.ts:392-406`).
pub fn wrap_text(
    _text: &str,
    _font: &str,
    _max_width: f64,
    _provider: &dyn TextMetricsProvider,
    _char_widths: &mut CharWidthCache,
) -> String {
    String::new()
}

/// `getWrappedTextLines(text, font, maxWidth)` (`textWrapping.ts:439-477`).
pub fn get_wrapped_text_lines(
    _text: &str,
    _font: &str,
    _max_width: f64,
    _provider: &dyn TextMetricsProvider,
    _char_widths: &mut CharWidthCache,
) -> Vec<WrappedTextLine> {
    Vec::new()
}
