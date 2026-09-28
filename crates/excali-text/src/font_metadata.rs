//! Font metrics per family and the baseline formula text rendering uses.
//!
//! Upstream: `FONT_METADATA`, `GOOGLE_FONTS_RANGES`, `LOCAL_FONT_PROTOCOL`,
//! `getVerticalOffset` and `getLineHeight`
//! (`packages/common/src/font-metadata.ts:35-181`); `getLineHeightInPx`
//! (`packages/element/src/textMeasurements.ts:91-96`); the fallback font
//! names and `getGenericFontFamilyFallback` / `getFontFamilyFallbacks`
//! (`packages/common/src/constants.ts:129-197`); `getFontFamilyString` and
//! `getFontString` (`packages/common/src/utils.ts:123-147`).
//!
//! Family ids are excali-core's [`FontFamily`] (`FONT_FAMILY`,
//! `FONT_FAMILY_FALLBACKS`, `constants.ts:140-167`).

use excali_core::element::FontFamily;
use excali_core::json::number_to_string;

/// Head and hhea metrics of a font, with its unitless line height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    /// `head.unitsPerEm`: 1000, 1024 or 2048.
    pub units_per_em: u16,
    /// `hhea.ascender`, in font units.
    pub ascender: f64,
    /// `hhea.descender`, in font units (negative).
    pub descender: f64,
    /// The hard-coded unitless line height upstream gives new text of this
    /// family.
    pub line_height: f64,
}

/// One `FONT_METADATA` entry: metrics and the family's flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetadata {
    pub metrics: FontMetrics,
    /// A deprecated family (kept for old scenes; badged in the picker).
    pub deprecated: bool,
    /// A family users cannot pick (not shown in the font picker).
    pub private: bool,
    /// A local-only family: never registered or inlined as a font face.
    pub local: bool,
    /// A fallback family, never an element's own.
    pub fallback: bool,
}

const fn metrics(
    units_per_em: u16,
    ascender: f64,
    descender: f64,
    line_height: f64,
) -> FontMetrics {
    FontMetrics {
        units_per_em,
        ascender,
        descender,
        line_height,
    }
}

const fn entry(metrics: FontMetrics) -> FontMetadata {
    FontMetadata {
        metrics,
        deprecated: false,
        private: false,
        local: false,
        fallback: false,
    }
}

const EXCALIFONT_METRICS: FontMetrics = metrics(1000, 886.0, -374.0, 1.25);

/// `FONT_METADATA` (`font-metadata.ts:35-134`): the nine element families
/// and the Xiaolai and Segoe UI Emoji fallbacks, in the order JavaScript
/// enumerates the record's integer keys (ascending). The generic fallbacks
/// (sans-serif 998, monospace 999) have no entry.
pub const FONT_METADATA: [(FontFamily, FontMetadata); 11] = [
    (
        FontFamily::VIRGIL,
        FontMetadata {
            deprecated: true,
            ..entry(metrics(1000, 886.0, -374.0, 1.25))
        },
    ),
    (
        FontFamily::HELVETICA,
        FontMetadata {
            deprecated: true,
            local: true,
            ..entry(metrics(2048, 1577.0, -471.0, 1.15))
        },
    ),
    (
        FontFamily::CASCADIA,
        FontMetadata {
            deprecated: true,
            ..entry(metrics(2048, 1900.0, -480.0, 1.2))
        },
    ),
    (FontFamily::EXCALIFONT, entry(EXCALIFONT_METRICS)),
    (
        FontFamily::NUNITO,
        entry(metrics(1000, 1011.0, -353.0, 1.25)),
    ),
    (
        FontFamily::LILITA_ONE,
        entry(metrics(1000, 923.0, -220.0, 1.15)),
    ),
    (
        FontFamily::COMIC_SHANNS,
        entry(metrics(1000, 750.0, -250.0, 1.25)),
    ),
    (
        FontFamily::LIBERATION_SANS,
        FontMetadata {
            private: true,
            ..entry(metrics(2048, 1854.0, -434.0, 1.15))
        },
    ),
    (
        FontFamily::ASSISTANT,
        FontMetadata {
            private: true,
            ..entry(metrics(2048, 1021.0, -287.0, 1.25))
        },
    ),
    (
        FontFamily::XIAOLAI,
        FontMetadata {
            fallback: true,
            ..entry(metrics(1000, 880.0, -144.0, 1.25))
        },
    ),
    (
        FontFamily::SEGOE_UI_EMOJI,
        FontMetadata {
            // Upstream reuses Excalifont's metrics.
            local: true,
            fallback: true,
            ..entry(EXCALIFONT_METRICS)
        },
    ),
];

