//! SVG metadata scene payload read/write against upstream (ex-112).
//!
//! Upstream at the pinned commit 438d89861f53d8a90ad566113ecac1b83761098f:
//!
//! - `encodeSvgBase64Payload` and `decodeSvgBase64Payload`
//!   (`packages/excalidraw/scene/export.ts:510-563`), `createHTMLComment`
//!   (`export.ts:286-291`);
//! - `stringToBase64` / `base64ToString` (`packages/excalidraw/data/encode.ts:49-58`)
//!   on top of the browser's `btoa` / `atob`;
//! - the tests in `packages/excalidraw/tests/export.test.tsx` ("test
//!   encoding/decoding scene for SVG export", "import embedded svg (legacy
//!   v1)", "import embedded svg (v2)") with their fixtures, and the two
//!   snapshots whose SVG holds an embedded scene:
//!   `tests/__snapshots__/export.test.tsx.snap` ("svg-embdedded scene export
//!   output") and `tests/scene/__snapshots__/export.test.ts.snap` ("with
//!   exportEmbedScene").
//!
//! All of these are vendored byte for byte under `fixtures/upstream`
//! (`scripts/fixtures/corpus.py`); `site/content/research/data-model.md`
//! section 5 describes the format.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use excali_core::encode::{
    atob, base64_to_string, btoa, deflate, encode, string_to_base64, to_byte_string,
    InvalidCharacterError,
};
use excali_core::svg_payload::{
    decode_svg_base64_payload, encode_svg_base64_payload, SvgPayloadError, PAYLOAD_MIME_TYPE,
};
use serde_json::{json, Value};

fn read_upstream(rel: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/upstream")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn scene(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("scene JSON: {e}\n{text}"))
}

/// The one element upstream's import tests look for.
fn only_text_element(scene: &Value) -> &str {
    let elements = scene["elements"].as_array().expect("elements");
    assert_eq!(elements.len(), 1, "{elements:?}");
    assert_eq!(elements[0]["type"], "text");
    elements[0]["text"].as_str().expect("text")
}

/// `<!-- payload-start -->BASE64<!-- payload-end -->` payloads, as the
/// upstream regex captures them.
fn embedded_base64(svg: &str) -> &str {
    let start = svg.find("<!-- payload-start -->").unwrap() + "<!-- payload-start -->".len();
    let end = svg.find("<!-- payload-end -->").unwrap();
    svg[start..end].trim()
}

