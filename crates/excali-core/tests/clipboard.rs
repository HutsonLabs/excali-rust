//! The clipboard JSON codec against upstream's own `serializeAsClipboardJSON`
//! and `parseClipboard` (`packages/excalidraw/clipboard.ts:143-193,
//! 523-555`), recorded by `tools/goldens/clipboard-fixtures.mjs` in
//! `tests/fixtures/clipboard.json`.

use excali_core::clipboard::{
    clipboard_items, parse_clipboard, parse_clipboard_text, serialize_as_clipboard_json,
    ClipboardData, MIME_TYPE_EXCALIDRAW_CLIPBOARD, MIME_TYPE_TEXT,
};
use excali_core::constants::{
    EXPORT_DATA_TYPE_EXCALIDRAW, EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD,
    EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD_WITH_API,
};
use excali_core::document::Document;
use excali_core::element::{Element, ElementBase, ElementKind};
use excali_core::fractional_index::ChangeStamp;
use serde_json::{json, Map, Value};

const FIXTURE: &str = include_str!("fixtures/clipboard.json");

/// Upstream's test mode: `randomInteger()` from roughjs's `Random` after
/// `reseed(seed)` (`packages/common/src/random.ts:8-15`,
/// `roughjs/bin/math.js`), `getUpdatedTimestamp()` 1.
struct UpstreamTestStamp {
    seed: i32,
}

impl UpstreamTestStamp {
    fn new(seed: i32) -> Self {
        UpstreamTestStamp { seed }
    }

    /// `Random.next()`: Park-Miller with `Math.imul`.
    fn next(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(48271);
        f64::from(self.seed & i32::MAX) / 2f64.powi(31)
    }
}

impl ChangeStamp for UpstreamTestStamp {
    fn version_nonce(&mut self) -> f64 {
        (self.next() * 2f64.powi(31)).floor()
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("clipboard.json parses")
}

fn cases(section: &str) -> Vec<Value> {
    fixture()[section].as_array().expect("case list").clone()
}

/// The case's element array, read through the typed codec so unknown keys,
/// key order and lone surrogates are kept as upstream kept them.
fn elements_of(case: &Value) -> Vec<Element> {
    let text = case["elements"].as_str().expect("elements text");
    let doc = Document::from_json(&format!("{{\"type\":\"excalidraw\",\"elements\":{text}}}"))
        .unwrap_or_else(|e| panic!("{}: {e}", case["id"]));
    doc.elements.expect("elements")
}

#[test]
fn serialize_matches_upstream_byte_for_byte() {
    let cases = cases("serialize");
    assert!(cases.len() >= 20);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let elements = elements_of(&case);
        let files = case["files"].as_object();
        let seed = case["seed"].as_i64().unwrap() as i32;
        let mut stamp = UpstreamTestStamp::new(seed);
        let output = serialize_as_clipboard_json(&elements, files, &mut stamp);
        assert_eq!(output, case["output"].as_str().unwrap(), "{id}");
    }
}

#[test]
fn serialize_does_not_touch_the_input() {
    let case = cases("serialize")
        .into_iter()
        .find(|c| c["id"] == "frameId-to-copied-non-frame-cleared")
        .unwrap();
    let elements = elements_of(&case);
    let before = elements.clone();
    let mut stamp = UpstreamTestStamp::new(1);
    let _ = serialize_as_clipboard_json(&elements, None, &mut stamp);
    assert_eq!(elements, before);
    assert_eq!(elements[1].base.frame_id.as_deref(), Some("rect"));
}

fn base(id: &str, frame_id: Option<&str>) -> ElementBase {
    let mut b = ElementBase::new(id, 0.0, 0.0, 1.0, 5.0);
    b.frame_id = frame_id.map(str::to_owned);
    b
}

#[test]
fn orphaned_child_frame_id_is_cleared_and_version_bumped() {
    // A child whose frameId resolves among the copied elements to an
    // element that is not a frame is orphaned: its frameId is cleared on
    // the copy through mutateElement (version + 1, fresh nonce, updated).
    let elements = vec![
        Element::new(base("r", None), ElementKind::Rectangle),
        Element::new(base("e", Some("r")), ElementKind::Ellipse),
        Element::new(
            base("f", None),
            ElementKind::Frame(excali_core::element::FrameFields { name: None }),
        ),
        Element::new(base("d", Some("f")), ElementKind::Diamond),
    ];
    let mut stamp = UpstreamTestStamp::new(7);
    let out: Value =
        serde_json::from_str(&serialize_as_clipboard_json(&elements, None, &mut stamp)).unwrap();
    assert_eq!(out["type"], EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD);
    assert!(out.get("files").is_none());
    let e = &out["elements"][1];
    assert_eq!(e["frameId"], Value::Null);
    assert_eq!(e["version"], 2);
    assert_eq!(e["updated"], 1);
    let mut expect = UpstreamTestStamp::new(7);
    assert_eq!(e["versionNonce"].as_f64(), Some(expect.version_nonce()));
    // Children of a copied frame keep it.
    assert_eq!(out["elements"][3]["frameId"], "f");
    assert_eq!(out["elements"][3]["version"], 1);
}

