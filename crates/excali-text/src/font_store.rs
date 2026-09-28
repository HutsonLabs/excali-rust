//! Advance-width measurement from font files: the port's replacement for
//! the browser's `canvas.measureText(line).width`.
//!
//! Upstream measures a line with `CanvasTextMetricsProvider.getLineWidth`
//! (`packages/element/src/textMeasurements.ts:121-150`): the canvas `font`
//! is set to the CSS font string from `getFontString`
//! (`"20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"`,
//! `packages/common/src/utils.ts:123-147`) and the result is the advance
//! width of the shaped line. The browser picks a face for each character by
//! walking that family list; within a family registered as several
//! range-split files (`packages/excalidraw/fonts/*/index.ts`), it uses the
//! face whose `unicode-range` contains the character and whose `cmap` maps
//! it, the last registered face first where ranges overlap (CSS Fonts 4,
//! "unicode-range" and "font matching"). Consecutive characters on the same
//! face are shaped together (HarfBuzz in Chromium, [`rustybuzz`] here, with
//! the default features: kerning, ligatures, contextual alternates and mark
//! positioning), and the line's width is the sum of the glyph advances
//! scaled by `size / unitsPerEm`.
//!
//! [`FontStore`] does the same with the faces it is given, normally every
//! vendored file of [`crate::font_faces::FONT_FACES`]
//! ([`FontStore::with_vendored_faces`]). What the browser would take from
//! the host is mapped as follows:
//! - `local:` families (Helvetica, Segoe UI Emoji) have no faces, so their
//!   characters fall through to the next family in the list, as they do in
//!   a browser without those fonts installed;
//! - the generic `sans-serif` resolves to Liberation Sans (metric-compatible
//!   with Arial and Helvetica, and the face upstream substitutes for
//!   Helvetica when exporting without a browser, `fonts/Fonts.ts:405-406`),
//!   and `monospace` to Cascadia; [`FontStore::set_generic_family`] changes
//!   either;
//! - a character no face in the list maps is measured with the first
//!   family's face (its `.notdef` advance), the browser's last-resort system
//!   font being unknowable. Combining marks and default-ignorable
//!   characters stay on the preceding character's face when it maps them or
//!   no face does, so a base and its marks shape together.
//!
//! An invalid font string leaves the canvas default, `10px sans-serif`
//! (HTML, `CanvasRenderingContext2D.font`).

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yoke::{Yoke, Yokeable};

use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::font_faces::{FontFaceDescriptor, FONT_FACES};
use crate::text_measurements::TextMetricsProvider;
use crate::unicode_range::{UnicodeRange, UnicodeRangeError};

/// Why a face could not be added.
#[derive(Clone, Debug, PartialEq)]
pub enum FontError {
    /// A WOFF or WOFF2 file that does not decompress.
    Decode(String),
    /// Bytes that are not an OpenType/TrueType font.
    Parse,
    /// An invalid `unicode-range` descriptor.
    UnicodeRange(UnicodeRangeError),
    /// An invalid `font-weight` descriptor.
    Weight(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Decode(reason) => write!(f, "font file does not decompress: {reason}"),
            FontError::Parse => write!(f, "not an OpenType or TrueType font"),
            FontError::UnicodeRange(e) => e.fmt(f),
            FontError::Weight(w) => write!(f, "invalid font-weight descriptor {w:?}"),
        }
    }
}

impl std::error::Error for FontError {}

impl From<UnicodeRangeError> for FontError {
    fn from(e: UnicodeRangeError) -> FontError {
        FontError::UnicodeRange(e)
    }
}

/// Decode a font file to OpenType/TrueType (sfnt) bytes: WOFF2 and WOFF are
/// decompressed, anything else is returned as is.
pub fn decode_font_file(data: &[u8]) -> Result<Vec<u8>, FontError> {
    match data.get(..4) {
        Some(b"wOF2") => {
            wuff::decompress_woff2(data).map_err(|e| FontError::Decode(format!("{e:?}")))
        }
        Some(b"wOFF") => {
            wuff::decompress_woff1(data).map_err(|e| FontError::Decode(format!("{e:?}")))
        }
        _ => Ok(data.to_vec()),
    }
}

