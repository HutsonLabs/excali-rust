//! Fidelity: does a subset draw the requested text exactly as the original
//! face does?
//!
//! An SVG export draws each text line with `<text>` in the inlined face
//! (`renderer/staticSvgScene.ts:776-842`), so what a viewer sees depends on
//! the face's cmap, glyph outlines and advances, its layout tables (the
//! browser shapes with HarfBuzz, kerning and ligatures included) and its
//! font-wide metrics. Each is compared with the original file, read with
//! ttf-parser and shaped with rustybuzz, the readers excali-text measures
//! with.

use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// The comparison of one subset with its original face.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fidelity {
    /// Code points asked for that the original face maps.
    pub code_points: usize,
    /// Of those, the ones the subset maps.
    pub kept: usize,
    /// Of those, the ones whose glyph has the original's advance and
    /// outline (composite glyphs resolved).
    pub glyphs_equal: usize,
    /// Whether unitsPerEm and the hhea, OS/2 typo and win metrics (and the
    /// USE_TYPO_METRICS flag) equal the original's.
    pub metrics_equal: bool,
    /// Runs of the scene's text in this face: maximal runs of characters
    /// the original face maps and the subset was asked for.
    pub runs: usize,
    /// Of those, the runs rustybuzz shapes to the same drawn glyphs
    /// (outlines, advances and offsets, in order) with either font.
    pub runs_equal: usize,
}

impl Fidelity {
    /// Everything equal.
    pub fn full(&self) -> bool {
        self.kept == self.code_points
            && self.glyphs_equal == self.code_points
            && self.metrics_equal
            && self.runs_equal == self.runs
    }
}

/// A glyph's outline as drawn, command by command, in font units.
#[derive(Default, PartialEq, Debug)]
struct Outline(Vec<(u8, [f32; 6])>);

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push((b'M', [x, y, 0.0, 0.0, 0.0, 0.0]));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push((b'L', [x, y, 0.0, 0.0, 0.0, 0.0]));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.push((b'Q', [x1, y1, x, y, 0.0, 0.0]));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.push((b'C', [x1, y1, x2, y2, x, y]));
    }
    fn close(&mut self) {
        self.0.push((b'Z', [0.0; 6]));
    }
}

fn outline(face: &Face<'_>, glyph: GlyphId) -> Outline {
    let mut o = Outline::default();
    face.outline_glyph(glyph, &mut o);
    o
}

/// Same advance and same outline.
fn same_glyph(a: &Face<'_>, ga: GlyphId, b: &Face<'_>, gb: GlyphId) -> bool {
    a.glyph_hor_advance(ga) == b.glyph_hor_advance(gb) && outline(a, ga) == outline(b, gb)
}

/// The font-wide metrics a browser lays text out with.
fn metrics(face: &Face<'_>) -> Vec<i32> {
    let t = face.tables();
    let mut m = vec![i32::from(face.units_per_em())];
    let h = t.hhea;
    m.extend([h.ascender, h.descender, h.line_gap].map(i32::from));
    if let Some(os2) = t.os2 {
        m.extend(
            [
                os2.typographic_ascender(),
                os2.typographic_descender(),
                os2.typographic_line_gap(),
                os2.windows_ascender(),
                os2.windows_descender(),
            ]
            .map(i32::from),
        );
        m.push(i32::from(os2.use_typographic_metrics()));
    } else {
        m.push(i32::MIN);
    }
    m
}

/// Maximal runs of `text`'s characters that are in `code_points` and that
/// `face` maps.
pub fn runs(face: &Face<'_>, code_points: &[u32], text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars() {
        if code_points.contains(&u32::from(c)) && face.glyph_index(c).is_some() {
            run.push(c);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

/// One drawn glyph of a shaped run: its outline, advance and offsets.
type Drawn = (Outline, i32, i32, i32, i32);

/// The glyphs a shaped run draws, in order. A glyph with no outline and no
/// advance draws nothing: HarfBuzz and rustybuzz shape a default-ignorable
/// character (U+00AD SOFT HYPHEN) as the face's space glyph at zero advance
/// when the face has one, and delete it, merging its cluster into the next
/// glyph's, when it has none. So a subset without U+0020 differs from its
/// original there only in that invisible glyph and in cluster numbers,
/// which place a caret but draw nothing; neither is compared.
fn drawn(face: &rustybuzz::Face<'_>, glyphs: &Face<'_>, run: &str) -> Vec<Drawn> {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(run);
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(face, &[], buffer);
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(i, p)| {
            (
                outline(glyphs, GlyphId(i.glyph_id as u16)),
                p.x_advance,
                p.y_advance,
                p.x_offset,
                p.y_offset,
            )
        })
        .filter(|(o, dx, dy, _, _)| !(o.0.is_empty() && *dx == 0 && *dy == 0))
        .collect()
}

fn shape_equal(original: &[u8], a: &Face<'_>, subset: &[u8], b: &Face<'_>, run: &str) -> bool {
    match (
        rustybuzz::Face::from_slice(original, 0),
        rustybuzz::Face::from_slice(subset, 0),
    ) {
        (Some(ra), Some(rb)) => drawn(&ra, a, run) == drawn(&rb, b, run),
        _ => false,
    }
}

/// Compares `subset` (sfnt) with `original` (sfnt) for `code_points` and the
/// scene's `texts` in this face. `None` when the subset cannot be parsed.
pub fn compare(
    original: &[u8],
    subset: &[u8],
    code_points: &[u32],
    texts: &[&str],
) -> Option<Fidelity> {
    let a = Face::parse(original, 0).ok()?;
    let b = Face::parse(subset, 0).ok()?;
    let mut f = Fidelity {
        metrics_equal: metrics(&a) == metrics(&b),
        ..Fidelity::default()
    };
    let mut seen: Vec<u32> = Vec::new();
    for &cp in code_points {
        if seen.contains(&cp) {
            continue;
        }
        seen.push(cp);
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let Some(ga) = a.glyph_index(c) else { continue };
        f.code_points += 1;
        let Some(gb) = b.glyph_index(c) else { continue };
        f.kept += 1;
        if same_glyph(&a, ga, &b, gb) {
            f.glyphs_equal += 1;
        }
    }
    for text in texts {
        for run in runs(&a, code_points, text) {
            f.runs += 1;
            if shape_equal(original, &a, subset, &b, &run) {
                f.runs_equal += 1;
            }
        }
    }
    Some(f)
}
