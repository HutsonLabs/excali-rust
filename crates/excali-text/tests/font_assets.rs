//! Font asset manifest and scene font selection (ex-307): upstream's font
//! registry (`Fonts.init`, `packages/excalidraw/fonts/Fonts.ts:375-416`),
//! `ExcalidrawFontFace`'s unicode-range test (`ExcalidrawFontFace.ts:114-131`),
//! `containsCJK` (`packages/element/src/textWrapping.ts:30-36`), the
//! characters `loadSceneFonts` / `loadElementsFonts` ask the browser for
//! (`Fonts.ts:153-177, 249-284`) and `generateFontFaceDeclarations`
//! (`Fonts.ts:182-217`).
//!
//! Fixture: `tests/fixtures/font-assets.json`, upstream's own output at the
//! pinned commit (`tools/goldens/font-assets.mjs`). Manifest:
//! `assets/fonts/manifest.json` (`scripts/fonts/assets.py`).

use std::collections::BTreeSet;
use std::path::Path;

use excali_core::element::{Element, ElementBase, ElementKind, FontFamily, TextFields};
use excali_text::font_assets::{
    contains_cjk, elements_font_loads, faces_to_load, font_face_declarations, parse_unicode_range,
    registered_families, registered_family, registered_family_named, scene_font_loads,
    ui_font_faces, FontFaceAsset, FontFormat, ASSETS_FALLBACK_URL, CJK_RANGES, FULL_UNICODE_RANGE,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/font-assets.json")).unwrap()
}

fn manifest() -> Value {
    serde_json::from_str(include_str!("../assets/fonts/manifest.json")).unwrap()
}

fn assets_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts"))
}

fn text_element(id: usize, family: u32, text: &str, deleted: bool) -> Element {
    let mut base = ElementBase::new(format!("t{id}"), 0.0, 0.0, 1.0, 0.0);
    base.is_deleted = deleted;
    Element::new(
        base,
        ElementKind::Text(TextFields::new(text, FontFamily(family), 1.25)),
    )
}

/// The fixture scene's elements as model elements.
fn scene_elements(scene: &Value) -> Vec<Element> {
    scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let deleted = e["isDeleted"].as_bool().unwrap();
            match e["type"].as_str().unwrap() {
                "text" => text_element(
                    i,
                    u32::try_from(e["fontFamily"].as_u64().unwrap()).unwrap(),
                    e["originalText"].as_str().unwrap(),
                    deleted,
                ),
                "rectangle" => {
                    let mut base = ElementBase::new(format!("r{i}"), 0.0, 0.0, 1.0, 0.0);
                    base.is_deleted = deleted;
                    Element::new(base, ElementKind::Rectangle)
                }
                other => panic!("fixture element type {other}"),
            }
        })
        .collect()
}

fn scenes() -> Vec<Value> {
    fixture()["scenes"].as_array().unwrap().clone()
}

fn scene(name: &str) -> Value {
    scenes()
        .into_iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no fixture scene {name}"))
}

/// Code points either side of both ends of every range, as the generator's
/// `probeCodePoints` lists them.
fn probe_code_points(ranges: &[(u32, u32)]) -> Vec<u32> {
    ranges
        .iter()
        .flat_map(|&(a, b)| {
            [a.checked_sub(1), Some(a), Some(b), b.checked_add(1)]
                .into_iter()
                .flatten()
                .filter(|&cp| cp <= 0x10FFFF)
        })
        .collect()
}

