//! The font asset manifest: which range-split file holds which characters
//! of which family, and which of those files a scene needs.
//!
//! Upstream counterparts: `Fonts` (`packages/excalidraw/fonts/Fonts.ts`),
//! `ExcalidrawFontFace` (`fonts/ExcalidrawFontFace.ts`), the per-family
//! face lists (`fonts/<Family>/index.ts`), `fonts/fonts.css` for the UI font,
//! and `containsCJK` (`packages/element/src/textWrapping.ts:30-36`).
//!
//! The files are vendored under `crates/excali-text/assets/fonts/` with the
//! licence of each directory (ADR-004) and listed in `manifest.json` there;
//! [`registered_families`] is the same table compiled in. Both are written by
//! `scripts/fonts/assets.py` from upstream's registry at the pinned commit.
//!
//! A host fetches only the files a scene needs, as upstream does: every face
//! is registered with its `unicode-range` and nothing is downloaded until
//! `document.fonts.load(font, text)` asks for the characters in use
//! ([`scene_font_loads`], [`faces_to_load`]). SVG export inlines the faces
//! [`font_face_declarations`] picks.

use std::fmt;

use excali_core::element::{Element, ElementKind, FontFamily};

use crate::font_metadata::{
    get_font_family_fallbacks, get_font_family_string, get_font_string,
    CJK_HAND_DRAWN_FALLBACK_FONT,
};

#[rustfmt::skip]
mod generated;

/// A face with no `unicode-range` covers every code point: the CSS Font
/// Loading default for `FontFace.unicodeRange`.
pub const FULL_UNICODE_RANGE: &str = "U+0-10FFFF";

/// `ExcalidrawFontFace.ASSETS_FALLBACK_URL` (`ExcalidrawFontFace.ts:11-15`,
/// the app build's value, with no package name): every bundled face's last
/// url is its file under this directory, and `getContent` answers that url
/// when it cannot fetch the file (`:58-86`).
pub const ASSETS_FALLBACK_URL: &str = "https://esm.sh/@excalidraw/excalidraw/dist/prod/";

/// `FONT_SIZES.sm` (`packages/common/src/constants.ts:122-127`): the size
/// `fontFacesLoader` builds its font string with (`Fonts.ts:255-258`).
pub const LOAD_FONT_SIZE: f64 = 16.0;

/// The container format of a font file, as named in a CSS `format()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFormat {
    Woff2,
    /// Liberation Sans 2.1.5, shipped as released (ADR-004).
    TrueType,
}

impl FontFormat {
    /// The CSS `format()` string: `woff2` or `truetype`.
    pub const fn css(self) -> &'static str {
        match self {
            FontFormat::Woff2 => "woff2",
            FontFormat::TrueType => "truetype",
        }
    }
}

/// One font face: a file and the descriptors it is registered with
/// (`new FontFace(family, src, {display: "swap", style: "normal",
/// weight: "400", ...descriptors})`, `ExcalidrawFontFace.ts:17-30`).
#[derive(Debug, PartialEq, Eq)]
pub struct FontFaceAsset {
    /// The CSS family name the face registers under (`Fonts.init`'s key).
    pub family: &'static str,
    /// Path under `assets/fonts/`, e.g. `Excalifont/Excalifont-Regular-<hash>.woff2`.
    pub file: &'static str,
    pub format: FontFormat,
    /// The `unicodeRange` descriptor as upstream writes it; `None` for
    /// faces upstream registers without one (they cover every code point).
    pub unicode_range: Option<&'static str>,
    /// The ranges of the effective unicode-range, inclusive, in order.
    pub ranges: &'static [(u32, u32)],
    pub weight: &'static str,
    pub style: &'static str,
    pub display: &'static str,
    /// File size in bytes.
    pub size: u64,
    /// SHA-256 of the file, lowercase hex.
    pub sha256: &'static str,
    /// The upstream file this face stands for (path under
    /// `packages/excalidraw/fonts/`); equal to `file` except for the
    /// Liberation Sans substitution recorded in ADR-004.
    pub upstream_file: &'static str,
}

