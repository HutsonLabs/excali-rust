//! The registered font faces (`Fonts.init()`,
//! `packages/excalidraw/fonts/Fonts.ts:375-416`, the descriptor arrays in
//! `fonts/*/index.ts` and `fonts/fonts.css`) and the files the port vendors
//! for them under `assets/fonts/` (ADR-004). The table itself is generated from
//! upstream by `scripts/fonts/font_faces.py`, which CI re-runs against the
//! pinned checkout.

mod common;

use std::collections::BTreeMap;

use excali_core::element::FontFamily;
use excali_text::font_faces::{font_faces, FONT_FACES};
use excali_text::font_metadata::{font_metrics, GOOGLE_FONTS_RANGES};
use excali_text::unicode_range::UnicodeRange;

#[test]
fn faces_per_family_in_registration_order() {
    let mut order: Vec<&str> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for face in FONT_FACES.iter() {
        if order.last() != Some(&face.family) {
            order.push(face.family);
        }
        *counts.entry(face.family).or_default() += 1;
    }
    assert_eq!(
        order,
        vec![
            "Cascadia",
            "Comic Shanns",
            "Excalifont",
            "Helvetica",
            "Liberation Sans",
            "Lilita One",
            "Nunito",
            "Virgil",
            "Xiaolai",
            "Segoe UI Emoji",
            "Assistant",
        ]
    );
    let expected: BTreeMap<&str, usize> = [
        ("Assistant", 4),
        ("Cascadia", 1),
        ("Comic Shanns", 4),
        ("Excalifont", 7),
        ("Helvetica", 1),
        ("Liberation Sans", 1),
        ("Lilita One", 2),
        ("Nunito", 5),
        ("Segoe UI Emoji", 1),
        ("Virgil", 1),
        ("Xiaolai", 209),
    ]
    .into_iter()
    .collect();
    assert_eq!(counts, expected);
}

#[test]
fn local_faces_are_helvetica_and_emoji_only() {
    let local: Vec<&str> = FONT_FACES
        .iter()
        .filter(|f| f.is_local())
        .map(|f| f.family)
        .collect();
    assert_eq!(local, vec!["Helvetica", "Segoe UI Emoji"]);
    assert!(FONT_FACES
        .iter()
        .all(|f| f.is_local() == f.vendored.is_none()));
}

#[test]
fn every_vendored_file_is_upstreams_except_liberation_sans() {
    for face in FONT_FACES.iter().filter(|f| !f.is_local()) {
        let (upstream, vendored) = (face.upstream.unwrap(), face.vendored.unwrap());
        if face.family == "Liberation Sans" {
            // ADR-004: upstream's 1.05 file is a licence gap; the port ships
            // the OFL build 2.1.5.
            assert_eq!(upstream, "Liberation/LiberationSans-Regular.woff2");
            assert_eq!(vendored, "Liberation/LiberationSans-Regular.ttf");
        } else {
            assert_eq!(upstream, vendored);
        }
        assert!(
            common::fonts_dir().join(vendored).is_file(),
            "assets/fonts/{vendored} is not vendored"
        );
    }
}

#[test]
fn ranges_parse_and_google_ranges_are_upstreams_constants() {
    for face in FONT_FACES.iter() {
        if let Some(range) = face.unicode_range {
            UnicodeRange::parse(range).unwrap_or_else(|e| panic!("{face:?}: {e}"));
        }
    }
    let nunito: Vec<_> = font_faces("Nunito").map(|f| f.unicode_range).collect();
    assert_eq!(
        nunito,
        vec![
            Some(GOOGLE_FONTS_RANGES.cyrilic_ext),
            Some(GOOGLE_FONTS_RANGES.cyrilic),
            Some(GOOGLE_FONTS_RANGES.vietnamese),
            Some(GOOGLE_FONTS_RANGES.latin_ext),
            Some(GOOGLE_FONTS_RANGES.latin),
        ]
    );
    assert!(font_faces("Nunito").all(|f| f.weight == Some("500")));
    let lilita: Vec<_> = font_faces("lilita one").map(|f| f.unicode_range).collect();
    assert_eq!(
        lilita,
        vec![
            Some(GOOGLE_FONTS_RANGES.latin_ext),
            Some(GOOGLE_FONTS_RANGES.latin)
        ]
    );
    let assistant: Vec<_> = font_faces("Assistant").map(|f| f.weight).collect();
    assert_eq!(
        assistant,
        vec![Some("400"), Some("500"), Some("600"), Some("700")]
    );
}

#[test]
fn every_vendored_face_loads_with_its_familys_units_per_em() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/font-advances.json")).unwrap();
    let faces = fixture["faces"].as_array().unwrap();
    assert_eq!(
        faces.len(),
        FONT_FACES.iter().filter(|f| !f.is_local()).count()
    );
    for face in faces {
        let family = face["family"].as_str().unwrap();
        // FONT_METADATA's unitsPerEm (font-metadata.ts:35-134) is the
        // file's; Assistant has no entry and uses Excalifont's metrics
        // upstream, so it is checked against the file alone.
        if let Some(id) = FontFamily::from_name(family) {
            if id != FontFamily::ASSISTANT {
                assert_eq!(
                    face["unitsPerEm"].as_u64().unwrap(),
                    u64::from(font_metrics(id).units_per_em),
                    "{}",
                    face["file"]
                );
            }
        }
    }
    assert!(common::store().families().count() >= 9);
}