#[test]
fn registry_is_upstreams_in_its_order() {
    let g = fixture();
    let upstream = g["registered"].as_array().unwrap();
    let ours = registered_families();
    assert_eq!(ours.len(), upstream.len());
    for (fam, up) in ours.iter().zip(upstream) {
        assert_eq!(u64::from(fam.id.0), up["id"].as_u64().unwrap());
        assert_eq!(fam.family, up["family"].as_str().unwrap());
        assert_eq!(fam.local, up["local"].as_bool().unwrap(), "{}", fam.family);
        let up_faces = up["faces"].as_array().unwrap();
        if fam.local {
            // Local faces register no file and are never fetched
            // (Fonts.ts:225-229, 301-304).
            assert!(fam.faces.is_empty(), "{}", fam.family);
            assert!(up_faces.iter().all(|f| f["file"].is_null()));
            continue;
        }
        assert_eq!(fam.faces.len(), up_faces.len(), "{}", fam.family);
        for (face, up) in fam.faces.iter().zip(up_faces) {
            assert_eq!(face.family, fam.family);
            assert_eq!(face.upstream_file, up["file"].as_str().unwrap());
            assert_eq!(face.unicode_range, up["unicodeRange"].as_str());
            let d = &up["descriptors"];
            assert_eq!(face.weight, d["weight"].as_str().unwrap());
            assert_eq!(face.style, d["style"].as_str().unwrap());
            assert_eq!(face.display, d["display"].as_str().unwrap());
            if face.file == face.upstream_file {
                assert_eq!(face.format, FontFormat::Woff2);
                assert_eq!(up["format"], "format('woff2')");
            }
        }
    }
}

#[test]
fn fallback_urls_are_upstreams_asset_urls() {
    // ExcalidrawFontFace.ASSETS_FALLBACK_URL (ExcalidrawFontFace.ts:11-15),
    // the last url of every bundled face, which getContent answers when no
    // url can be fetched
    let fixture = fixture();
    assert_eq!(
        ASSETS_FALLBACK_URL,
        fixture["assetsFallbackUrl"].as_str().unwrap()
    );
    let face = &registered_family(FontFamily::EXCALIFONT).unwrap().faces[0];
    assert_eq!(
        face.fallback_url(),
        format!("{ASSETS_FALLBACK_URL}{}", face.upstream_file)
    );
    let liberation = &registered_family(FontFamily::LIBERATION_SANS)
        .unwrap()
        .faces[0];
    assert_eq!(
        liberation.fallback_url(),
        "https://esm.sh/@excalidraw/excalidraw/dist/prod/Liberation/LiberationSans-Regular.woff2"
    );
}

#[test]
fn liberation_sans_is_the_one_substituted_file() {
    // ADR-004: upstream's Liberation Sans 1.05 woff2 is a licence gap; the
    // port ships the OFL 2.1.5 ttf in its place.
    let substituted: Vec<&FontFaceAsset> = registered_families()
        .iter()
        .flat_map(|f| f.faces)
        .filter(|f| f.file != f.upstream_file)
        .collect();
    assert_eq!(substituted.len(), 1);
    let lib = substituted[0];
    assert_eq!(lib.family, "Liberation Sans");
    assert_eq!(lib.file, "Liberation/LiberationSans-Regular.ttf");
    assert_eq!(lib.upstream_file, "Liberation/LiberationSans-Regular.woff2");
    assert_eq!(lib.format, FontFormat::TrueType);
    assert_eq!(
        lib.sha256,
        "76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8"
    );
    assert_eq!(lib.format.css(), "truetype");
    assert_eq!(FontFormat::Woff2.css(), "woff2");
}

#[test]
fn families_are_found_by_id_and_by_font_face_name() {
    assert_eq!(
        registered_family(FontFamily::EXCALIFONT).unwrap().family,
        "Excalifont"
    );
    assert_eq!(
        registered_family(FontFamily::EXCALIFONT)
            .unwrap()
            .faces
            .len(),
        7
    );
    assert_eq!(
        registered_family(FontFamily::XIAOLAI).unwrap().faces.len(),
        209
    );
    assert!(registered_family(FontFamily::HELVETICA).unwrap().local);
    assert!(registered_family(FontFamily::ASSISTANT).is_none());
    assert!(registered_family(FontFamily(42)).is_none());
    assert_eq!(
        registered_family_named("Lilita One").unwrap().id,
        FontFamily::LILITA_ONE
    );
    assert_eq!(
        registered_family_named("Segoe UI Emoji").unwrap().id,
        FontFamily::SEGOE_UI_EMOJI
    );
    assert!(registered_family_named("sans-serif").is_none());
    assert!(registered_family_named("excalifont").is_none());
}

