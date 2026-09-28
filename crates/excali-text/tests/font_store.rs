//! Measuring lines from the vendored font files: WOFF2 decoding, glyph
//! advances, the family fallback chain of upstream's font strings,
//! `unicode-range` and weight matching, shaping, and font string parsing.
//!
//! `tests/fixtures/font-advances.json` holds glyph advances read from the
//! same files with fontTools (`scripts/fonts/advances.py`), an
//! implementation independent of the port's decoder and parser.

mod common;

use common::{font_file, store};
use excali_core::element::FontFamily;
use excali_text::font_metadata::get_font_string;
use excali_text::font_store::{decode_font_file, parse_font, FontError, FontSpec, FontStore};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::Value;

const EXCALIFONT_LATIN: &str =
    "Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2";
const VIRGIL: &str = "Virgil/Virgil-Regular.woff2";

fn font(size: f64, family: FontFamily) -> String {
    get_font_string(size, family)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn woff2_files_decode_to_sfnt_and_ttf_passes_through() {
    let sfnt = decode_font_file(&font_file(EXCALIFONT_LATIN)).unwrap();
    assert!(
        sfnt.starts_with(&[0, 1, 0, 0]) || sfnt.starts_with(b"OTTO"),
        "{:?}",
        &sfnt[..4]
    );
    let ttf = font_file("Liberation/LiberationSans-Regular.ttf");
    assert_eq!(decode_font_file(&ttf).unwrap(), ttf);
    assert!(matches!(
        decode_font_file(b"wOF2 not really a font"),
        Err(FontError::Decode(_))
    ));
}

#[test]
fn bad_faces_are_rejected() {
    let mut s = FontStore::new();
    assert_eq!(
        s.add_face("T", b"not a font at all", None, None),
        Err(FontError::Parse)
    );
    let latin = font_file(EXCALIFONT_LATIN);
    assert!(matches!(
        s.add_face("T", &latin, Some("U+7e-20"), None),
        Err(FontError::UnicodeRange(_))
    ));
    assert_eq!(
        s.add_face("T", &latin, None, Some("heavy")),
        Err(FontError::Weight("heavy".to_owned()))
    );
    assert_eq!(s.families().count(), 0);
    assert_eq!(s.line_width("abc", "20px T"), 0.0);
}

#[test]
fn single_characters_measure_their_glyph_advance() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/font-advances.json")).unwrap();
    let store = store();
    let mut checked = 0;
    for face in fixture["faces"].as_array().unwrap() {
        let family = face["family"].as_str().unwrap();
        let weight = face["weight"].as_u64().unwrap();
        let upem = face["unitsPerEm"].as_f64().unwrap();
        let font = format!("{weight} 100px \"{family}\"");
        for sample in face["advances"].as_array().unwrap() {
            let cp = u32::try_from(sample[0].as_u64().unwrap()).unwrap();
            let ch = char::from_u32(cp).unwrap();
            let expected = sample[1].as_f64().unwrap() * 100.0 / upem;
            let got = store.line_width(&ch.to_string(), &font);
            assert!(
                close(got, expected),
                "{} U+{cp:04X} in {font:?}: {got} != {expected}",
                face["file"]
            );
            assert_eq!(store.family_for(ch, &font), Some(family));
            checked += 1;
        }
    }
    assert!(checked > 1500, "{checked}");
}

#[test]
fn width_scales_with_font_size() {
    let store = store();
    let at_20 = store.line_width("Hello, world", &font(20.0, FontFamily::EXCALIFONT));
    let at_33 = store.line_width("Hello, world", &font(33.3, FontFamily::EXCALIFONT));
    assert!(at_20 > 0.0);
    assert!(close(at_33, at_20 * 33.3 / 20.0));
}

#[test]
fn provider_is_line_width() {
    let store = store();
    let f = font(20.0, FontFamily::NUNITO);
    assert_eq!(
        store.get_line_width("Nunito", &f),
        store.line_width("Nunito", &f)
    );
}

#[test]
fn excalifont_falls_back_to_xiaolai_for_cjk() {
    let store = store();
    let excalifont = font(20.0, FontFamily::EXCALIFONT);
    assert_eq!(store.family_for('A', &excalifont), Some("Excalifont"));
    assert_eq!(store.family_for('\u{4E2D}', &excalifont), Some("Xiaolai"));
    assert_eq!(
        store.line_width("\u{4E2D}\u{6587}", &excalifont),
        store.line_width("\u{4E2D}\u{6587}", "20px Xiaolai")
    );
    // Mixed text: each run on its own face.
    let mixed = store.line_width("ab\u{4E2D}cd", &excalifont);
    let parts = store.line_width("ab", &excalifont)
        + store.line_width("\u{4E2D}", "20px Xiaolai")
        + store.line_width("cd", &excalifont);
    assert!(close(mixed, parts), "{mixed} != {parts}");
    // Other families have no Xiaolai in their list (constants.ts:182-197).
    assert_ne!(
        store.family_for('\u{4E2D}', &font(20.0, FontFamily::NUNITO)),
        Some("Xiaolai")
    );
}

