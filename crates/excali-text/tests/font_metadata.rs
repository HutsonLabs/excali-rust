//! Font metadata and the vertical offset formula (ex-301):
//! `FONT_METADATA`, `getVerticalOffset` and `getLineHeight`
//! (`packages/common/src/font-metadata.ts:35-181`), `getLineHeightInPx`
//! (`packages/element/src/textMeasurements.ts:91-96`), the family fallbacks
//! (`packages/common/src/constants.ts:129-197`) and `getFontFamilyString` /
//! `getFontString` (`packages/common/src/utils.ts:123-147`).
//!
//! Fixture: `tests/fixtures/font-metadata.json`, upstream's own output at the
//! pinned commit (`tools/goldens/font-metadata.mjs`).

use excali_core::element::FontFamily;
use excali_text::font_metadata::{
    font_metadata, font_metrics, get_font_family_fallbacks, get_font_family_string,
    get_font_string, get_generic_font_family_fallback, get_line_height, get_line_height_in_px,
    get_vertical_offset, FontMetadata, CJK_HAND_DRAWN_FALLBACK_FONT, FONT_METADATA,
    GOOGLE_FONTS_RANGES, LOCAL_FONT_PROTOCOL, MONOSPACE_GENERIC_FONT, SANS_SERIF_GENERIC_FONT,
    WINDOWS_EMOJI_FALLBACK_FONT,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/font-metadata.json")).unwrap()
}