#[test]
fn unicode_ranges_parse_as_upstream_reads_them() {
    for fam in registered_families() {
        for face in fam.faces {
            let css = face.unicode_range_css();
            assert_eq!(css, face.unicode_range.unwrap_or(FULL_UNICODE_RANGE));
            assert_eq!(
                parse_unicode_range(css).unwrap(),
                face.ranges,
                "{}",
                face.file
            );
        }
    }
    assert_eq!(
        parse_unicode_range("U+0000-00FF, U+0131,U+2000-206F").unwrap(),
        vec![(0, 0xff), (0x131, 0x131), (0x2000, 0x206f)]
    );
    assert_eq!(
        parse_unicode_range("U+0-10FFFF").unwrap(),
        vec![(0, 0x10ffff)]
    );
    assert_eq!(
        parse_unicode_range("u+41").unwrap_err().to_string(),
        "invalid unicode-range part \"u+41\""
    );
    // Forms upstream's RegExp cannot express throw there; they are errors here.
    for bad in [
        "",
        "U+",
        "U+20-",
        "U+4??",
        "U+zz",
        "U+110000",
        "U+7e-20",
        "U+20,,U+30",
    ] {
        assert!(parse_unicode_range(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn range_test_matches_upstreams_regex_on_every_boundary() {
    let g = fixture();
    let mut checked = 0;
    for (fam, up) in registered_families()
        .iter()
        .zip(g["registered"].as_array().unwrap())
    {
        for (face, up) in fam.faces.iter().zip(up["faces"].as_array().unwrap()) {
            let probes = probe_code_points(face.ranges);
            let bits: String = probes
                .iter()
                .map(|&cp| if face.covers(cp) { '1' } else { '0' })
                .collect();
            assert_eq!(bits, up["probes"].as_str().unwrap(), "{}", face.file);
            checked += probes.len();
        }
    }
    assert!(checked > 10_000, "{checked}");
}

#[test]
fn matches_is_upstreams_quick_exit_on_a_text() {
    let excalifont = registered_family(FontFamily::EXCALIFONT).unwrap();
    let latin = &excalifont.faces[0];
    assert!(latin.matches("Hello"));
    assert!(latin.matches("你好 a"));
    assert!(!latin.matches("你好"));
    assert!(!latin.matches(""));
    let cascadia = registered_family(FontFamily::CASCADIA).unwrap();
    assert!(cascadia.faces[0].matches("\u{10FFFF}"));
    assert!(cascadia.faces[0].matches("\0"));
}

#[test]
fn contains_cjk_is_upstreams_on_every_code_point() {
    let g = fixture();
    let upstream: Vec<(u32, u32)> = g["cjk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                u32::try_from(r[0].as_u64().unwrap()).unwrap(),
                u32::try_from(r[1].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    assert_eq!(CJK_RANGES.to_vec(), upstream);
    for &(a, b) in &upstream {
        for cp in [a, b] {
            let ch = char::from_u32(cp).unwrap();
            assert!(contains_cjk(&ch.to_string()), "U+{cp:X}");
        }
        for cp in [a - 1, b + 1] {
            if let Some(ch) = char::from_u32(cp) {
                let inside = upstream.iter().any(|&(x, y)| cp >= x && cp <= y);
                assert_eq!(contains_cjk(&ch.to_string()), inside, "U+{cp:X}");
            }
        }
    }
    assert!(contains_cjk("Hello 你好"));
    assert!(!contains_cjk("Hello"));
    assert!(!contains_cjk(""));
}

#[test]
fn font_loads_are_the_characters_upstream_asks_the_browser_for() {
    for s in scenes() {
        let elements = scene_elements(&s);
        for (loads, key) in [
            (elements_font_loads(&elements), "elementsFonts"),
            (scene_font_loads(&elements), "sceneFonts"),
        ] {
            let want = s[key].as_array().unwrap();
            assert_eq!(loads.len(), want.len(), "{} {key}", s["name"]);
            for (load, w) in loads.iter().zip(want) {
                assert_eq!(
                    u64::from(load.font_family.0),
                    w["fontFamily"].as_u64().unwrap()
                );
                assert_eq!(
                    load.text,
                    w["characters"].as_str().unwrap(),
                    "{}",
                    s["name"]
                );
                assert_eq!(load.font, w["font"].as_str().unwrap(), "{}", s["name"]);
            }
        }
    }
}

#[test]
fn declarations_are_upstreams_generate_font_face_declarations() {
    for s in scenes() {
        let elements = scene_elements(&s);
        let ours = font_face_declarations(&elements);
        let want = s["declarations"].as_array().unwrap();
        let got: Vec<(String, String, Vec<u64>)> = ours
            .iter()
            .map(|d| {
                (
                    d.face.family.to_owned(),
                    d.face.upstream_file.to_owned(),
                    d.characters
                        .chars()
                        .map(|c| u64::from(u32::from(c)))
                        .collect(),
                )
            })
            .collect();
        let expected: Vec<(String, String, Vec<u64>)> = want
            .iter()
            .map(|d| {
                (
                    d["family"].as_str().unwrap().to_owned(),
                    d["file"].as_str().unwrap().to_owned(),
                    d["codePoints"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c.as_u64().unwrap())
                        .collect(),
                )
            })
            .collect();
        assert_eq!(got, expected, "{}", s["name"]);
    }
}

fn files(faces: &[&FontFaceAsset]) -> Vec<&'static str> {
    faces.iter().map(|f| f.file).collect()
}

fn load_files(name: &str) -> Vec<&'static str> {
    let elements = scene_elements(&scene(name));
    let loads = scene_font_loads(&elements);
    let faces = faces_to_load(&loads);
    files(&faces)
}

/// The files `document.fonts.load(font, text)` fetches: for every family of
/// the font's family list registered with a file, the faces whose
/// unicode-range holds a character of the text (CSS Font Loading, "find the
/// matching font faces"). Checked in Chromium by tests/web.
#[test]
fn a_scene_loads_only_the_ranges_its_text_uses() {
    const EXCALIFONT_LATIN: &str =
        "Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2";
    assert_eq!(load_files("empty"), Vec::<&str>::new());
    assert_eq!(load_files("no-text"), Vec::<&str>::new());
    assert_eq!(load_files("excalifont-latin"), vec![EXCALIFONT_LATIN]);
    assert_eq!(load_files("duplicates"), vec![EXCALIFONT_LATIN]);
    assert_eq!(
        load_files("excalifont-cyrillic-greek"),
        vec![
            EXCALIFONT_LATIN,
            "Excalifont/Excalifont-Regular-b9dcf9d2e50a1eaf42fc664b50a3fd0d.woff2",
            "Excalifont/Excalifont-Regular-41b173a47b57366892116a575a43e2b6.woff2",
        ]
    );
    assert_eq!(load_files("helvetica"), Vec::<&str>::new());
    assert_eq!(load_files("emoji-element"), Vec::<&str>::new());
    assert_eq!(load_files("unknown-family"), Vec::<&str>::new());
    assert_eq!(load_files("excalifont-empty-text"), Vec::<&str>::new());
    assert_eq!(
        load_files("deleted"),
        vec![EXCALIFONT_LATIN],
        "deleted elements load nothing"
    );
    assert_eq!(
        load_files("liberation"),
        vec!["Liberation/LiberationSans-Regular.ttf"]
    );
    assert_eq!(
        load_files("nunito-latin"),
        vec!["Nunito/Nunito-Regular-XRXI3I6Li01BKofiOc5wtlZ2di8HDIkhdTQ3j6zbXWjgeg.woff2"]
    );
    // CJK in Excalifont: its Latin face and only the Xiaolai files holding
    // those characters, out of 209.
    let cjk = load_files("excalifont-cjk");
    assert_eq!(cjk[0], EXCALIFONT_LATIN);
    assert!(cjk[1..].iter().all(|f| f.starts_with("Xiaolai/")));
    assert!(cjk.len() > 2 && cjk.len() < 12, "{cjk:?}");
    // Xiaolai is Excalifont's fallback only (constants.ts:182-197).
    assert_eq!(load_files("nunito-cjk"), Vec::<&str>::new());
}

#[test]
fn load_plan_agrees_with_upstreams_declarations_where_both_apply() {
    // For one family and text with CJK (so Xiaolai is in both), the files the
    // browser loads are the files upstream inlines into an SVG.
    for name in [
        "excalifont-cjk",
        "excalifont-cjk-only",
        "excalifont-fullwidth",
    ] {
        let elements = scene_elements(&scene(name));
        let loaded: BTreeSet<&str> = files(&faces_to_load(&scene_font_loads(&elements)))
            .into_iter()
            .collect();
        let declared: BTreeSet<&str> = font_face_declarations(&elements)
            .iter()
            .map(|d| d.face.file)
            .collect();
        assert_eq!(loaded, declared, "{name}");
    }
}

#[test]
fn table_equals_manifest_json() {
    let m = manifest();
    assert_eq!(m["upstream"], fixture()["upstream"]);
    let fams = m["families"].as_array().unwrap();
    assert_eq!(fams.len(), registered_families().len());
    let face_json = |face: &FontFaceAsset| {
        serde_json::json!({
            "file": face.file,
            "format": face.format.css(),
            "unicodeRange": face.unicode_range,
            "weight": face.weight,
            "style": face.style,
            "display": face.display,
            "size": face.size,
            "sha256": face.sha256,
            "upstreamFile": face.upstream_file,
        })
    };
    for (fam, j) in registered_families().iter().zip(fams) {
        assert_eq!(u64::from(fam.id.0), j["id"].as_u64().unwrap());
        assert_eq!(fam.family, j["family"]);
        assert_eq!(fam.local, j["local"]);
        let faces: Vec<Value> = fam.faces.iter().map(face_json).collect();
        assert_eq!(&Value::Array(faces), &j["faces"]);
    }
    assert_eq!(m["ui"]["family"], "Assistant");
    let ui: Vec<Value> = ui_font_faces().iter().map(face_json).collect();
    assert_eq!(&Value::Array(ui), &m["ui"]["faces"]);
    assert_eq!(
        ui_font_faces().iter().map(|f| f.weight).collect::<Vec<_>>(),
        ["400", "500", "600", "700"]
    );
}

#[test]
fn every_manifest_file_is_on_disk_with_its_size() {
    let mut n = 0;
    for face in registered_families()
        .iter()
        .flat_map(|f| f.faces)
        .chain(ui_font_faces())
    {
        let meta = std::fs::metadata(assets_dir().join(face.file))
            .unwrap_or_else(|e| panic!("{}: {e}", face.file));
        assert_eq!(meta.len(), face.size, "{}", face.file);
        n += 1;
    }
    assert_eq!(n, 1 + 4 + 7 + 1 + 2 + 5 + 1 + 209 + 4);
}

#[test]
fn src_is_the_url_under_the_base_with_its_format() {
    let excalifont = registered_family(FontFamily::EXCALIFONT).unwrap();
    assert_eq!(
        excalifont.faces[0].src("https://host/vendor/excali/fonts/"),
        "url(\"https://host/vendor/excali/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2\") format(\"woff2\")"
    );
    // A base without the trailing slash gets one, as normalizeBaseUrl does
    // (ExcalidrawFontFace.ts:191-208).
    let lib = registered_family(FontFamily::LIBERATION_SANS).unwrap();
    assert_eq!(
        lib.faces[0].src("fonts"),
        "url(\"fonts/Liberation/LiberationSans-Regular.ttf\") format(\"truetype\")"
    );
}