/// `FONT_METADATA[fontFamily]`: `None` for ids without an entry (4, the
/// generic fallbacks, custom ids).
pub fn font_metadata(family: FontFamily) -> Option<&'static FontMetadata> {
    FONT_METADATA
        .iter()
        .find(|(f, _)| *f == family)
        .map(|(_, meta)| meta)
}

/// `FONT_METADATA[fontFamily]?.metrics || FONT_METADATA[Excalifont].metrics`
/// (`font-metadata.ts:160-162`, `:176-178`): the family's metrics, or
/// Excalifont's for a family without an entry.
pub fn font_metrics(family: FontFamily) -> &'static FontMetrics {
    match font_metadata(family) {
        Some(meta) => &meta.metrics,
        None => &FONT_METADATA[3].1.metrics,
    }
}

/// `getVerticalOffset(fontFamily, fontSize, lineHeightPx)`
/// (`font-metadata.ts:155-170`): the distance from the top of a line box to
/// the alphabetic baseline. The line's leftover height (line height minus
/// the scaled ascent and descent) is split evenly above and below.
///
/// The arithmetic is upstream's, in upstream's order, so the result is the
/// same double.
pub fn get_vertical_offset(family: FontFamily, font_size: f64, line_height_px: f64) -> f64 {
    let m = font_metrics(family);
    let font_size_em = font_size / f64::from(m.units_per_em);
    let line_gap = (line_height_px - font_size_em * m.ascender + font_size_em * m.descender) / 2.0;
    font_size_em * m.ascender + line_gap
}

/// `getLineHeight(fontFamily)` (`font-metadata.ts:175-181`): the family's
/// unitless line height, Excalifont's (1.25) for a family without an entry.
pub fn get_line_height(family: FontFamily) -> f64 {
    font_metrics(family).line_height
}

/// `getLineHeightInPx(fontSize, lineHeight)`
/// (`packages/element/src/textMeasurements.ts:91-96`): `fontSize *
/// lineHeight`, the W3C definition of a unitless line height.
pub fn get_line_height_in_px(font_size: f64, line_height: f64) -> f64 {
    font_size * line_height
}

/// The unicode ranges Google Fonts splits families by (`GOOGLE_FONTS_RANGES`,
/// `font-metadata.ts:137-147`), as CSS `unicode-range` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoogleFontsRanges {
    pub latin: &'static str,
    pub latin_ext: &'static str,
    pub cyrilic_ext: &'static str,
    pub cyrilic: &'static str,
    pub vietnamese: &'static str,
}

impl GoogleFontsRanges {
    /// The ranges under upstream's keys (spelling kept), in upstream's order.
    pub const fn entries(&self) -> [(&'static str, &'static str); 5] {
        [
            ("LATIN", self.latin),
            ("LATIN_EXT", self.latin_ext),
            ("CYRILIC_EXT", self.cyrilic_ext),
            ("CYRILIC", self.cyrilic),
            ("VIETNAMESE", self.vietnamese),
        ]
    }
}

/// `GOOGLE_FONTS_RANGES` (`font-metadata.ts:137-147`).
pub const GOOGLE_FONTS_RANGES: GoogleFontsRanges = GoogleFontsRanges {
    latin: "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+2074, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD",
    latin_ext: "U+0100-02AF, U+0304, U+0308, U+0329, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF",
    cyrilic_ext: "U+0460-052F, U+1C80-1C88, U+20B4, U+2DE0-2DFF, U+A640-A69F, U+FE2E-FE2F",
    cyrilic: "U+0301, U+0400-045F, U+0490-0491, U+04B0-04B1, U+2116",
    vietnamese: "U+0102-0103, U+0110-0111, U+0128-0129, U+0168-0169, U+01A0-01A1, U+01AF-01B0, U+0300-0301, U+0303-0304, U+0308-0309, U+0323, U+0329, U+1EA0-1EF9, U+20AB",
};