#[test]
fn generic_families_map_to_vendored_faces() {
    let store = store();
    // Helvetica is local-only upstream: sans-serif, Liberation Sans, draws it.
    let helvetica = font(20.0, FontFamily::HELVETICA);
    assert_eq!(store.family_for('A', &helvetica), Some("Liberation Sans"));
    assert_eq!(
        store.line_width("Helvetica", &helvetica),
        store.line_width("Helvetica", "20px Liberation Sans")
    );
    // Virgil has no Omega: its sans-serif fallback does.
    let virgil = font(20.0, FontFamily::VIRGIL);
    assert_eq!(store.family_for('\u{416}', &virgil), Some("Virgil"));
    assert_eq!(
        store.family_for('\u{3A9}', &virgil),
        Some("Liberation Sans")
    );
    // Comic Shanns has no Cyrillic: its monospace fallback, Cascadia, does.
    let comic = font(20.0, FontFamily::COMIC_SHANNS);
    assert_eq!(store.family_for('a', &comic), Some("Comic Shanns"));
    assert_eq!(store.family_for('\u{416}', &comic), Some("Cascadia"));

    let mut unmapped = store.clone();
    unmapped.set_generic_family("sans-serif", None);
    assert_eq!(unmapped.family_for('A', &helvetica), None);
    unmapped.set_generic_family("SANS-SERIF", Some("Virgil"));
    assert_eq!(unmapped.family_for('A', &helvetica), Some("Virgil"));
}

#[test]
fn unmapped_characters_measure_the_first_familys_notdef() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/font-advances.json")).unwrap();
    let notdef = fixture["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == EXCALIFONT_LATIN)
        .unwrap()["notdefAdvance"]
        .as_f64()
        .unwrap();
    let store = store();
    let excalifont = font(20.0, FontFamily::EXCALIFONT);
    // No vendored face maps U+1F600 (Segoe UI Emoji is local-only).
    assert_eq!(store.family_for('\u{1F600}', &excalifont), None);
    let width = store.line_width("\u{1F600}", &excalifont);
    assert!(close(width, notdef * 20.0 / 1000.0), "{width}");
}

#[test]
fn marks_and_default_ignorables_stay_with_their_base() {
    let store = store();
    let excalifont = font(20.0, FontFamily::EXCALIFONT);
    // Zero width space and a variation selector: no advance.
    assert!(close(
        store.line_width("a\u{200B}b", &excalifont),
        store.line_width("ab", &excalifont)
    ));
    assert!(close(
        store.line_width("a\u{FE0F}", &excalifont),
        store.line_width("a", &excalifont)
    ));
    // A combining acute is positioned over its base, not advanced.
    let base = store.line_width("e", &excalifont);
    let accented = store.line_width("e\u{301}", &excalifont);
    assert!(close(accented, base), "{accented} != {base}");
}

#[test]
fn kerning_applies_within_a_run() {
    let store = store();
    let excalifont = font(100.0, FontFamily::EXCALIFONT);
    let pairs = ["AV", "To", "Te", "Yo", "LT", "P.", "r,"];
    let kerned = pairs.iter().filter(|p| {
        let separate: f64 = p
            .chars()
            .map(|c| store.line_width(&c.to_string(), &excalifont))
            .sum();
        !close(store.line_width(p, &excalifont), separate)
    });
    assert!(kerned.count() > 0, "no kern pair among {pairs:?}");
}

#[test]
fn unicode_range_limits_a_face() {
    let latin = font_file(EXCALIFONT_LATIN);
    let mut s = FontStore::new();
    s.add_face("T", &latin, Some("U+41"), None).unwrap();
    assert_eq!(s.family_for('A', "20px T"), Some("T"));
    // The file maps B, but its range does not list it.
    assert_eq!(s.family_for('B', "20px T"), None);
}

#[test]
fn overlapping_ranges_try_the_last_registered_face_first() {
    let virgil = font_file(VIRGIL);
    let latin = font_file(EXCALIFONT_LATIN);
    let mut s = FontStore::new();
    s.add_face("Virgil", &virgil, None, None).unwrap();
    s.add_face("Excalifont", &latin, None, None).unwrap();
    s.add_face("T", &virgil, Some("U+41-5A"), None).unwrap();
    s.add_face("T", &latin, Some("U+41"), None).unwrap();
    assert_eq!(
        s.line_width("A", "20px T"),
        s.line_width("A", "20px Excalifont")
    );
    assert_eq!(
        s.line_width("B", "20px T"),
        s.line_width("B", "20px Virgil")
    );
    assert_ne!(
        s.line_width("A", "20px Excalifont"),
        s.line_width("A", "20px Virgil")
    );
}