/// A metadata payload the way the writer lays it out, for any base64 and
/// version comment.
fn metadata(version: Option<&str>, base64: &str) -> String {
    let version = version
        .map(|v| format!("<!-- payload-version:{v} -->"))
        .unwrap_or_default();
    format!(
        "<!-- payload-type:application/vnd.excalidraw+json -->{version}<!-- payload-start -->{base64}<!-- payload-end -->"
    )
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Upstream fixtures: "import embedded svg (legacy v1)" and "(v2)".
// ---------------------------------------------------------------------------

#[test]
fn legacy_v1_fixture_decodes_to_its_raw_scene_json() {
    let svg = read_upstream("packages/excalidraw/tests/fixtures/test_embedded_v1.svg");
    // No payload-version comment: v1, the base64 of UTF-8 scene JSON.
    assert!(!svg.contains("payload-version"));
    let json = decode_svg_base64_payload(&svg).expect("v1 fixture decodes");
    // Legacy un-encoded scene JSON is returned as is.
    let raw = base64::engine::general_purpose::STANDARD
        .decode(embedded_base64(&svg))
        .unwrap();
    assert_eq!(json.as_bytes(), &raw[..]);
    let scene = scene(&json);
    assert_eq!(scene["type"], "excalidraw");
    assert_eq!(only_text_element(&scene), "test");
}

#[test]
fn v2_fixture_decodes_to_the_compressed_scene() {
    let svg = read_upstream("packages/excalidraw/tests/fixtures/smiley_embedded_v2.svg");
    assert!(svg.contains("<!-- payload-version:2 -->"));
    let json = decode_svg_base64_payload(&svg).expect("v2 fixture decodes");
    assert!(json.starts_with("{\n  \"type\": \"excalidraw\""), "{json}");
    let scene = scene(&json);
    assert_eq!(only_text_element(&scene), "😀");
}

// ---------------------------------------------------------------------------
// Upstream snapshots: the writer's output is upstream's, byte for byte.
// ---------------------------------------------------------------------------

/// The `<metadata>` content of an upstream snapshot: from the payload-type
/// comment through the payload-end comment.
fn snapshot_payload(snap: &str, name: &str) -> String {
    let at = snap
        .find(&format!("exports[`{name}`]"))
        .unwrap_or_else(|| panic!("snapshot {name}"));
    let snap = &snap[at..];
    let start = snap.find("<!-- payload-type:").unwrap();
    let end = snap.find("<!-- payload-end -->").unwrap() + "<!-- payload-end -->".len();
    snap[start..end].to_owned()
}

fn assert_writer_reproduces(snapshot_file: &str, name: &str, element_text: &str) {
    let snap = read_upstream(snapshot_file);
    let expected = snapshot_payload(&snap, name);
    // The metadata element sits right after the source comment.
    assert!(snap.contains(&format!(
        "<!-- svg-source:excalidraw --><metadata>{expected}</metadata>"
    )));

    let payload = decode_svg_base64_payload(&expected).expect("snapshot decodes");
    let scene = scene(&payload);
    assert_eq!(scene["type"], "excalidraw");
    assert_eq!(scene["source"], "https://excalidraw.com");
    let texts: Vec<&str> = scene["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["text"].as_str())
        .collect();
    assert!(texts.contains(&element_text), "{texts:?}");

    // encodeSvgBase64Payload(serializeAsJSON(...)) for that very scene.
    let written = encode_svg_base64_payload(&payload);
    assert_eq!(written, expected);
    assert_eq!(decode_svg_base64_payload(&written).unwrap(), payload);
}

#[test]
fn writer_matches_export_test_tsx_snapshot() {
    assert_writer_reproduces(
        "packages/excalidraw/tests/__snapshots__/export.test.tsx.snap",
        "export > export svg-embedded scene > svg-embdedded scene export output 1",
        "😀",
    );
}

#[test]
fn writer_matches_scene_export_test_snapshot() {
    let snap = read_upstream("packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap");
    let expected = snapshot_payload(&snap, "exportToSvg > with exportEmbedScene 1");
    let payload = decode_svg_base64_payload(&expected).expect("snapshot decodes");
    assert_eq!(scene(&payload)["type"], "excalidraw");
    assert_eq!(encode_svg_base64_payload(&payload), expected);
    assert!(snap.contains(&format!(
        "\"<!-- svg-source:excalidraw --><metadata>{expected}</metadata>"
    )));
}

#[test]
fn writer_emits_the_comments_exactly() {
    let written = encode_svg_base64_payload("{}");
    let data = encode("{}", true);
    let base64 = b64(&excali_core::encode::byte_string_to_bytes(&data.to_json()));
    assert_eq!(
        written,
        format!(
            "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->{base64}<!-- payload-end -->"
        )
    );
    assert_eq!(PAYLOAD_MIME_TYPE, "application/vnd.excalidraw+json");
}

/// "test encoding/decoding scene for SVG export": a scene with a text
/// element "😀" survives the metadata round trip.
#[test]
fn encoding_and_decoding_a_scene_round_trips() {
    let scene_json = excali_core::json::to_string_pretty(&json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://excalidraw.com",
        "elements": [{ "id": "A", "type": "text", "text": "😀", "width": 16, "height": 16 }],
        "appState": { "gridSize": 20, "viewBackgroundColor": "#ffffff" },
        "files": {},
    }));
    let metadata = encode_svg_base64_payload(&scene_json);
    let decoded = decode_svg_base64_payload(&metadata).unwrap();
    assert_eq!(decoded, scene_json);
    assert_eq!(only_text_element(&scene(&decoded)), "😀");
    // Inside a whole SVG document too.
    let svg = format!(
        "<svg version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\"><!-- svg-source:excalidraw --><metadata>{metadata}</metadata><defs></defs></svg>"
    );
    assert_eq!(decode_svg_base64_payload(&svg).unwrap(), scene_json);
}