impl FontFaceAsset {
    /// `FontFace.unicodeRange`: the descriptor, or [`FULL_UNICODE_RANGE`].
    pub fn unicode_range_css(&self) -> &'static str {
        self.unicode_range.unwrap_or(FULL_UNICODE_RANGE)
    }

    /// Whether the face's unicode-range holds the code point.
    pub fn covers(&self, code_point: u32) -> bool {
        // Ranges are sorted within most faces but not all (the Google Fonts
        // ranges repeat code points), so scan; the longest list is short.
        self.ranges
            .iter()
            .any(|&(a, b)| code_point >= a && code_point <= b)
    }

    /// `getUnicodeRangeRegex().test(text)` (`ExcalidrawFontFace.ts:39-41`,
    /// `114-131`): whether any character of `text` is in the face's range.
    pub fn matches(&self, text: &str) -> bool {
        text.chars().any(|c| self.covers(u32::from(c)))
    }

    /// The face's upstream file under [`ASSETS_FALLBACK_URL`]: upstream's
    /// last url for it, which `getContent` answers when no url can be
    /// fetched, so an SVG export that cannot read the file still names it.
    pub fn fallback_url(&self) -> String {
        format!("{ASSETS_FALLBACK_URL}{}", self.upstream_file)
    }

    /// The CSS `src` for this face under `base_url` (the directory holding
    /// `assets/fonts/`'s contents), with a trailing slash added when
    /// missing as `normalizeBaseUrl` does (`ExcalidrawFontFace.ts:191-208`):
    /// `url("<base>/<file>") format("<format>")`.
    pub fn src(&self, base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        let sep = if base.is_empty() { "" } else { "/" };
        format!(
            "url(\"{base}{sep}{}\") format(\"{}\")",
            self.file,
            self.format.css()
        )
    }
}

/// One family of upstream's registry (`Fonts.registered`).
#[derive(Debug, PartialEq, Eq)]
pub struct RegisteredFamily {
    pub id: FontFamily,
    /// The family name the faces register under.
    pub family: &'static str,
    /// `FONT_METADATA[id].local`: the host's installed font (Helvetica,
    /// Segoe UI Emoji); no file, never registered or inlined
    /// (`Fonts.ts:225-229`, `301-304`).
    pub local: bool,
    pub faces: &'static [FontFaceAsset],
}

/// Every family of `Fonts.registered`, in upstream's order (`Fonts.ts:398-411`).
pub fn registered_families() -> &'static [RegisteredFamily] {
    &generated::REGISTERED
}

/// `Fonts.registered.get(id)`.
pub fn registered_family(id: FontFamily) -> Option<&'static RegisteredFamily> {
    registered_families().iter().find(|f| f.id == id)
}

/// The registered family whose faces use this CSS family name (exact, as
/// `document.fonts` matches the names of a font's family list).
pub fn registered_family_named(name: &str) -> Option<&'static RegisteredFamily> {
    registered_families().iter().find(|f| f.family == name)
}

/// The UI font faces (Assistant 400, 500, 600 and 700, `fonts/fonts.css:5-35`),
/// loaded by the stylesheet rather than per scene.
pub fn ui_font_faces() -> &'static [FontFaceAsset] {
    &generated::UI_FACES
}

/// A CSS `unicode-range` value that upstream's `getUnicodeRangeRegex` cannot
/// turn into a character class (its `RegExp` would throw).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnicodeRangeError {
    part: String,
}

impl fmt::Display for UnicodeRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid unicode-range part {:?}", self.part)
    }
}

impl std::error::Error for UnicodeRangeError {}

/// The ranges of a unicode-range as `getUnicodeRangeRegex` reads it
/// (`ExcalidrawFontFace.ts:114-131`): split on `,` and the whitespace after
/// it, drop the first `U+`, then a hex start and an optional `-` hex end.
/// Anything its regex would reject (an empty or non-hex bound, a wildcard,
/// a code point past U+10FFFF, an end before its start) is an error.
pub fn parse_unicode_range(value: &str) -> Result<Vec<(u32, u32)>, UnicodeRangeError> {
    let mut out = Vec::new();
    let mut rest = value;
    loop {
        let (part, next) = match rest.find(',') {
            Some(i) => (&rest[..i], Some(rest[i + 1..].trim_start())),
            None => (rest, None),
        };
        out.push(parse_range_part(part)?);
        match next {
            Some(n) => rest = n,
            None => return Ok(out),
        }
    }
}

fn parse_range_part(part: &str) -> Result<(u32, u32), UnicodeRangeError> {
    let err = || UnicodeRangeError {
        part: part.to_owned(),
    };
    let body = part.replacen("U+", "", 1);
    let mut bounds = body.split('-');
    let hex = |s: Option<&str>| -> Result<u32, UnicodeRangeError> {
        let s = s.ok_or_else(err)?;
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let v = u32::from_str_radix(s, 16).map_err(|_| err())?;
        if v > 0x10FFFF {
            return Err(err());
        }
        Ok(v)
    };
    let start = hex(bounds.next())?;
    let end = match bounds.next() {
        Some(e) => hex(Some(e))?,
        None => start,
    };
    if end < start {
        return Err(err());
    }
    Ok((start, end))
}