#[test]
fn serialize_writes_compact_json() {
    let elements = vec![Element::new(base("r", None), ElementKind::Rectangle)];
    let mut stamp = UpstreamTestStamp::new(1);
    let files = Map::new();
    let out = serialize_as_clipboard_json(&elements, Some(&files), &mut stamp);
    assert!(out.starts_with("{\"type\":\"excalidraw/clipboard\",\"elements\":[{\"id\":\"r\","));
    assert!(out.ends_with("}],\"files\":{}}"));
    assert!(!out.contains('\n'));
}

#[test]
fn copy_writes_the_json_under_both_mime_types() {
    let items = clipboard_items("{}");
    assert_eq!(
        items,
        [
            ("application/vnd.excalidraw.clipboard+json", "{}"),
            ("text/plain", "{}")
        ]
    );
    assert_eq!(MIME_TYPE_EXCALIDRAW_CLIPBOARD, "application/vnd.excalidraw.clipboard+json");
    assert_eq!(MIME_TYPE_TEXT, "text/plain");
}

/// The upstream result object of a parse case, rebuilt from ours: key list
/// and values as the generator records them.
fn recorded(data: &ClipboardData) -> Value {
    match data {
        ClipboardData::Text(text) => json!({ "keys": ["text"], "text": text }),
        ClipboardData::Elements(p) => {
            let mut out = Map::new();
            out.insert(
                "keys".into(),
                json!(["elements", "files", "text", "programmaticAPI"]),
            );
            out.insert("elements".into(), Value::Array(p.elements.clone()));
            if let Some(files) = &p.files {
                out.insert("files".into(), json!({ "value": files }));
            }
            if let Some(text) = &p.text {
                out.insert("text".into(), Value::from(text.as_str()));
            }
            out.insert("programmaticAPI".into(), Value::from(p.programmatic_api));
            Value::Object(out)
        }
    }
}

/// Object equality with key order, as the recorded JSON has it.
fn same_ordered(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((ka, va), (kb, vb))| ka == kb && same_ordered(va, vb))
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same_ordered(a, b))
        }
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

#[test]
fn parse_matches_upstream() {
    let cases = cases("parse");
    assert!(cases.len() >= 60);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let text_plain = case["textPlain"].as_str();
        let plain = case["isPlainPaste"].as_bool().unwrap();
        let data = parse_clipboard(text_plain, plain);
        let ours = recorded(&data);
        let theirs = &case["result"];
        // `keys` first, then the values in the generator's order.
        let mut expected = Map::new();
        for (k, v) in theirs.as_object().unwrap() {
            expected.insert(k.clone(), v.clone());
        }
        assert!(
            same_ordered(&ours, &Value::Object(expected)),
            "{id}:\n ours   {ours}\n theirs {theirs}"
        );
    }
}

#[test]
fn accepts_the_three_export_types() {
    for (ty, api) in [
        (EXPORT_DATA_TYPE_EXCALIDRAW, false),
        (EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD, false),
        (EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD_WITH_API, true),
    ] {
        let text = format!("{{\"type\":\"{ty}\",\"elements\":[{{\"id\":\"a\"}}]}}");
        match parse_clipboard_text(&text, false) {
            ClipboardData::Elements(p) => {
                assert_eq!(p.elements, vec![json!({"id": "a"})], "{ty}");
                assert_eq!(p.programmatic_api, api, "{ty}");
                assert_eq!(p.files, None);
                assert_eq!(p.text, None);
            }
            other => panic!("{ty}: {other:?}"),
        }
    }
    assert_eq!(
        parse_clipboard_text("{\"type\":\"excalidrawlib\",\"elements\":[]}", false),
        ClipboardData::Text("{\"type\":\"excalidrawlib\",\"elements\":[]}".into())
    );
}

#[test]
fn parse_clipboard_text_does_not_trim() {
    // parseClipboard's JSON step takes the event text as it is; only the
    // text/plain read (parse_clipboard) trims.
    assert_eq!(parse_clipboard_text(" x ", false), ClipboardData::Text(" x ".into()));
    assert_eq!(parse_clipboard(Some(" x "), false), ClipboardData::Text("x".into()));
}

#[test]
fn round_trip_through_the_clipboard() {
    let case = cases("serialize")
        .into_iter()
        .find(|c| c["id"] == "every-type-with-files")
        .unwrap();
    let elements = elements_of(&case);
    let mut stamp = UpstreamTestStamp::new(3);
    let json = serialize_as_clipboard_json(&elements, case["files"].as_object(), &mut stamp);
    let ClipboardData::Elements(pasted) = parse_clipboard(Some(&json), false) else {
        panic!("not elements");
    };
    assert_eq!(pasted.elements.len(), elements.len());
    let typed: Vec<Element> = pasted
        .elements
        .iter()
        .map(|v| Element::from_map(v.as_object().unwrap().clone()).unwrap())
        .collect();
    assert_eq!(typed.len(), elements.len());
    assert_eq!(typed[0], elements[0]);
    assert_eq!(pasted.files.as_ref(), Some(&case["files"]));
}
