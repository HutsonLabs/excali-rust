//! Text runs: one `fillText` call each.

use excali_core::json::number_to_string;

use super::paint::Color;

/// A CSS font: `font = "<size>px <family list>"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    /// The size in CSS pixels (user units).
    pub size: f64,
    /// The CSS family list, fallbacks included, as upstream's
    /// `getFontFamilyString` writes it (`common/src/utils.ts:123-136`).
    pub family: String,
}

impl Font {
    pub fn new(size: f64, family: impl Into<String>) -> Self {
        Self {
            size,
            family: family.into(),
        }
    }

    /// `` `${fontSize}px ${family}` ``, upstream's `getFontString`
    /// (`common/src/utils.ts:139-147`), the size printed as JavaScript
    /// prints numbers.
    pub fn css(&self) -> String {
        format!("{}px {}", number_to_string(self.size), self.family)
    }
}

/// `textAlign`: where `x` sits on the line. Upstream assigns the text
/// element's `textAlign` (`"left"`, `"center"` or `"right"`) and moves `x`
/// by 0, `w / 2` or `w` to match (`renderElement.ts:641-652`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn as_css(self) -> &'static str {
        match self {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        }
    }
}

/// The base direction for bidirectional text. Upstream sets the canvas's
/// `dir` attribute from `isRTL(text)` before drawing a text element
/// (`renderElement.ts:627-634`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn as_css(self) -> &'static str {
        match self {
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
        }
    }
}

/// One line of text: `fillText(text, x, y)` with `y` on the alphabetic
/// baseline (the canvas's default `textBaseline`, which upstream keeps;
/// the sticky-note footer sets it explicitly, `renderElement.ts:455-461`).
/// A multi-line text element is one run per line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font: Font,
    pub color: Color,
    pub align: TextAlign,
    pub direction: Direction,
}

impl TextRun {
    /// A left-aligned, left-to-right run.
    pub fn new(text: impl Into<String>, x: f64, y: f64, font: Font, color: Color) -> Self {
        Self {
            text: text.into(),
            x,
            y,
            font,
            color,
            align: TextAlign::default(),
            direction: Direction::default(),
        }
    }
}