/// `LOCAL_FONT_PROTOCOL` (`font-metadata.ts:150`): the URL scheme of a
/// local-only font face, which is neither registered nor inlined.
pub const LOCAL_FONT_PROTOCOL: &str = "local:";

/// `CJK_HAND_DRAWN_FALLBACK_FONT` (`constants.ts:129`).
pub const CJK_HAND_DRAWN_FALLBACK_FONT: &str = "Xiaolai";
/// `WINDOWS_EMOJI_FALLBACK_FONT` (`constants.ts:130`).
pub const WINDOWS_EMOJI_FALLBACK_FONT: &str = "Segoe UI Emoji";
/// `SANS_SERIF_GENERIC_FONT` (`constants.ts:155`).
pub const SANS_SERIF_GENERIC_FONT: &str = "sans-serif";
/// `MONOSPACE_GENERIC_FONT` (`constants.ts:156`).
pub const MONOSPACE_GENERIC_FONT: &str = "monospace";

/// `getGenericFontFamilyFallback(fontFamily)` (`constants.ts:169-179`):
/// monospace for Cascadia and Comic Shanns, sans-serif otherwise.
pub fn get_generic_font_family_fallback(family: FontFamily) -> &'static str {
    match family {
        FontFamily::CASCADIA | FontFamily::COMIC_SHANNS => MONOSPACE_GENERIC_FONT,
        _ => SANS_SERIF_GENERIC_FONT,
    }
}

/// `getFontFamilyFallbacks(fontFamily)` (`constants.ts:182-197`): Xiaolai
/// (Excalifont only), the generic fallback, then Segoe UI Emoji.
pub fn get_font_family_fallbacks(family: FontFamily) -> &'static [&'static str] {
    const EXCALIFONT: [&str; 3] = [
        CJK_HAND_DRAWN_FALLBACK_FONT,
        SANS_SERIF_GENERIC_FONT,
        WINDOWS_EMOJI_FALLBACK_FONT,
    ];
    const SANS: [&str; 2] = [SANS_SERIF_GENERIC_FONT, WINDOWS_EMOJI_FALLBACK_FONT];
    const MONO: [&str; 2] = [MONOSPACE_GENERIC_FONT, WINDOWS_EMOJI_FALLBACK_FONT];
    if family == FontFamily::EXCALIFONT {
        return &EXCALIFONT;
    }
    if get_generic_font_family_fallback(family) == MONOSPACE_GENERIC_FONT {
        &MONO
    } else {
        &SANS
    }
}

/// `getFontFamilyString({ fontFamily })` (`utils.ts:123-136`): the CSS
/// family list, the family's name followed by its fallbacks, for the nine
/// `FONT_FAMILY` families; `"Segoe UI Emoji"` for any other id (fallback
/// ids included, as upstream only searches `FONT_FAMILY`).
pub fn get_font_family_string(family: FontFamily) -> String {
    if !FontFamily::ELEMENT_FAMILIES.contains(&family) {
        return WINDOWS_EMOJI_FALLBACK_FONT.to_owned();
    }
    let mut out = family.name().unwrap_or_default().to_owned();
    for fallback in get_font_family_fallbacks(family) {
        out.push_str(", ");
        out.push_str(fallback);
    }
    out
}

/// `getFontString({ fontSize, fontFamily })` (`utils.ts:139-147`):
/// `` `${fontSize}px ${getFontFamilyString(...)}` ``, the size printed as
/// JavaScript prints numbers.
pub fn get_font_string(font_size: f64, family: FontFamily) -> String {
    format!(
        "{}px {}",
        number_to_string(font_size),
        get_font_family_string(family)
    )
}