/// A face parsed for shaping, borrowing the sfnt bytes it was parsed from.
#[derive(Clone, Yokeable)]
struct Parsed<'a>(rustybuzz::Face<'a>);

/// The sfnt bytes of a face and the face parsed from them, parsed once when
/// the face is loaded. [`Yoke`] keeps the borrowing face together with the
/// shared bytes it borrows, so measuring a line only shapes.
type ParsedFace = Yoke<Parsed<'static>, Arc<[u8]>>;

/// The segment properties a shaping plan is built for: direction, script
/// (`None` when the text has no script of its own, as `rustybuzz::shape`
/// plans it) and language.
type PlanKey = (
    rustybuzz::Direction,
    Option<rustybuzz::Script>,
    Option<rustybuzz::Language>,
);

/// The shaping plans built for a face, one per set of segment properties
/// met so far. A plan compiles the face's OpenType feature lookups, which
/// for faces with large `GSUB`/`GPOS` tables (Cascadia) costs more than
/// shaping a line, so it is built once per face and properties rather than
/// on every call as `rustybuzz::shape` does.
type Plans = Mutex<Vec<(PlanKey, Arc<rustybuzz::ShapePlan>)>>;

/// One loaded face.
struct Face {
    weight: u16,
    range: UnicodeRange,
    parsed: ParsedFace,
    plans: Plans,
    units_per_em: f64,
    /// The code points the `cmap` maps to a glyph other than `.notdef`,
    /// sorted.
    coverage: Vec<u32>,
}

impl Clone for Face {
    fn clone(&self) -> Face {
        Face {
            weight: self.weight,
            range: self.range.clone(),
            parsed: self.parsed.clone(),
            plans: Mutex::new(self.plans().clone()),
            units_per_em: self.units_per_em,
            coverage: self.coverage.clone(),
        }
    }
}

impl Face {
    fn load(data: &[u8], range: UnicodeRange, weight: u16) -> Result<Face, FontError> {
        let data: Arc<[u8]> = decode_font_file(data)?.into();
        let parsed = ParsedFace::try_attach_to_cart(data, |sfnt| {
            rustybuzz::Face::from_slice(sfnt, 0)
                .map(Parsed)
                .ok_or(FontError::Parse)
        })?;
        let face: &ttf_parser::Face<'_> = &parsed.get().0;
        let mut coverage = Vec::new();
        if let Some(cmap) = face.tables().cmap {
            for subtable in cmap.subtables {
                if subtable.is_unicode() {
                    subtable.codepoints(|cp| coverage.push(cp));
                }
            }
        }
        coverage.sort_unstable();
        coverage.dedup();
        coverage.retain(|&cp| {
            char::from_u32(cp)
                .and_then(|c| face.glyph_index(c))
                .is_some_and(|g| g.0 != 0)
        });
        let units_per_em = f64::from(face.units_per_em());
        Ok(Face {
            weight,
            range,
            parsed,
            plans: Mutex::new(Vec::new()),
            units_per_em,
            coverage,
        })
    }

    fn maps(&self, ch: char) -> bool {
        self.coverage.binary_search(&u32::from(ch)).is_ok()
    }

    /// Whether the browser would use this face for `ch`: in its
    /// `unicode-range` and mapped by its `cmap`.
    fn covers(&self, ch: char) -> bool {
        self.range.contains(u32::from(ch)) && self.maps(ch)
    }

    fn plans(&self) -> MutexGuard<'_, Vec<(PlanKey, Arc<rustybuzz::ShapePlan>)>> {
        // A panic while the lock was held leaves at worst a list missing
        // one plan, which is still valid.
        self.plans.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The plan for shaping `buffer` on this face with the default features
    /// (the plan `rustybuzz::shape` would build), built the first time its
    /// segment properties are met.
    fn plan(&self, buffer: &rustybuzz::UnicodeBuffer) -> Arc<rustybuzz::ShapePlan> {
        // `UnicodeBuffer::script` reports a buffer without a script as
        // UNKNOWN, which guess_segment_properties never assigns.
        let script = Some(buffer.script()).filter(|&s| s != rustybuzz::script::UNKNOWN);
        let key: PlanKey = (buffer.direction(), script, buffer.language());
        let mut plans = self.plans();
        if let Some((_, plan)) = plans.iter().find(|(k, _)| *k == key) {
            return Arc::clone(plan);
        }
        let plan = Arc::new(rustybuzz::ShapePlan::new(
            &self.parsed.get().0,
            key.0,
            key.1,
            key.2.as_ref(),
            &[],
        ));
        plans.push((key, Arc::clone(&plan)));
        plan
    }