fn family(v: &Value) -> FontFamily {
    FontFamily(u32::try_from(v.as_u64().unwrap()).unwrap())
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

/// One `FONT_METADATA` entry as upstream writes it: optional flags only
/// when set.
fn metadata_json(id: FontFamily, meta: &FontMetadata) -> Value {
    let m = &meta.metrics;
    let mut entry = json!({
        "id": id.0,
        "metrics": {
            "unitsPerEm": m.units_per_em,
            "ascender": m.ascender,
            "descender": m.descender,
            "lineHeight": m.line_height,
        },
    });
    let obj = entry.as_object_mut().unwrap();
    for (key, set) in [
        ("deprecated", meta.deprecated),
        ("private", meta.private),
        ("local", meta.local),
        ("fallback", meta.fallback),
    ] {
        if set {
            obj.insert(key.to_owned(), json!(true));
        }
    }
    entry
}

/// JSON numbers compare by value: `886` and `886.0` are the same entry.
fn normalise(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalise).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn metadata_table_is_upstreams_in_key_order() {
    let fixture = fixture();
    let expected: Vec<Value> = fixture["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .map(normalise)
        .collect();
    let actual: Vec<Value> = FONT_METADATA
        .iter()
        .map(|(id, meta)| normalise(&metadata_json(*id, meta)))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn ten_families_plus_fallbacks_have_metrics() {
    // Nine element families (FONT_FAMILY, 4 unused) plus Xiaolai and
    // Segoe UI Emoji; the generic fallbacks 998/999 have no entry.
    assert_eq!(FONT_METADATA.len(), 11);
    for f in FontFamily::ELEMENT_FAMILIES {
        let meta = font_metadata(f).unwrap_or_else(|| panic!("no metadata for {f:?}"));
        assert!(!meta.fallback, "{f:?}");
    }
    for f in [FontFamily::XIAOLAI, FontFamily::SEGOE_UI_EMOJI] {
        assert!(font_metadata(f).unwrap().fallback, "{f:?}");
    }
    for f in [
        FontFamily(0),
        FontFamily(4),
        FontFamily::SANS_SERIF,
        FontFamily::MONOSPACE,
        FontFamily(1001),
    ] {
        assert!(font_metadata(f).is_none(), "{f:?}");
        // Excalifont's metrics stand in (font-metadata.ts:160-162).
        assert_eq!(
            font_metrics(f),
            &font_metadata(FontFamily::EXCALIFONT).unwrap().metrics
        );
    }
}

#[test]
fn metrics_match_the_research_table() {
    // site/content/research/rendering.md section 7.
    let rows: [(FontFamily, u16, f64, f64, f64); 10] = [
        (FontFamily::EXCALIFONT, 1000, 886.0, -374.0, 1.25),
        (FontFamily::NUNITO, 1000, 1011.0, -353.0, 1.25),
        (FontFamily::LILITA_ONE, 1000, 923.0, -220.0, 1.15),
        (FontFamily::COMIC_SHANNS, 1000, 750.0, -250.0, 1.25),
        (FontFamily::VIRGIL, 1000, 886.0, -374.0, 1.25),
        (FontFamily::HELVETICA, 2048, 1577.0, -471.0, 1.15),
        (FontFamily::CASCADIA, 2048, 1900.0, -480.0, 1.2),
        (FontFamily::LIBERATION_SANS, 2048, 1854.0, -434.0, 1.15),
        (FontFamily::ASSISTANT, 2048, 1021.0, -287.0, 1.25),
        (FontFamily::XIAOLAI, 1000, 880.0, -144.0, 1.25),
    ];
    for (f, upm, asc, desc, lh) in rows {
        let m = &font_metadata(f).unwrap().metrics;
        assert_eq!(
            (m.units_per_em, m.ascender, m.descender, m.line_height),
            (upm, asc, desc, lh),
            "{f:?}"
        );
    }
    let flags = |f| {
        let m = font_metadata(f).unwrap();
        (m.deprecated, m.private, m.local, m.fallback)
    };
    assert_eq!(flags(FontFamily::VIRGIL), (true, false, false, false));
    assert_eq!(flags(FontFamily::HELVETICA), (true, false, true, false));
    assert_eq!(flags(FontFamily::CASCADIA), (true, false, false, false));
    assert_eq!(
        flags(FontFamily::LIBERATION_SANS),
        (false, true, false, false)
    );
    assert_eq!(flags(FontFamily::ASSISTANT), (false, true, false, false));
    assert_eq!(flags(FontFamily::XIAOLAI), (false, false, false, true));
    assert_eq!(
        flags(FontFamily::SEGOE_UI_EMOJI),
        (false, false, true, true)
    );
    assert_eq!(flags(FontFamily::EXCALIFONT), (false, false, false, false));
}

#[test]
fn line_height_matches_upstream_tests() {
    // textElement.test.ts:188-210.
    assert_eq!(get_line_height_in_px(20.0, 1.25), 25.0);
    assert_eq!(get_line_height(FontFamily::EXCALIFONT), 1.25);
    assert_eq!(get_line_height(FontFamily::CASCADIA), 1.2);
    assert_eq!(get_line_height(FontFamily::DEFAULT), 1.25);
}

#[test]
fn line_height_per_family_is_upstreams() {
    let fixture = fixture();
    for case in fixture["families"].as_array().unwrap() {
        let f = family(&case["id"]);
        assert_eq!(get_line_height(f), num(&case["lineHeight"]), "{f:?}");
    }
}

#[test]
fn line_height_in_px_is_upstreams() {
    let fixture = fixture();
    for case in fixture["lineHeightInPx"].as_array().unwrap() {
        let (size, lh) = (num(&case["fontSize"]), num(&case["lineHeight"]));
        assert_eq!(
            get_line_height_in_px(size, lh).to_bits(),
            num(&case["px"]).to_bits(),
            "{size} {lh}"
        );
    }
}

#[test]
fn vertical_offset_worked_example() {
    // Excalifont at 20px, line height 1.25: em = 0.02, ascent 17.72,
    // descent 7.48, half the 25px line's leftover (-0.2) is the gap.
    let offset = get_vertical_offset(FontFamily::EXCALIFONT, 20.0, 25.0);
    assert!((offset - 17.62).abs() < 1e-12, "{offset}");
    // Unknown families use Excalifont's metrics.
    assert_eq!(get_vertical_offset(FontFamily(4), 20.0, 25.0), offset);
    // Helvetica, 2048 units per em: the baseline sits lower in the line.
    let helvetica = get_vertical_offset(FontFamily::HELVETICA, 20.0, 23.0);
    let em: f64 = 20.0 / 2048.0;
    let expected = em * 1577.0 + (23.0 - em * 1577.0 + em * -471.0) / 2.0;
    assert_eq!(helvetica.to_bits(), expected.to_bits());
}

#[test]
fn vertical_offset_is_upstreams_bit_for_bit() {
    let fixture = fixture();
    let cases = fixture["verticalOffset"].as_array().unwrap();
    let mut checked = 0;
    for case in cases {
        let f = family(&case["fontFamily"]);
        let size = num(&case["fontSize"]);
        let heights = case["lineHeightPx"].as_array().unwrap();
        let offsets = case["offset"].as_array().unwrap();
        assert_eq!(heights.len(), offsets.len());
        for (h, o) in heights.iter().zip(offsets) {
            let got = get_vertical_offset(f, size, num(h));
            assert_eq!(
                got.to_bits(),
                num(o).to_bits(),
                "{f:?} {size} {h}: {got} != {o}"
            );
            checked += 1;
        }
    }
    assert!(checked > 3000, "{checked}");
}

#[test]
fn fallbacks_are_upstreams() {
    let fixture = fixture();
    let names = &fixture["names"];
    assert_eq!(names["cjk"], CJK_HAND_DRAWN_FALLBACK_FONT);
    assert_eq!(names["emoji"], WINDOWS_EMOJI_FALLBACK_FONT);
    assert_eq!(names["sansSerif"], SANS_SERIF_GENERIC_FONT);
    assert_eq!(names["monospace"], MONOSPACE_GENERIC_FONT);
    // The fallback ids and names agree with excali-core's FontFamily.
    for (name, id) in fixture["fallbacks"].as_object().unwrap() {
        assert_eq!(FontFamily::from_name(name), Some(family(id)), "{name}");
    }
    for (name, id) in fixture["fontFamily"].as_object().unwrap() {
        assert_eq!(FontFamily::from_name(name), Some(family(id)), "{name}");
    }
    assert_eq!(family(&fixture["defaultFontFamily"]), FontFamily::DEFAULT);
    for case in fixture["families"].as_array().unwrap() {
        let f = family(&case["id"]);
        assert_eq!(
            get_generic_font_family_fallback(f),
            case["genericFallback"],
            "{f:?}"
        );
        assert_eq!(
            json!(get_font_family_fallbacks(f)),
            case["fallbacks"],
            "{f:?}"
        );
        assert_eq!(get_font_family_string(f), case["fontFamilyString"], "{f:?}");
    }
}

#[test]
fn font_string_is_upstreams() {
    let fixture = fixture();
    for case in fixture["fontString"].as_array().unwrap() {
        let f = family(&case["fontFamily"]);
        let size = num(&case["fontSize"]);
        assert_eq!(get_font_string(size, f), case["font"], "{f:?} {size}");
    }
    assert_eq!(
        get_font_string(20.0, FontFamily::EXCALIFONT),
        "20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"
    );
}

#[test]
fn google_fonts_ranges_and_local_protocol_are_upstreams() {
    let fixture = fixture();
    let expected = fixture["googleFontsRanges"].as_object().unwrap();
    let actual: Vec<(&str, &str)> = GOOGLE_FONTS_RANGES.entries().to_vec();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
            .collect::<Vec<_>>()
    );
    assert_eq!(fixture["localFontProtocol"], LOCAL_FONT_PROTOCOL);
}
