//! Glyph outlines of a shaped line (`FontStore::shape_line`): what a
//! canvas `fillText` fills, for hosts that rasterize text themselves (the
//! CLI's PNG export, ex-409). The line is shaped exactly as
//! `FontStore::line_width` measures it (same faces, same fallback runs,
//! same advances), and each glyph's outline is the face's `glyf`/`CFF`
//! outline scaled to the font size, placed on the alphabetic baseline with
//! y growing downwards, as the canvas draws it.

mod common;

use common::{font_file, store};
use excali_core::element::FontFamily;
use excali_text::font_metadata::get_font_string;
use excali_text::font_store::{decode_font_file, OutlineSegment};

const LIBERATION: &str = "Liberation/LiberationSans-Regular.ttf";

/// The bounding box of a line's outline points: `[min_x, min_y, max_x, max_y]`.
fn bbox(segments: &[OutlineSegment]) -> Option<[f64; 4]> {
    let mut points = Vec::new();
    for s in segments {
        match *s {
            OutlineSegment::MoveTo(x, y) | OutlineSegment::LineTo(x, y) => points.push((x, y)),
            OutlineSegment::QuadTo(x1, y1, x, y) => points.extend([(x1, y1), (x, y)]),
            OutlineSegment::CurveTo(x1, y1, x2, y2, x, y) => {
                points.extend([(x1, y1), (x2, y2), (x, y)])
            }
            OutlineSegment::Close => {}
        }
    }
    let first = *points.first()?;
    Some(points.iter().fold(
        [first.0, first.1, first.0, first.1],
        |[a, b, c, d], &(x, y)| [a.min(x), b.min(y), c.max(x), d.max(y)],
    ))
}

#[test]
fn the_width_is_the_measured_width() {
    let s = store();
    for (text, font) in [
        ("Hello, World", get_font_string(20.0, FontFamily::EXCALIFONT)),
        ("AV Ta To", get_font_string(36.0, FontFamily::VIRGIL)),
        ("fi fl -> =>", get_font_string(16.0, FontFamily::CASCADIA)),
        ("abc 中文 def", get_font_string(20.0, FontFamily::EXCALIFONT)),
        (
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",
            get_font_string(20.0, FontFamily::NUNITO),
        ),
        ("frame name", "14px Helvetica, sans-serif".to_owned()),
        ("", get_font_string(20.0, FontFamily::EXCALIFONT)),
    ] {
        let line = s.shape_line(text, &font, false);
        let measured = s.line_width(text, &font);
        assert!(
            (line.width - measured).abs() < 1e-9,
            "{text:?} in {font}: shaped {} measured {measured}",
            line.width
        );
    }
}

#[test]
fn empty_and_blank_lines_have_no_outline() {
    let s = store();
    let font = get_font_string(20.0, FontFamily::EXCALIFONT);
    let empty = s.shape_line("", &font, false);
    assert_eq!(empty.width, 0.0);
    assert!(empty.outline.is_empty());
    let blank = s.shape_line("   ", &font, false);
    assert!(blank.width > 0.0);
    assert!(blank.outline.is_empty(), "{:?}", blank.outline);
}

#[test]
fn a_glyph_is_the_face_outline_scaled_onto_the_baseline() {
    // Liberation Sans "I" at 100 px: the face's outline in font units,
    // times 100 / unitsPerEm, y flipped (the canvas's y grows downwards).
    let sfnt = decode_font_file(&font_file(LIBERATION)).unwrap();
    let face = ttf_parser::Face::parse(&sfnt, 0).unwrap();
    let scale = 100.0 / f64::from(face.units_per_em());
    let glyph = face.glyph_index('I').unwrap();
    let rect = face.glyph_bounding_box(glyph).unwrap();

    let line = store().shape_line("I", "100px Liberation Sans", false);
    let [min_x, min_y, max_x, max_y] = bbox(&line.outline).unwrap();
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    assert!(close(min_x, f64::from(rect.x_min) * scale), "{min_x}");
    assert!(close(max_x, f64::from(rect.x_max) * scale), "{max_x}");
    assert!(close(min_y, -f64::from(rect.y_max) * scale), "{min_y}");
    assert!(close(max_y, -f64::from(rect.y_min) * scale), "{max_y}");
    assert!(matches!(line.outline[0], OutlineSegment::MoveTo(..)));
    assert!(matches!(line.outline.last(), Some(OutlineSegment::Close)));
    let advance = f64::from(face.glyph_hor_advance(glyph).unwrap()) * scale;
    assert!(close(line.width, advance));
}

#[test]
fn glyphs_follow_each_other_along_the_line() {
    let s = store();
    let one = s.shape_line("I", "100px Liberation Sans", false);
    let two = s.shape_line("II", "100px Liberation Sans", false);
    let [a0, _, a1, _] = bbox(&one.outline).unwrap();
    let [b0, _, b1, _] = bbox(&two.outline).unwrap();
    assert!((a0 - b0).abs() < 1e-9);
    assert!((b1 - (a1 + one.width)).abs() < 1e-6, "{b1} {a1} {}", one.width);
}

#[test]
fn fallback_faces_draw_what_the_first_family_lacks() {
    // "a" from Excalifont, "中" from Xiaolai: two runs, both drawn.
    let s = store();
    let font = get_font_string(40.0, FontFamily::EXCALIFONT);
    let a = s.shape_line("a", &font, false);
    let zhong = s.shape_line("中", &font, false);
    let both = s.shape_line("a中", &font, false);
    assert!(!a.outline.is_empty() && !zhong.outline.is_empty());
    assert_eq!(both.outline.len(), a.outline.len() + zhong.outline.len());
    // Left to right: the CJK glyph starts after the "a".
    let [z0, ..] = bbox(&both.outline[a.outline.len()..]).unwrap();
    assert!(z0 >= a.width - 1e-9, "{z0} < {}", a.width);
}

#[test]
fn right_to_left_lines_lay_runs_out_from_the_right() {
    // With the canvas's `direction` rtl the runs of "a中" read right to
    // left: the CJK run is drawn first, at the left end.
    let s = store();
    let font = get_font_string(40.0, FontFamily::EXCALIFONT);
    let a = s.shape_line("a", &font, false);
    let zhong = s.shape_line("中", &font, false);
    let rtl = s.shape_line("a中", &font, true);
    assert!((rtl.width - (a.width + zhong.width)).abs() < 1e-9);
    let [_, _, z1, _] = bbox(&rtl.outline[..zhong.outline.len()]).unwrap();
    let [a0, ..] = bbox(&rtl.outline[zhong.outline.len()..]).unwrap();
    assert!(z1 <= zhong.width + 1e-9, "{z1}");
    assert!(a0 >= zhong.width - 1e-9, "{a0}");
}

#[test]
fn hebrew_is_shaped_right_to_left_within_its_run() {
    // "שלום": the first letter (shin) is the rightmost glyph.
    let s = store();
    let font = "40px Nunito, Liberation Sans";
    let word = s.shape_line("\u{5E9}\u{5DC}\u{5D5}\u{5DD}", font, true);
    let shin = s.shape_line("\u{5E9}", font, true);
    let [_, _, word_max, _] = bbox(&word.outline).unwrap();
    let [shin_min, _, shin_max, _] = bbox(&shin.outline).unwrap();
    let offset = word.width - shin.width;
    assert!((word_max - (shin_max + offset)).abs() < 1e-6);
    assert!(shin_min + offset > 0.0);
}