    /// The advance width of `text` shaped on this face at `size` px.
    fn shaped_width(&self, text: &str, size: f64) -> f64 {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let plan = self.plan(&buffer);
        let glyphs = rustybuzz::shape_with_plan(&self.parsed.get().0, &plan, buffer);
        let units: i64 = glyphs
            .glyph_positions()
            .iter()
            .map(|p| i64::from(p.x_advance))
            .sum();
        units as f64 * size / self.units_per_em
    }
}

#[derive(Clone)]
struct Family {
    name: String,
    /// In registration order.
    faces: Vec<Face>,
}

impl Family {
    /// CSS Fonts 4 §5.2 step 4, font-weight: the weight of this family's
    /// faces that best matches `desired`.
    fn matched_weight(&self, desired: u16) -> Option<u16> {
        let mut weights: Vec<u16> = self.faces.iter().map(|f| f.weight).collect();
        weights.sort_unstable();
        weights.dedup();
        if weights.contains(&desired) {
            return Some(desired);
        }
        let below = weights.iter().rev().copied().find(|&w| w < desired);
        let above = weights.iter().copied().find(|&w| w > desired);
        if (400..=500).contains(&desired) {
            weights
                .iter()
                .copied()
                .find(|&w| w > desired && w <= 500)
                .or(below)
                .or(above)
        } else if desired < 400 {
            below.or(above)
        } else {
            above.or(below)
        }
    }
}

/// The family and face a character is drawn with; `None` when nothing can
/// draw it.
type Drawn<'a> = Option<(&'a Family, &'a Face)>;

/// A family in a font string's list, with the weight matched in it.
struct Candidate<'a> {
    family: &'a Family,
    weight: u16,
}

impl<'a> Candidate<'a> {
    /// The faces the browser tries for a character, the last registered
    /// first.
    fn faces(&self) -> impl Iterator<Item = &'a Face> + '_ {
        let weight = self.weight;
        self.family
            .faces
            .iter()
            .rev()
            .filter(move |f| f.weight == weight)
    }
}

/// A parsed CSS `font` shorthand.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    /// The size in px.
    pub size: f64,
    /// The numeric font-weight (400 `normal`, 700 `bold`).
    pub weight: u16,
    /// The family list, names unquoted, generic keywords lower-cased.
    pub families: Vec<String>,
}

const GENERIC_FAMILIES: [&str; 5] = ["serif", "sans-serif", "monospace", "cursive", "fantasy"];