/// The code points `containsCJK` accepts (`textWrapping.ts:30-36`, `89-120`:
/// Han, Hiragana, Katakana and Hangul script, plus the CJK symbols and
/// punctuation upstream lists), as inclusive ranges. Taken from upstream's
/// function on every code point under Node 26.10 (Unicode 17); the test
/// suite checks it against `tests/fixtures/font-assets.json`.
#[rustfmt::skip]
pub const CJK_RANGES: [(u32, u32); 63] = [
    (0x1100, 0x11FF), (0x2025, 0x2025), (0x2E80, 0x2E99), (0x2E9B, 0x2EF3), (0x2F00, 0x2FD5),
    (0x3001, 0x3003), (0x3005, 0x3012), (0x3014, 0x301F), (0x3021, 0x3029), (0x302E, 0x3030),
    (0x3038, 0x303B), (0x3041, 0x3096), (0x309D, 0x309F), (0x30A1, 0x30FF), (0x3131, 0x318E),
    (0x31F0, 0x321E), (0x3260, 0x327E), (0x32D0, 0x32FE), (0x3300, 0x3357), (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF), (0xA960, 0xA97C), (0xAC00, 0xD7A3), (0xD7B0, 0xD7C6), (0xD7CB, 0xD7FB),
    (0xF900, 0xFA6D), (0xFA70, 0xFAD9), (0xFF01, 0xFF01), (0xFF03, 0xFF04), (0xFF06, 0xFF0F),
    (0xFF1A, 0xFF1F), (0xFF3B, 0xFF3E), (0xFF40, 0xFF40), (0xFF5B, 0xFF5D), (0xFF5F, 0xFF60),
    (0xFF62, 0xFF63), (0xFF66, 0xFF6F), (0xFF71, 0xFF9D), (0xFFA0, 0xFFBE), (0xFFC2, 0xFFC7),
    (0xFFCA, 0xFFCF), (0xFFD2, 0xFFD7), (0xFFDA, 0xFFDC), (0xFFE0, 0xFFE6), (0x16FE2, 0x16FE3),
    (0x16FF0, 0x16FF6), (0x1AFF0, 0x1AFF3), (0x1AFF5, 0x1AFFB), (0x1AFFD, 0x1AFFE),
    (0x1B000, 0x1B122), (0x1B132, 0x1B132), (0x1B150, 0x1B152), (0x1B155, 0x1B155),
    (0x1B164, 0x1B167), (0x1F200, 0x1F200), (0x20000, 0x2A6DF), (0x2A700, 0x2B81D),
    (0x2B820, 0x2CEAD), (0x2CEB0, 0x2EBE0), (0x2EBF0, 0x2EE5D), (0x2F800, 0x2FA1D),
    (0x30000, 0x3134A), (0x31350, 0x33479),
];

/// `containsCJK(text)` (`textWrapping.ts:30-36`).
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        let cp = u32::from(c);
        CJK_RANGES
            .binary_search_by(|&(a, b)| {
                if b < cp {
                    std::cmp::Ordering::Less
                } else if a > cp {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    })
}

/// What `fontFacesLoader` asks the browser for, for one family of the
/// elements (`Fonts.ts:249-284`): `document.fonts.load(font, text)`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontLoad {
    pub font_family: FontFamily,
    /// `getFontString({fontFamily, fontSize: FONT_SIZES.sm})`: the family
    /// followed by its fallbacks.
    pub font: String,
    /// `getCharacters(charsPerFamily, fontFamily)`: the family's distinct
    /// characters in first-use order.
    pub text: String,
    /// The faces that load fetches ([`matching_faces`] of `font` and `text`).
    pub faces: Vec<&'static FontFaceAsset>,
}

fn text_fields(element: &Element) -> Option<(FontFamily, &str)> {
    match &element.kind {
        ElementKind::Text(t) => Some((t.font_family, t.original_text.as_str())),
        _ => None,
    }
}

/// `getUniqueFamilies` and `getCharsPerFamily` / `getCharacters`
/// (`Fonts.ts:421-470`): each text element's family in first-use order,
/// with the distinct characters of its elements' `originalText`.
fn chars_per_family<'a>(
    elements: impl IntoIterator<Item = &'a Element>,
) -> Vec<(FontFamily, String)> {
    let mut out: Vec<(FontFamily, String)> = Vec::new();
    for (family, text) in elements.into_iter().filter_map(text_fields) {
        let i = match out.iter().position(|(f, _)| *f == family) {
            Some(i) => i,
            None => {
                out.push((family, String::new()));
                out.len() - 1
            }
        };
        let chars = &mut out[i].1;
        for c in text.chars() {
            if !chars.contains(c) {
                chars.push(c);
            }
        }
    }
    out
}