#[test]
fn writer_handles_every_code_point_class() {
    for text in ["", "a", "é中😀\u{0}\u{7f}\u{80}\u{ff}\u{fffd}", "\"\\\n\t</metadata>&<!-- -->"] {
        let written = encode_svg_base64_payload(text);
        // Base64 needs no escaping inside a text node.
        let b = embedded_base64(&written);
        assert!(b.bytes().all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c)), "{b}");
        assert_eq!(decode_svg_base64_payload(&written).unwrap(), text);
    }
}

// ---------------------------------------------------------------------------
// decodeSvgBase64Payload, case by case.
// ---------------------------------------------------------------------------

#[test]
fn without_the_payload_type_it_is_invalid() {
    assert_eq!(decode_svg_base64_payload(""), Err(SvgPayloadError::Invalid));
    assert_eq!(
        decode_svg_base64_payload("<svg><!-- payload-start -->e30=<!-- payload-end --></svg>"),
        Err(SvgPayloadError::Invalid)
    );
    // Only the text `payload-type:<mime>` is looked for, not the comment.
    let raw = b64(br#"{"type":"excalidraw"}"#);
    assert_eq!(
        decode_svg_base64_payload(&format!(
            "payload-type:application/vnd.excalidraw+json<!-- payload-start -->{raw}<!-- payload-end -->"
        ))
        .unwrap(),
        r#"{"type":"excalidraw"}"#
    );
    // Another type is not a scene.
    assert_eq!(
        decode_svg_base64_payload(
            "<!-- payload-type:application/json --><!-- payload-start -->e30=<!-- payload-end -->"
        ),
        Err(SvgPayloadError::Invalid)
    );
    assert_eq!(SvgPayloadError::Invalid.to_string(), "INVALID");
}

#[test]
fn without_start_and_end_comments_it_is_invalid() {
    for svg in [
        "<!-- payload-type:application/vnd.excalidraw+json -->",
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-start -->e30=",
        "<!-- payload-type:application/vnd.excalidraw+json -->e30=<!-- payload-end -->",
        // `.+?` needs at least one character.
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-start --><!-- payload-end -->",
        // `.` does not match a line terminator.
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-start -->eyJ0\neXBl<!-- payload-end -->",
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-start -->eyJ0\u{2028}eXBl<!-- payload-end -->",
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-end -->e30=<!-- payload-start -->",
    ] {
        assert_eq!(decode_svg_base64_payload(svg), Err(SvgPayloadError::Invalid), "{svg:?}");
    }
}

#[test]
fn the_regex_trims_whitespace_and_takes_the_first_payload() {
    let raw = br#"{"type":"excalidraw","n":1}"#;
    let other = br#"{"type":"excalidraw","n":2}"#;
    // `\s*` on both sides, including newlines and Unicode spaces.
    let svg = metadata(None, &format!("\n\t \u{a0}\u{feff}{}\u{3000}\r\n  ", b64(raw)));
    assert_eq!(decode_svg_base64_payload(&svg).unwrap().as_bytes(), raw);
    // The first start/end pair wins.
    let svg = format!("{}{}", metadata(None, &b64(raw)), metadata(None, &b64(other)));
    assert_eq!(decode_svg_base64_payload(&svg).unwrap().as_bytes(), raw);
    // A start whose payload cannot match (line break) gives way to a later one.
    let svg = format!(
        "<!-- payload-start -->ab\ncd{}",
        metadata(None, &b64(raw))
    );
    assert_eq!(decode_svg_base64_payload(&svg).unwrap().as_bytes(), raw);
    // Lazy `.+?`: the capture stops at the first end comment.
    let svg = format!(
        "{}{}<!-- payload-end -->",
        metadata(None, &b64(raw)),
        b64(other)
    );
    assert_eq!(decode_svg_base64_payload(&svg).unwrap().as_bytes(), raw);
    // With only whitespace between the comments, `\s*` takes it and the
    // capture (at least one character) runs on to the next end comment.
    let svg = format!(
        "<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-start --> <!-- payload-end -->{}<!-- payload-end -->",
        b64(raw)
    );
    // Capture: "<!-- payload-end -->" + base64, which atob rejects.
    assert!(matches!(
        decode_svg_base64_payload(&svg),
        Err(SvgPayloadError::Failed(_))
    ));
}

#[test]
fn the_version_selects_utf8_or_byte_string() {
    let text = "{\"type\":\"excalidraw\",\"t\":\"é😀\"}";
    let utf8 = b64(text.as_bytes());
    // v1 (absent, or "1"): UTF-8.
    assert_eq!(decode_svg_base64_payload(&metadata(None, &utf8)).unwrap(), text);
    assert_eq!(decode_svg_base64_payload(&metadata(Some("1"), &utf8)).unwrap(), text);
    // Any other version: a byte string, so UTF-8 bytes come back one char
    // each (mojibake), exactly as upstream's atob gives them to JSON.parse.
    let mojibake = to_byte_string(text.as_bytes());
    for v in ["2", "01", "3", "10"] {
        assert_eq!(
            decode_svg_base64_payload(&metadata(Some(v), &utf8)).unwrap(),
            mojibake,
            "version {v}"
        );
    }
    // `(\d+)`: a version comment that does not match reads as v1.
    for bad in ["", "x", "2 ", " 2", "-1"] {
        let svg = metadata(None, &utf8).replace(
            "<!-- payload-start -->",
            &format!("<!-- payload-version:{bad} --><!-- payload-start -->"),
        );
        assert_eq!(decode_svg_base64_payload(&svg).unwrap(), text, "{bad:?}");
    }
    // The first matching version comment anywhere in the document counts.
    let svg = format!("<!-- payload-version:x --><!-- payload-version:1 -->{}", metadata(Some("2"), &utf8));
    assert_eq!(decode_svg_base64_payload(&svg).unwrap(), text);
}

#[test]
fn v1_payload_bytes_are_decoded_like_text_decoder() {
    // A leading BOM is dropped and invalid UTF-8 becomes U+FFFD.
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(br#"{"type":"excalidraw","t":""#);
    bytes.push(0xff);
    bytes.extend_from_slice(br#""}"#);
    assert_eq!(
        decode_svg_base64_payload(&metadata(None, &b64(&bytes))).unwrap(),
        "{\"type\":\"excalidraw\",\"t\":\"\u{fffd}\"}"
    );
}

#[test]
fn encoded_wrappers_are_decoded() {
    let text = "{\"type\":\"excalidraw\",\"elements\":[]}";
    for compress in [true, false] {
        let wrapper = encode(text, compress).to_json();
        let bstring = b64(&excali_core::encode::byte_string_to_bytes(&wrapper));
        assert_eq!(
            decode_svg_base64_payload(&metadata(Some("2"), &bstring)).unwrap(),
            text
        );
    }
    // The wrapper's text need not be a scene: upstream returns whatever
    // decode gives.
    let wrapper = encode("not json", false).to_json();
    assert_eq!(
        decode_svg_base64_payload(&metadata(None, &b64(wrapper.as_bytes()))).unwrap(),
        "not json"
    );
    // A v1 payload may hold a wrapper too (it is ASCII when uncompressed).
    let wrapper = encode(text, false).to_json();
    assert_eq!(
        decode_svg_base64_payload(&metadata(None, &b64(wrapper.as_bytes()))).unwrap(),
        text
    );
    // Lone surrogate escapes in `encoded` keep their low byte, as
    // byteStringToArrayBuffer stores UTF-16 code units in a Uint8Array.
    let json = r#"{"encoding":"bstring","compressed":false,"encoded":"a\ud841b"}"#;
    assert_eq!(
        decode_svg_base64_payload(&metadata(Some("2"), &b64(json.as_bytes()))).unwrap(),
        "aAb"
    );
}

#[test]
fn anything_else_fails() {
    let failed = |json: &str, version: Option<&str>| {
        let r = decode_svg_base64_payload(&metadata(version, &b64(json.as_bytes())));
        assert!(matches!(r, Err(SvgPayloadError::Failed(_))), "{json}: {r:?}");
        assert_eq!(r.unwrap_err().to_string(), "FAILED");
    };
    // Not JSON.
    failed("", None);
    failed("{", Some("2"));
    failed("excalidraw", None);
    // `in` on a primitive throws; objects without `encoded` must be scenes.
    for json in [
        "null",
        "42",
        "true",
        "\"encoded\"",
        "[]",
        "{}",
        r#"{"type":"excalidrawlib"}"#,
        r#"{"type":"Excalidraw"}"#,
        r#"{"type":null}"#,
    ] {
        failed(json, None);
        failed(json, Some("2"));
    }
    // A wrapper decode rejects.
    failed(r#"{"encoding":"base64","compressed":false,"encoded":""}"#, None);
    failed(r#"{"compressed":false,"encoded":""}"#, None);
    failed(r#"{"encoding":"bstring","compressed":true,"encoded":"xyz"}"#, None);
    // Not base64 at all.
    for base64 in ["e30", "e30=e30=", "e3!0", "é30=", "e30==="] {
        let r = decode_svg_base64_payload(&metadata(Some("2"), base64));
        assert!(matches!(r, Err(SvgPayloadError::Failed(_))), "{base64}: {r:?}");
    }
}

#[test]
fn truncated_compressed_data_is_no_scene() {
    // pako returns undefined for input that ends early, so upstream's
    // decode (and decodeSvgBase64Payload) returns undefined too.
    let z = deflate(b"{\"type\":\"excalidraw\"}");
    let wrapper = json!({
        "version": "1",
        "encoding": "bstring",
        "compressed": true,
        "encoded": to_byte_string(&z[..z.len() - 6]),
    })
    .to_string();
    let base64 = b64(&excali_core::encode::byte_string_to_bytes(&wrapper));
    assert_eq!(
        decode_svg_base64_payload(&metadata(Some("2"), &base64)),
        Err(SvgPayloadError::Incomplete)
    );
}

// ---------------------------------------------------------------------------
// btoa / atob and encode.ts's base64 helpers.
// ---------------------------------------------------------------------------

#[test]
fn btoa_encodes_byte_strings() {
    assert_eq!(btoa("").unwrap(), "");
    assert_eq!(btoa("f").unwrap(), "Zg==");
    assert_eq!(btoa("fo").unwrap(), "Zm8=");
    assert_eq!(btoa("foo").unwrap(), "Zm9v");
    assert_eq!(btoa("foob").unwrap(), "Zm9vYg==");
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(btoa(&to_byte_string(&all)).unwrap(), b64(&all));
    // A char above U+00FF is an InvalidCharacterError.
    assert_eq!(btoa("\u{100}"), Err(InvalidCharacterError));
    assert_eq!(btoa("😀"), Err(InvalidCharacterError));
}

#[test]
fn atob_is_forgiving_base64_decode() {
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(atob(&b64(&all)).unwrap(), to_byte_string(&all));
    assert_eq!(atob("").unwrap(), "");
    assert_eq!(atob("Zg==").unwrap(), "f");
    // Padding is optional; ASCII whitespace anywhere is removed.
    assert_eq!(atob("Zg").unwrap(), "f");
    assert_eq!(atob("Zm8").unwrap(), "fo");
    assert_eq!(atob(" Zm\t9v\nYg\u{c}==\r").unwrap(), "foob");
    // Leftover bits need not be zero.
    assert_eq!(atob("Zh==").unwrap(), "f");
    for bad in ["Z", "Zg=", "Zg===", "Z===", "=Zg=", "Zm=9", "Zm9v!", "Zm9\u{b}v", "Zm9\u{a0}v", "Zm-_"] {
        assert_eq!(atob(bad), Err(InvalidCharacterError), "{bad:?}");
    }
}

#[test]
fn string_to_base64_and_back_follow_encode_ts() {
    // Not a byte string: UTF-8 first.
    assert_eq!(string_to_base64("é😀", false).unwrap(), b64("é😀".as_bytes()));
    assert_eq!(base64_to_string(&b64("é😀".as_bytes()), false).unwrap(), "é😀");
    // A byte string: as is.
    assert_eq!(string_to_base64("\u{e9}", true).unwrap(), "6Q==");
    assert_eq!(base64_to_string("6Q==", true).unwrap(), "\u{e9}");
    assert_eq!(string_to_base64("😀", true), Err(InvalidCharacterError));
    assert_eq!(base64_to_string("6", true), Err(InvalidCharacterError));
}