/// Parse a CSS `font` shorthand of the form canvas accepts:
/// `[style] [variant] [weight] [stretch] <size>px[/<line-height>] <family>[, <family>]*`.
/// `None` if it is not one (the canvas then keeps its previous font).
pub fn parse_font(font: &str) -> Option<FontSpec> {
    let font = font.trim_matches(|c: char| c.is_ascii_whitespace());
    let mut weight = 400;
    let mut rest = font;
    loop {
        let end = rest
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let token = &rest[..end];
        if token.is_empty() {
            return None;
        }
        let lower = token.to_ascii_lowercase();
        if let Some(size) = parse_size(&lower) {
            let families = parse_families(&rest[end..])?;
            return Some(FontSpec {
                size,
                weight,
                families,
            });
        }
        match lower.as_str() {
            "normal" | "italic" | "oblique" | "small-caps" | "ultra-condensed"
            | "extra-condensed" | "condensed" | "semi-condensed" | "semi-expanded" | "expanded"
            | "extra-expanded" | "ultra-expanded" => {}
            "bold" | "bolder" => weight = 700,
            "lighter" => weight = 100,
            _ => {
                let w: f64 = lower.parse().ok()?;
                if !(1.0..=1000.0).contains(&w) {
                    return None;
                }
                weight = w.round() as u16;
            }
        }
        rest = rest[end..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    }
}

/// `<number>px`, optionally followed by `/<line-height>`.
fn parse_size(token: &str) -> Option<f64> {
    let size = token.split_once('/').map_or(token, |(s, _)| s);
    let number = size.strip_suffix("px")?;
    if number.is_empty()
        || !number
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn parse_families(list: &str) -> Option<Vec<String>> {
    let mut families = Vec::new();
    for part in list.split(',') {
        let part = part.trim_matches(|c: char| c.is_ascii_whitespace());
        let unquoted = part
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .or_else(|| part.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')));
        let name = match unquoted {
            Some(name) => name.to_owned(),
            None => {
                let words: Vec<&str> = part.split_ascii_whitespace().collect();
                if words.is_empty() {
                    return None;
                }
                let name = words.join(" ");
                let lower = name.to_ascii_lowercase();
                if GENERIC_FAMILIES.contains(&lower.as_str()) {
                    lower
                } else {
                    name
                }
            }
        };
        if name.is_empty() {
            return None;
        }
        families.push(name);
    }
    Some(families)
}

/// The canvas default font, `10px sans-serif`.
fn default_font() -> FontSpec {
    FontSpec {
        size: 10.0,
        weight: 400,
        families: vec!["sans-serif".to_owned()],
    }
}

/// `font-weight` descriptor: `normal`, `bold` or a number in 1-1000 (the
/// first of a range).
fn parse_weight(descriptor: Option<&str>) -> Result<u16, FontError> {
    let Some(d) = descriptor else {
        return Ok(400);
    };
    let first = d.split_ascii_whitespace().next().unwrap_or("");
    match first.to_ascii_lowercase().as_str() {
        "normal" => Ok(400),
        "bold" => Ok(700),
        other => match other.parse::<f64>() {
            Ok(w) if (1.0..=1000.0).contains(&w) => Ok(w.round() as u16),
            _ => Err(FontError::Weight(d.to_owned())),
        },
    }
}

fn sticks_to_previous(ch: char) -> bool {
    ch.general_category_group() == GeneralCategoryGroup::Mark || is_default_ignorable(ch)
}

/// Unicode `Default_Ignorable_Code_Point` (DerivedCoreProperties.txt,
/// Unicode 16).
fn is_default_ignorable(ch: char) -> bool {
    matches!(
        u32::from(ch),
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// Font faces by family, measuring lines as the browser's canvas does.
#[derive(Clone)]
pub struct FontStore {
    families: Vec<Family>,
    /// Generic family keyword -> family name.
    generics: Vec<(String, String)>,
}

impl fmt::Debug for FontStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontStore")
            .field(
                "families",
                &self
                    .families
                    .iter()
                    .map(|fam| (&fam.name, fam.faces.len()))
                    .collect::<Vec<_>>(),
            )
            .field("generics", &self.generics)
            .finish()
    }
}

impl Default for FontStore {
    fn default() -> FontStore {
        FontStore::new()
    }
}

impl FontStore {
    /// A store with no faces; `sans-serif` maps to Liberation Sans and
    /// `monospace` to Cascadia once faces of those families are added.
    pub fn new() -> FontStore {
        FontStore {
            families: Vec::new(),
            generics: vec![
                ("sans-serif".to_owned(), "Liberation Sans".to_owned()),
                ("monospace".to_owned(), "Cascadia".to_owned()),
            ],
        }
    }

    /// Every vendored face of [`FONT_FACES`] (all but the `local:` ones),
    /// with `load` returning the bytes of a file given its path relative to
    /// `crates/excali-text/assets/fonts/`. A face whose file `load` does
    /// not return is left out, which is how a host loads faces lazily.
    pub fn with_vendored_faces(
        mut load: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<FontStore, FontError> {
        let mut store = FontStore::new();
        for face in FONT_FACES.iter() {
            let Some(path) = face.vendored else {
                continue;
            };
            if let Some(bytes) = load(path) {
                store.add_font_face(face, &bytes)?;
            }
        }
        Ok(store)
    }

    /// Add the file of a [`FONT_FACES`] entry under its family, range and
    /// weight.
    pub fn add_font_face(
        &mut self,
        face: &FontFaceDescriptor,
        font_data: &[u8],
    ) -> Result<(), FontError> {
        self.add_face(face.family, font_data, face.unicode_range, face.weight)
    }

    /// Register a face, as `new FontFace(family, data, { unicodeRange,
    /// weight })` followed by `document.fonts.add` does. `font_data` is a
    /// WOFF2, WOFF, TrueType or OpenType file.
    pub fn add_face(
        &mut self,
        family: &str,
        font_data: &[u8],
        unicode_range: Option<&str>,
        weight: Option<&str>,
    ) -> Result<(), FontError> {
        let range = match unicode_range {
            Some(r) => UnicodeRange::parse(r)?,
            None => UnicodeRange::all(),
        };
        let weight = parse_weight(weight)?;
        let face = Face::load(font_data, range, weight)?;
        match self
            .families
            .iter_mut()
            .find(|f| f.name.eq_ignore_ascii_case(family))
        {
            Some(existing) => existing.faces.push(face),
            None => self.families.push(Family {
                name: family.to_owned(),
                faces: vec![face],
            }),
        }
        Ok(())
    }

    /// Map a generic family keyword (`"sans-serif"`, `"monospace"`,
    /// `"serif"`, `"cursive"`, `"fantasy"`) to a family, or unmap it.
    pub fn set_generic_family(&mut self, generic: &str, family: Option<&str>) {
        let generic = generic.to_ascii_lowercase();
        self.generics.retain(|(g, _)| *g != generic);
        if let Some(family) = family {
            self.generics.push((generic, family.to_owned()));
        }
    }

    /// The names of the families with faces, in the order first added.
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.families.iter().map(|f| f.name.as_str())
    }

    fn family(&self, name: &str) -> Option<&Family> {
        let name = self
            .generics
            .iter()
            .find(|(g, _)| g == name)
            .map_or(name, |(_, family)| family.as_str());
        self.families
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }

    fn candidates(&self, spec: &FontSpec) -> Vec<Candidate<'_>> {
        spec.families
            .iter()
            .filter_map(|name| self.family(name))
            .filter_map(|family| {
                family
                    .matched_weight(spec.weight)
                    .map(|weight| Candidate { family, weight })
            })
            .collect()
    }

    /// The face the browser would draw `ch` with, and its family.
    fn resolve<'a>(candidates: &[Candidate<'a>], ch: char) -> Option<(&'a Family, &'a Face)> {
        candidates.iter().find_map(|c| {
            c.faces()
                .find(|face| face.covers(ch))
                .map(|face| (c.family, face))
        })
    }

    /// The first family's face for characters no face maps: the one that
    /// draws a space, else its last registered face.
    fn last_resort<'a>(candidates: &[Candidate<'a>]) -> Option<(&'a Family, &'a Face)> {
        let first = candidates.first()?;
        let face = first
            .faces()
            .find(|f| f.covers(' '))
            .or_else(|| first.faces().next())?;
        Some((first.family, face))
    }

    fn spec(font: &str) -> FontSpec {
        parse_font(font).unwrap_or_else(default_font)
    }

    /// The runs of `text` and the family and face each is drawn with.
    fn runs<'s, 't>(
        &'s self,
        text: &'t str,
        candidates: &[Candidate<'s>],
    ) -> Vec<(&'t str, Drawn<'s>)> {
        let mut runs: Vec<(usize, usize, Drawn<'s>)> = Vec::new();
        for (i, ch) in text.char_indices() {
            let end = i + ch.len_utf8();
            let previous = runs.last().and_then(|r| r.2);
            let face = match previous {
                Some((family, face)) if sticks_to_previous(ch) && face.maps(ch) => {
                    Some((family, face))
                }
                _ => match Self::resolve(candidates, ch) {
                    Some(found) => Some(found),
                    None if sticks_to_previous(ch) && previous.is_some() => previous,
                    None => Self::last_resort(candidates),
                },
            };
            match runs.last_mut() {
                Some(run) if same_face(run.2, face) => run.1 = end,
                _ => runs.push((i, end, face)),
            }
        }
        runs.into_iter()
            .map(|(start, end, face)| (&text[start..end], face))
            .collect()
    }

    /// The family `ch` is drawn with in `font`, `None` if no face maps it.
    pub fn family_for(&self, ch: char, font: &str) -> Option<&str> {
        let spec = Self::spec(font);
        let candidates = self.candidates(&spec);
        Self::resolve(&candidates, ch).map(|(family, _)| family.name.as_str())
    }

    /// The advance width of `text`, a single line, in the CSS font string
    /// `font`: the canvas `measureText(text).width`.
    pub fn line_width(&self, text: &str, font: &str) -> f64 {
        let spec = Self::spec(font);
        let candidates = self.candidates(&spec);
        self.runs(text, &candidates)
            .into_iter()
            .map(|(run, face)| face.map_or(0.0, |(_, face)| face.shaped_width(run, spec.size)))
            .sum()
    }
}

fn same_face(a: Drawn<'_>, b: Drawn<'_>) -> bool {
    match (a, b) {
        (Some((_, a)), Some((_, b))) => std::ptr::eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl TextMetricsProvider for FontStore {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        self.line_width(text, font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Included rather than read: excali-text does no file I/O (ADR-008).
    const CASCADIA: &[u8] = include_bytes!("../assets/fonts/Cascadia/CascadiaCode-Regular.woff2");
    const LIBERATION: &[u8] =
        include_bytes!("../assets/fonts/Liberation/LiberationSans-Regular.ttf");
    const VIRGIL: &[u8] = include_bytes!("../assets/fonts/Virgil/Virgil-Regular.woff2");

    fn face(data: &[u8]) -> Face {
        Face::load(data, UnicodeRange::all(), 400).unwrap()
    }

    /// What `rustybuzz::shape` gives with the face parsed afresh and a plan
    /// built for the one call: the measurement before faces were kept
    /// parsed and plans cached.
    fn uncached_width(face: &Face, text: &str, size: f64) -> f64 {
        let parsed = rustybuzz::Face::from_slice(face.parsed.backing_cart(), 0).unwrap();
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let glyphs = rustybuzz::shape(&parsed, &[], buffer);
        let units: i64 = glyphs
            .glyph_positions()
            .iter()
            .map(|p| i64::from(p.x_advance))
            .sum();
        units as f64 * size / face.units_per_em
    }

    #[test]
    fn cached_plans_shape_as_a_fresh_plan_does() {
        let texts = [
            "Hello, World",
            "AV Ta To",
            "12 345",
            " ",
            "->=> != <=",
            "\u{645}\u{631}\u{62D}\u{628}\u{627}",
            "abc \u{5E9}\u{5DC}\u{5D5}\u{5DD} def",
            "e\u{301}",
            "",
        ];
        for (path, data) in [
            ("Cascadia", CASCADIA),
            ("Liberation Sans", LIBERATION),
            ("Virgil", VIRGIL),
        ] {
            let face = face(data);
            // Twice: the second pass shapes with the plans the first built.
            for _ in 0..2 {
                for text in texts {
                    assert_eq!(
                        face.shaped_width(text, 20.0),
                        uncached_width(&face, text, 20.0),
                        "{path}: {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_plan_is_built_once_per_segment_properties() {
        let face = face(CASCADIA);
        for text in ["one", "two", "three", "a != b"] {
            face.shaped_width(text, 16.0);
        }
        // Latin, left to right, no language.
        assert_eq!(face.plans().len(), 1);
        // Digits and spaces only: no script of their own.
        face.shaped_width("12 34", 16.0);
        face.shaped_width("5", 16.0);
        assert_eq!(face.plans().len(), 2);
        // Hebrew, right to left.
        face.shaped_width("\u{5E9}\u{5DC}\u{5D5}\u{5DD}", 16.0);
        assert_eq!(face.plans().len(), 3);
        face.shaped_width("one more", 16.0);
        assert_eq!(face.plans().len(), 3);
    }

    #[test]
    fn a_cloned_store_measures_the_same() {
        let mut store = FontStore::new();
        store.add_face("Cascadia", CASCADIA, None, None).unwrap();
        let before = store.line_width("fn main() -> i32", "16px Cascadia");
        let clone = store.clone();
        drop(store);
        assert_eq!(
            clone.line_width("fn main() -> i32", "16px Cascadia"),
            before
        );
    }

    #[test]
    fn stores_are_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<FontStore>();
    }
}