/// The font faces `document.fonts.load(font, text)` fetches once every
/// non-local registered face is in `document.fonts` (`Fonts.ts:224-236`):
/// for each family of the font's family list, in order, the registered faces
/// whose unicode-range holds at least one character of `text` (CSS Font
/// Loading, "find the matching font faces"). Generic families and local or
/// unregistered names match nothing; an empty text matches nothing.
pub fn matching_faces(family_list: &str, text: &str) -> Vec<&'static FontFaceAsset> {
    let mut out: Vec<&'static FontFaceAsset> = Vec::new();
    for name in family_list.split(", ") {
        let Some(family) = registered_family_named(name) else {
            continue;
        };
        if family.local {
            continue;
        }
        for face in family.faces {
            if face.matches(text) && !out.iter().any(|f| std::ptr::eq(*f, face)) {
                out.push(face);
            }
        }
    }
    out
}

fn font_loads<'a>(elements: impl IntoIterator<Item = &'a Element>) -> Vec<FontLoad> {
    chars_per_family(elements)
        .into_iter()
        .map(|(font_family, text)| {
            let faces = matching_faces(&get_font_family_string(font_family), &text);
            FontLoad {
                font_family,
                font: get_font_string(LOAD_FONT_SIZE, font_family),
                text,
                faces,
            }
        })
        .collect()
}

/// `Fonts.loadElementsFonts(elements)` (`Fonts.ts:169-177`): one load per
/// family of the text elements, deleted ones included (the caller passes
/// what it exports).
pub fn elements_font_loads(elements: &[Element]) -> Vec<FontLoad> {
    font_loads(elements)
}

/// `fonts.loadSceneFonts()` (`Fonts.ts:153-164`): [`elements_font_loads`]
/// over the scene's non-deleted elements.
pub fn scene_font_loads(elements: &[Element]) -> Vec<FontLoad> {
    font_loads(elements.iter().filter(|e| !e.base.is_deleted))
}

/// Every face the loads fetch, each once, in load order.
pub fn faces_to_load(loads: &[FontLoad]) -> Vec<&'static FontFaceAsset> {
    let mut out: Vec<&'static FontFaceAsset> = Vec::new();
    for face in loads.iter().flat_map(|l| l.faces.iter().copied()) {
        if !out.iter().any(|f| std::ptr::eq(*f, face)) {
            out.push(face);
        }
    }
    out
}

/// One `@font-face` rule SVG export inlines: the face, subset to the
/// characters of its family (`ExcalidrawFontFace.toCSS`, `:37-51`).
#[derive(Debug, Clone, PartialEq)]
pub struct FontFaceDeclaration {
    pub face: &'static FontFaceAsset,
    /// All characters of the family, the code points upstream subsets to.
    pub characters: String,
}

/// The faces `Fonts.generateFontFaceDeclarations(elements)` inlines, in its
/// order (`Fonts.ts:182-217`, `286-330`): Xiaolai first, with Excalifont's
/// characters, when Excalifont text contains CJK; then each family of the
/// elements in first-use order, skipping unregistered and local ones; within
/// a family, its faces in registry order whose range holds a character.
pub fn font_face_declarations(elements: &[Element]) -> Vec<FontFaceDeclaration> {
    let mut families = chars_per_family(elements);
    // "for simplicity, assuming we have just one family with the CJK
    // handdrawn fallback"
    let with_cjk = families
        .iter()
        .find(|(f, _)| get_font_family_fallbacks(*f).contains(&CJK_HAND_DRAWN_FALLBACK_FONT))
        .map(|(_, chars)| chars.clone());
    if let Some(chars) = with_cjk {
        if contains_cjk(&chars) {
            families.retain(|(f, _)| *f != FontFamily::XIAOLAI);
            families.insert(0, (FontFamily::XIAOLAI, chars));
        }
    }
    let mut out: Vec<FontFaceDeclaration> = Vec::new();
    for (family, chars) in &families {
        let Some(registered) = registered_family(*family) else {
            continue;
        };
        if registered.local {
            continue;
        }
        for face in registered.faces {
            if face.matches(chars) {
                let decl = FontFaceDeclaration {
                    face,
                    characters: chars.clone(),
                };
                // `Array.from(new Set(fontFaces))`
                if !out.contains(&decl) {
                    out.push(decl);
                }
            }
        }
    }
    out
}