#[test]
fn weight_matching() {
    let store = store();
    let mut by_weight = FontStore::new();
    for (name, file) in [
        ("R", "Assistant/Assistant-Regular.woff2"),
        ("M", "Assistant/Assistant-Medium.woff2"),
        ("S", "Assistant/Assistant-SemiBold.woff2"),
        ("B", "Assistant/Assistant-Bold.woff2"),
    ] {
        by_weight
            .add_face(name, &font_file(file), None, None)
            .unwrap();
    }
    let text = "Assistant weights";
    let w = |f: &str| store.line_width(text, f);
    let single = |f: &str| by_weight.line_width(text, &format!("20px {f}"));
    let (r, m, sb, b) = (single("R"), single("M"), single("S"), single("B"));
    assert!(r < m && m < sb && sb < b, "{r} {m} {sb} {b}");
    assert_eq!(w("20px Assistant"), r);
    assert_eq!(w("normal 20px Assistant"), r);
    assert_eq!(w("500 20px Assistant"), m);
    assert_eq!(w("600 20px Assistant"), sb);
    assert_eq!(w("bold 20px Assistant"), b);
    // 400-500: heavier up to 500 first.
    assert_eq!(w("450 20px Assistant"), m);
    // Below 400: lighter first, none, then heavier ascending.
    assert_eq!(w("300 20px Assistant"), r);
    // Above 500: heavier first, then lighter descending.
    assert_eq!(w("800 20px Assistant"), b);
    assert_eq!(w("550 20px Assistant"), sb);
    // Nunito registers only weight 500 (fonts/Nunito/index.ts): normal text
    // uses it.
    let nunito = font(20.0, FontFamily::NUNITO);
    assert_eq!(store.family_for('N', &nunito), Some("Nunito"));
    assert_eq!(
        store.line_width("Nunito", &nunito),
        store.line_width("Nunito", "500 20px Nunito")
    );
}

#[test]
fn font_string_parsing() {
    assert_eq!(
        parse_font("20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"),
        Some(FontSpec {
            size: 20.0,
            weight: 400,
            families: vec![
                "Excalifont".into(),
                "Xiaolai".into(),
                "sans-serif".into(),
                "Segoe UI Emoji".into()
            ],
        })
    );
    assert_eq!(
        parse_font("italic bold 16.5px/1.2 \"Lilita One\" ,  'Comic Shanns', MONOSPACE"),
        Some(FontSpec {
            size: 16.5,
            weight: 700,
            families: vec![
                "Lilita One".into(),
                "Comic Shanns".into(),
                "monospace".into()
            ],
        })
    );
    assert_eq!(
        parse_font("300 1e1px Virgil").map(|s| (s.size, s.weight)),
        Some((10.0, 300))
    );
    for bad in [
        "",
        "20px",
        "20 Virgil",
        "20pt Virgil",
        "heavy 20px Virgil",
        "20px Virgil,",
        "20px , Virgil",
        "-1px Virgil",
        "1001 20px Virgil",
    ] {
        assert_eq!(parse_font(bad), None, "{bad:?}");
    }
}

#[test]
fn invalid_font_strings_measure_in_the_canvas_default() {
    let store = store();
    assert_eq!(
        store.line_width("default", "not a font"),
        store.line_width("default", "10px sans-serif")
    );
    assert!(store.line_width("default", "10px sans-serif") > 0.0);
}

#[test]
fn family_names_are_case_insensitive() {
    let store = store();
    assert_eq!(
        store.line_width("Case", "20px excalifont"),
        store.line_width("Case", "20px Excalifont")
    );
    assert_eq!(store.family_for('C', "20px LILITA ONE"), Some("Lilita One"));
}

/// Per-line cost. Wrapping (ex-303) measures each character through
/// `CharWidthCache` and every line again on each edit, so a measurement may
/// only shape: faces are parsed when loaded and shaping plans built once.
/// Before that, a single character took about 1.7 ms in Cascadia (debug
/// build, Apple M-series); now about 11 us. The bounds leave a wide margin
/// for slower CI machines and still fail if a face is parsed or a plan
/// built on every call.
#[test]
fn measuring_a_line_only_shapes() {
    const CHAR_BOUND: std::time::Duration = std::time::Duration::from_micros(250);
    const LINE_BOUND: std::time::Duration = std::time::Duration::from_millis(2);
    const N: u32 = 500;
    let store = store();
    for family in [
        FontFamily::VIRGIL,
        FontFamily::EXCALIFONT,
        FontFamily::NUNITO,
        FontFamily::CASCADIA,
        FontFamily::LILITA_ONE,
        FontFamily::COMIC_SHANNS,
        FontFamily::LIBERATION_SANS,
        FontFamily::ASSISTANT,
    ] {
        let f = font(20.0, family);
        // Warm: the first call per face and script builds its plan.
        store.line_width("warm up 0", &f);
        let start = std::time::Instant::now();
        for i in 0..N {
            let ch = char::from(b'a' + u8::try_from(i % 26).unwrap());
            std::hint::black_box(store.line_width(ch.encode_utf8(&mut [0; 4]), &f));
        }
        let per_char = start.elapsed() / N;
        let start = std::time::Instant::now();
        for i in 0..N {
            let line = format!("The quick brown fox jumps over {i}");
            std::hint::black_box(store.line_width(&line, &f));
        }
        let per_line = start.elapsed() / N;
        assert!(per_char < CHAR_BOUND, "{f}: {per_char:?} per character");
        assert!(per_line < LINE_BOUND, "{f}: {per_line:?} per line");
    }
}
