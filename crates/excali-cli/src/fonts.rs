//! The vendored fonts (`crates/excali-text/assets/fonts`, ADR-004): text
//! measurement for export, and the text a PNG export draws.
//!
//! A browser draws with the faces a scene's text needs: every registered
//! face is declared with its `unicode-range`, and only those holding a
//! character in use are fetched (`Fonts.ts`). [`load_fonts`] loads the
//! same set: each vendored face whose range holds a character of the
//! scene's texts or frame names, or printable ASCII (what frame labels
//! and placeholders may draw).
//!
//! [`GlyphText`] is the raster backend's `TextRasterizer`: `fillText` as
//! the glyph outlines of the shaped line
//! (`excali_text::font_store::FontStore::shape_line`), placed by
//! `textAlign` and filled non-zero, anti-aliased, in the run's colour.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use excali_core::element::{Element, ElementKind};
use excali_raster::tiny_skia::{self, FillRule, Mask, Paint, PathBuilder, Pixmap};
use excali_raster::TextRasterizer;
use excali_scene::display::{Direction, TextAlign, TextRun};
use excali_scene::export::TextMetrics;
use excali_text::font_assets::parse_unicode_range;
use excali_text::font_faces::FONT_FACES;
use excali_text::font_store::{FontStore, OutlineSegment};

use crate::error::Failure;

/// The environment variable naming the font directory.
pub const FONTS_DIR_VAR: &str = "EXCALI_FONTS_DIR";

/// The font directory this binary was built with: the workspace's vendored
/// fonts.
pub fn built_in_fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../excali-text/assets/fonts")
}

/// The characters a scene may draw: its texts, its frame names, and
/// printable ASCII.
fn scene_characters(elements: &[Element]) -> HashSet<u32> {
    let mut chars: HashSet<u32> = (0x20..0x7f).collect();
    for element in elements {
        let texts: [Option<&str>; 2] = match &element.kind {
            ElementKind::Text(t) => [Some(&t.text), Some(&t.original_text)],
            ElementKind::Frame(f) | ElementKind::MagicFrame(f) => [f.name.as_deref(), None],
            _ => [None, None],
        };
        for text in texts.into_iter().flatten() {
            chars.extend(text.chars().map(u32::from));
        }
    }
    chars
}

/// The vendored files of the faces whose range holds one of `chars`.
fn needed_files(chars: &HashSet<u32>) -> HashSet<&'static str> {
    FONT_FACES
        .iter()
        .filter_map(|face| {
            let file = face.vendored?;
            let wanted = match face.unicode_range {
                None => true,
                Some(range) => parse_unicode_range(range).map_or(true, |ranges| {
                    ranges
                        .iter()
                        .any(|&(lo, hi)| chars.iter().any(|c| (lo..=hi).contains(c)))
                }),
            };
            wanted.then_some(file)
        })
        .collect()
}

/// The faces `elements` need, from `dir`.
pub fn load_fonts(dir: &Path, elements: &[Element]) -> Result<FontStore, Failure> {
    if !dir.is_dir() {
        return Err(Failure::Io(format!(
            "{}: no font directory (set --fonts or {FONTS_DIR_VAR})",
            dir.display()
        )));
    }
    let needed = needed_files(&scene_characters(elements));
    let mut missing: Option<Failure> = None;
    let store = FontStore::with_vendored_faces(|file| {
        if !needed.contains(file) {
            return None;
        }
        let path = dir.join(file);
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                missing.get_or_insert_with(|| Failure::io(&path, &e));
                None
            }
        }
    })
    .map_err(|e| Failure::Io(format!("{}: {e}", dir.display())))?;
    match missing {
        Some(failure) => Err(failure),
        None => Ok(store),
    }
}

/// Frame name measurement for SVG export.
pub struct Metrics<'a>(pub &'a FontStore);

impl TextMetrics for Metrics<'_> {
    fn measure(&self, text: &str, font: &str) -> f64 {
        self.0.line_width(text, font)
    }
}

/// `fillText` from the font files.
pub struct GlyphText<'a> {
    store: &'a FontStore,
}

impl<'a> GlyphText<'a> {
    pub fn new(store: &'a FontStore) -> GlyphText<'a> {
        GlyphText { store }
    }
}

impl TextRasterizer for GlyphText<'_> {
    fn fill_text(
        &mut self,
        target: &mut Pixmap,
        run: &TextRun,
        color: tiny_skia::Color,
        transform: tiny_skia::Transform,
        clip: Option<&Mask>,
    ) {
        let line =
            self.store
                .shape_line(&run.text, &run.font.css(), run.direction == Direction::Rtl);
        // textAlign: where x sits on the line.
        let x = run.x
            - match run.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => line.width / 2.0,
                TextAlign::Right => line.width,
            };
        let y = run.y;
        let p = |px: f64, py: f64| ((x + px) as f32, (y + py) as f32);
        let mut path = PathBuilder::new();
        for segment in &line.outline {
            match *segment {
                OutlineSegment::MoveTo(a, b) => {
                    let (a, b) = p(a, b);
                    path.move_to(a, b);
                }
                OutlineSegment::LineTo(a, b) => {
                    let (a, b) = p(a, b);
                    path.line_to(a, b);
                }
                OutlineSegment::QuadTo(a, b, c, d) => {
                    let (a, b) = p(a, b);
                    let (c, d) = p(c, d);
                    path.quad_to(a, b, c, d);
                }
                OutlineSegment::CurveTo(a, b, c, d, e, f) => {
                    let (a, b) = p(a, b);
                    let (c, d) = p(c, d);
                    let (e, f) = p(e, f);
                    path.cubic_to(a, b, c, d, e, f);
                }
                OutlineSegment::Close => path.close(),
            }
        }
        let Some(path) = path.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        target.fill_path(&path, &paint, FillRule::Winding, transform, clip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_text_needs_no_cjk_faces() {
        let needed = needed_files(&scene_characters(&[]));
        assert!(needed.iter().any(|f| f.starts_with("Excalifont/")));
        assert!(needed.contains("Liberation/LiberationSans-Regular.ttf"));
        let cjk = needed.iter().filter(|f| f.starts_with("Xiaolai/")).count();
        let all = FONT_FACES
            .iter()
            .filter(|f| f.vendored.is_some_and(|v| v.starts_with("Xiaolai/")))
            .count();
        assert!(cjk < all / 10, "{cjk} of {all}");
    }

    #[test]
    fn cjk_text_needs_the_faces_holding_it() {
        let chars: HashSet<u32> = "\u{4E2D}".chars().map(u32::from).collect();
        let needed = needed_files(&chars);
        assert!(needed.iter().any(|f| f.starts_with("Xiaolai/")));
    }

    #[test]
    fn a_missing_directory_is_an_io_failure() {
        let e = load_fonts(Path::new("/nonexistent/fonts"), &[]).unwrap_err();
        assert!(matches!(e, Failure::Io(_)), "{e}");
    }

    #[test]
    fn the_built_in_directory_loads() {
        let store = load_fonts(&built_in_fonts_dir(), &[]).unwrap();
        assert!(store.families().any(|f| f == "Excalifont"));
        assert!(store.line_width("abc", "20px Excalifont") > 0.0);
    }
}
