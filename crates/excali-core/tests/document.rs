//! The typed `.excalidraw` codec: [`Document`] and [`Element`] serde.
//!
//! Upstream writes a scene with `serializeAsJSON`
//! (`packages/excalidraw/data/json.ts:52-75`): the object literal
//! `{type, version, source, elements, appState, files}` passed to
//! `JSON.stringify(data, null, 2)`. It reads with `JSON.parse` and keeps
//! every property it does not know: `restoreElementWithProperties` spreads
//! the original element first (`restore.ts:500-508`), so unknown keys and
//! the element's own key order survive, and `JSON.stringify` writes keys in
//! the object's property order.
//!
//! Fixtures: written by `tools/goldens/scene-fixtures.mjs` (CI runs it
//! with `--check`), which bundles upstream's own TypeScript from the pinned
//! checkout (commit 438d89861f53d8a90ad566113ecac1b83761098f) with esbuild,
//! as `tools/goldens` does, and runs it under node with `Date.now` fixed at
//! 1700000000000, upstream's `reseed(1700000000000)`
//! (`packages/common/src/random.ts`) before each fixture, and `Math.random`
//! disabled. See the generator's header for the exact calls:
//!
//! - `every-type.excalidraw`: one element of every persisted type built by
//!   upstream's own constructors (`newElement`, `newEmbeddableElement`,
//!   `newIframeElement`, `newStickyNoteElement`, `newFrameElement`,
//!   `newMagicFrameElement`, `newTextElement`, `newFreeDrawElement`,
//!   `newLinearElement`, `newArrowElement`, `newImageElement`,
//!   `packages/element/src/newElement.ts:87-692`) with explicit ids and
//!   seeds, saved by `serializeAsJSON(elements, getDefaultAppState(),
//!   files, "local")` with `window.EXCALIDRAW_EXPORT_SOURCE =
//!   "https://excalidraw.com"`. `newTextElement` measures its text with a
//!   canvas; node has none, so a metrics provider measures 10 px per
//!   character (the text's width 50), which only affects `width`;
//! - `unknown-keys.excalidraw`: `JSON.stringify(data, null, 2)` of a scene
//!   with unknown keys at the top level, first, in the middle and last in an
//!   element, and inside `appState`, `files` and nested known objects;
//!   known keys in a non-canonical order; and values the typed model
//!   normalises (`customData: null`, `roundness.value: null`);
//! - `unknown-keys-edited.excalidraw`: that file parsed, then edited with
//!   upstream's `mutateElement` (`packages/element/src/mutateElement.ts`,
//!   which assigns each update onto the element, 80-100): the rectangle's
//!   `x` set to 99 and a `customData` added, the text's `text` changed;
//!   `futureTopLevel` deleted and `appState.futureAppStateFlag` set to
//!   false; then `JSON.stringify(data, null, 2)` again. `mutateElement`
//!   also bumps `version` and draws `versionNonce` from the reseeded
//!   generator.

use excali_core::constants::{EXPORT_DATA_TYPE_EXCALIDRAW, VERSION_EXCALIDRAW};
use excali_core::document::Document;
use excali_core::element::{
    ArrowFields, Arrowhead, Element, ElementBase, ElementKind, FileId, FontFamily, FrameFields,
    FreedrawFields, ImageFields, LineFields, LinearFields, Radians, StickyNoteFields, TextFields,
};
use excali_core::json;
use serde_json::{json, Map, Value};

const EVERY_TYPE: &str = include_str!("fixtures/every-type.excalidraw");
const UNKNOWN_KEYS: &str = include_str!("fixtures/unknown-keys.excalidraw");
const UNKNOWN_KEYS_EDITED: &str = include_str!("fixtures/unknown-keys-edited.excalidraw");
const EMPTY_SCENE: &str = include_str!("fixtures/empty-scene.excalidraw");

const TIMESTAMP: f64 = 1_700_000_000_000.0;

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

fn default_app_state() -> Map<String, Value> {
    object(json!({
        "gridSize": 20,
        "gridStep": 5,
        "gridModeEnabled": false,
        "viewBackgroundColor": "#ffffff",
        "lockedMultiSelections": {}
    }))
}

// ---------------------------------------------------------------------------
// Writing new elements: upstream's constructor key order

/// The elements of `every-type.excalidraw`, built with this crate's
/// constructors and the same options upstream's constructors were given.
fn every_type_elements() -> Vec<Element> {
    let base = |id: &str, seed: f64, x: f64, y: f64| ElementBase::new(id, x, y, seed, TIMESTAMP);
    let sized = |mut b: ElementBase, w: f64, h: f64| {
        b.width = w;
        b.height = h;
        b
    };

    let mut rect = Element::new(
        sized(base("rect", 1.0, 0.0, 0.0), 100.0, 50.0),
        ElementKind::Rectangle,
    );
    rect.base.custom_data = Some(object(json!({"note": "kept"})));

    let mut ellipse_base = sized(base("ellipse", 3.0, 20.0, 20.0), 30.0, 20.0);
    ellipse_base.angle = Radians(0.5);

    let mut embed_base = base("embed", 4.0, 0.0, 100.0);
    embed_base.link = Some("https://example.com".into());

    // newStickyNoteElement: normalizeStickyNoteStyle sets the default
    // sticky background (colors.ts:268) and keeps the stroke. It does so
    // through newElementWith, which bumps `version` and draws
    // `versionNonce` (randomInteger after reseed(1700000000000)).
    let mut sticky_base = sized(base("sticky", 6.0, 300.0, 0.0), 200.0, 200.0);
    sticky_base.background_color = "#ffdf6b".into();
    sticky_base.version = 2.0;
    sticky_base.version_nonce = 428_152_832.0;

    let text_base = sized(base("text", 9.0, 5.0, 5.0), 50.0, 25.0);

    let mut arrow = ArrowFields::new(LinearFields::new(vec![[0.0, 0.0], [20.0, 5.0]]), false);
    arrow.linear.end_arrowhead = Some(Arrowhead::Arrow);

    vec![
        rect,
        Element::new(
            sized(base("diamond", 2.0, 10.0, 10.0), 40.0, 40.0),
            ElementKind::Diamond,
        ),
        Element::new(ellipse_base, ElementKind::Ellipse),
        Element::new(embed_base, ElementKind::Embeddable),
        Element::new(base("iframe", 5.0, 0.0, 200.0), ElementKind::Iframe),
        Element::new(
            sticky_base,
            ElementKind::StickyNote(StickyNoteFields { base_height: 200.0 }),
        ),
        Element::new(
            base("frame", 7.0, -100.0, -100.0),
            ElementKind::Frame(FrameFields {
                name: Some("Frame A".into()),
            }),
        ),
        Element::new(
            base("magic", 8.0, -200.0, -200.0),
            ElementKind::MagicFrame(FrameFields { name: None }),
        ),
        Element::new(
            text_base,
            ElementKind::Text(TextFields::new("Hello", FontFamily::EXCALIFONT, 1.25)),
        ),
        Element::new(
            base("draw", 10.0, 1.0, 2.0),
            ElementKind::Freedraw(FreedrawFields::new(vec![[0.0, 0.0], [1.5, 2.25]], true)),
        ),
        Element::new(
            base("line", 11.0, 3.0, 4.0),
            ElementKind::Line(LineFields {
                linear: LinearFields::new(vec![[0.0, 0.0], [10.0, 0.0]]),
                polygon: false,
            }),
        ),
        Element::new(base("arrow", 12.0, 5.0, 6.0), ElementKind::Arrow(arrow)),
        Element::new(
            base("elbow", 13.0, 7.0, 8.0),
            ElementKind::Arrow(ArrowFields::new(
                LinearFields::new(vec![[0.0, 0.0], [0.0, 30.0], [40.0, 30.0]]),
                true,
            )),
        ),
        Element::new_image(
            sized(base("image", 14.0, 9.0, 10.0), 64.0, 64.0),
            ImageFields {
                file_id: Some(FileId("file1".into())),
                ..ImageFields::default()
            },
        ),
    ]
}

#[test]
fn new_elements_are_written_in_upstream_constructor_order() {
    let files = object(json!({
        "file1": {
            "mimeType": "image/png",
            "id": "file1",
            "dataURL": "data:image/png;base64,iVBORw0KGgo=",
            "created": 1700000000000u64,
            "lastRetrieved": 1700000000000u64
        }
    }));
    let doc = Document::new(
        "https://excalidraw.com",
        every_type_elements(),
        default_app_state(),
        Some(files),
    );
    assert_eq!(doc.to_json(), EVERY_TYPE);
}

#[test]
fn new_document_has_upstream_header_values() {
    let doc = Document::new("https://example.com", Vec::new(), Map::new(), None);
    assert_eq!(EXPORT_DATA_TYPE_EXCALIDRAW, "excalidraw");
    assert_eq!(VERSION_EXCALIDRAW, 2.0);
    assert_eq!(doc.version, Some(2.0));
    assert_eq!(doc.source.as_deref(), Some("https://example.com"));
    assert_eq!(doc.elements.as_ref().map(Vec::len), Some(0));
    assert!(doc.extra.is_empty());
}

#[test]
fn files_none_drops_the_key_like_a_database_save() {
    // serializeAsJSON(..., "database") passes files: undefined, which
    // JSON.stringify omits (json.ts:67-71). Output of upstream for
    // ([], getDefaultAppState(), {}, "database"):
    let expected = "{\n  \"type\": \"excalidraw\",\n  \"version\": 2,\n  \"source\": \"https://excalidraw.com\",\n  \"elements\": [],\n  \"appState\": {\n    \"gridSize\": 20,\n    \"gridStep\": 5,\n    \"gridModeEnabled\": false,\n    \"viewBackgroundColor\": \"#ffffff\",\n    \"lockedMultiSelections\": {}\n  }\n}";
    let doc = Document::new(
        "https://excalidraw.com",
        Vec::new(),
        default_app_state(),
        None,
    );
    assert_eq!(doc.to_json(), expected);
}

// ---------------------------------------------------------------------------
// Reading and writing back

#[test]
fn fixtures_are_what_json_stringify_writes() {
    for text in [EVERY_TYPE, UNKNOWN_KEYS, UNKNOWN_KEYS_EDITED] {
        assert_eq!(json::round_trip(text).unwrap(), text);
    }
}

#[test]
fn every_fixture_round_trips_byte_for_byte() {
    for text in [
        EVERY_TYPE,
        UNKNOWN_KEYS,
        UNKNOWN_KEYS_EDITED,
        EMPTY_SCENE.trim_end(),
    ] {
        let doc = Document::from_json(text).expect("fixture parses");
        assert_eq!(doc.to_json(), text);
    }
}

#[test]
fn parsed_new_elements_equal_the_constructed_ones() {
    let doc = Document::from_json(EVERY_TYPE).unwrap();
    assert_eq!(doc.elements.unwrap(), every_type_elements());
}

#[test]
fn unknown_top_level_keys_are_kept_in_extra() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let keys: Vec<&str> = doc.extra.keys().map(String::as_str).collect();
    assert_eq!(keys, ["futureTopLevel", "pluginData"]);
    assert_eq!(doc.extra["pluginData"], json!({"b": 1, "a": 2}));
    assert_eq!(doc.source.as_deref(), Some("https://future.example"));
    let app_state = doc.app_state.unwrap();
    assert_eq!(app_state["futureAppStateFlag"], json!(true));
    let files = doc.files.unwrap();
    assert_eq!(files["f1"]["futureFileKey"], json!("x"));
}

#[test]
fn unknown_element_keys_are_kept_in_extra() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let elements = doc.elements.unwrap();
    let rect = &elements[0];
    assert_eq!(rect.element_type().as_str(), "rectangle");
    let keys: Vec<&str> = rect.extra.keys().map(String::as_str).collect();
    assert_eq!(keys, ["futureFirst", "futureMiddle", "futureLast"]);
    assert_eq!(rect.extra["futureLast"], json!([true, null, 1.5e-7]));
    assert_eq!(rect.base.index.as_ref().map(|i| i.0.as_str()), Some("a0"));

    // A key another type owns is unknown on this one.
    let diamond = &elements[3];
    assert_eq!(diamond.extra["labelPosition"], json!(0.5));
}

#[test]
fn edits_keep_unknown_keys_and_append_new_keys_like_js() {
    let mut doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let elements = doc.elements.as_mut().unwrap();

    let rect = &mut elements[0];
    rect.base.x = 99.0;
    rect.base.custom_data = Some(object(json!({"added": true})));
    rect.base.version = 4.0;
    rect.base.version_nonce = 428_152_832.0;

    let text = &mut elements[1];
    let ElementKind::Text(fields) = &mut text.kind else {
        panic!("second element is text");
    };
    fields.text = "edited".into();
    text.base.version = 4.0;
    text.base.version_nonce = 2_130_208_768.0;

    doc.extra.shift_remove("futureTopLevel");
    doc.app_state
        .as_mut()
        .unwrap()
        .insert("futureAppStateFlag".into(), json!(false));

    assert_eq!(doc.to_json(), UNKNOWN_KEYS_EDITED);
}

#[test]
fn values_the_model_normalises_are_written_back_as_read_while_unchanged() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let elements = doc.elements.unwrap();
    // customData: null reads as no customData, roundness.value null as no
    // value, and the unknown key inside a bound element is not modelled;
    // unchanged, all are written back exactly as read.
    let written = json::to_string_pretty(&serde_json::to_value(&elements).unwrap());
    let read = json::round_trip(
        &serde_json::to_string(&serde_json::from_str::<Value>(UNKNOWN_KEYS).unwrap()["elements"])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(written, read);
}

#[test]
fn a_changed_normalised_value_is_written_from_the_model() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let mut elements = doc.elements.unwrap();
    let diamond = &mut elements[3];
    let bound = diamond.base.bound_elements.as_mut().unwrap();
    assert_eq!(bound.len(), 1);
    bound[0].id = "other".into();
    let value = serde_json::to_value(&*diamond).unwrap();
    // The model writes BoundElement as {id, type}; the unknown nested key
    // goes with the edit, as the typed value replaces the raw one.
    assert_eq!(
        value["boundElements"],
        json!([{"id": "other", "type": "arrow"}])
    );
    // Other keys keep their place.
    assert_eq!(value["labelPosition"], json!(0.5));
}

#[test]
fn a_key_removed_from_extra_is_not_written_and_order_holds() {
    let mut doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let rect = &mut doc.elements.as_mut().unwrap()[0];
    rect.extra.shift_remove("futureMiddle");
    let value = serde_json::to_value(&*rect).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(&keys[..3], ["futureFirst", "id", "type"]);
    assert_eq!(keys[7], "angle");
    assert_eq!(keys[8], "strokeColor");
    assert_eq!(keys.last(), Some(&"futureLast"));
}

#[test]
fn extra_keys_on_a_new_element_follow_the_known_keys() {
    let mut e = Element::new(
        ElementBase::new("n", 0.0, 0.0, 1.0, TIMESTAMP),
        ElementKind::Ellipse,
    );
    e.extra.insert("z".into(), json!(1));
    e.extra.insert("a".into(), json!(2));
    let value = serde_json::to_value(&e).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.len(), 29);
    assert_eq!(&keys[..2], ["id", "type"]);
    assert_eq!(&keys[26..], ["locked", "z", "a"]);
}

#[test]
fn an_extra_key_that_the_model_owns_does_not_override_it() {
    let mut e = Element::new(
        ElementBase::new("n", 5.0, 0.0, 1.0, TIMESTAMP),
        ElementKind::Rectangle,
    );
    e.extra.insert("x".into(), json!("not a number"));
    let value = serde_json::to_value(&e).unwrap();
    assert_eq!(value["x"], json!(5.0));
}

#[test]
fn element_serde_round_trips_through_values() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    for element in doc.elements.unwrap() {
        let value = serde_json::to_value(&element).unwrap();
        let back: Element = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back, element);
        assert_eq!(serde_json::to_value(&back).unwrap(), value);
    }
}

#[test]
fn key_order_does_not_affect_equality() {
    let doc = Document::from_json(UNKNOWN_KEYS).unwrap();
    let ellipse = doc.elements.unwrap()[2].clone();
    // Read in reverse key order; built fresh it would be written in
    // constructor order. Same content, so equal.
    let mut rebuilt = Element::new(ellipse.base.clone(), ellipse.kind.clone());
    rebuilt.extra = ellipse.extra.clone();
    assert_eq!(rebuilt, ellipse);
    assert_ne!(
        serde_json::to_string(&rebuilt).unwrap(),
        serde_json::to_string(&ellipse).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Absent and null keys (ImportedDataState, data/types.ts:35-50)

#[test]
fn absent_top_level_keys_stay_absent() {
    let text = "{\n  \"type\": \"excalidraw\"\n}";
    let doc = Document::from_json(text).unwrap();
    assert_eq!(doc.version, None);
    assert_eq!(doc.source, None);
    assert!(doc.elements.is_none());
    assert!(doc.app_state.is_none());
    assert!(doc.files.is_none());
    assert_eq!(doc.to_json(), text);
}

#[test]
fn null_elements_and_app_state_are_written_back_as_null() {
    let text = "{\n  \"type\": \"excalidraw\",\n  \"elements\": null,\n  \"appState\": null\n}";
    let doc = Document::from_json(text).unwrap();
    assert!(doc.elements.is_none());
    assert!(doc.app_state.is_none());
    assert_eq!(doc.to_json(), text);
}

#[test]
fn absent_optional_element_keys_stay_absent() {
    // A text element without labelPosition and without created: upstream
    // writes back only what it read.
    let mut value = serde_json::to_value(Element::new(
        ElementBase::new("t", 0.0, 0.0, 1.0, TIMESTAMP),
        ElementKind::Text(TextFields::new("x", FontFamily::EXCALIFONT, 1.25)),
    ))
    .unwrap();
    let map = value.as_object_mut().unwrap();
    map.shift_remove("labelPosition");
    map.shift_remove("created");
    let element: Element = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(element.base.created, None);
    assert_eq!(serde_json::to_value(&element).unwrap(), value);
}

// ---------------------------------------------------------------------------
// Validation (isValidExcalidrawData, json.ts:115-126)

#[test]
fn type_must_be_excalidraw() {
    assert!(Document::from_json("{\"type\": \"excalidrawlib\"}").is_err());
    assert!(Document::from_json("{\"elements\": []}").is_err());
    assert!(Document::from_json("[]").is_err());
    assert!(Document::from_json("{\"type\": \"excalidraw\", \"elements\": {}}").is_err());
    assert!(Document::from_json("{\"type\": \"excalidraw\", \"appState\": []}").is_err());
    assert!(Document::from_json("{\"type\": ").is_err());
}

#[test]
fn unknown_element_type_is_an_error_for_the_typed_codec() {
    // Restore (ex-103) drops unknown types from untyped input; the typed
    // codec only reads elements it can model.
    let text = "{\"type\": \"excalidraw\", \"elements\": [{\"type\": \"hexagon\"}]}";
    assert!(Document::from_json(text).is_err());
}

// ---------------------------------------------------------------------------
// Strings and numbers

#[test]
fn lone_surrogates_in_element_text_survive() {
    let mut value = serde_json::to_value(Element::new(
        ElementBase::new("t", 0.0, 0.0, 1.0, TIMESTAMP),
        ElementKind::Text(TextFields::new("PLACEHOLDER", FontFamily::EXCALIFONT, 1.25)),
    ))
    .unwrap();
    value["futureNote"] = json!("PLACEHOLDER");
    let doc = json!({"type": "excalidraw", "elements": [value]});
    let text = json::to_string_pretty(&doc).replace("PLACEHOLDER", "a\\ud83db\\ud83d\\ude00");
    let parsed = Document::from_json(&text).unwrap();
    // JSON.stringify keeps the lone high surrogate as an escape and writes
    // the valid pair as the character.
    let written = parsed.to_json();
    assert_eq!(written, json::round_trip(&text).unwrap());
    assert_eq!(written.matches("\"a\\ud83db\u{1F600}\"").count(), 3);
}

#[test]
fn numbers_are_written_as_javascript_does() {
    let mut e = Element::new(
        ElementBase::new("n", 0.1 + 0.2, -0.0, 1.0, TIMESTAMP),
        ElementKind::Rectangle,
    );
    e.base.width = 1e21;
    e.base.height = 1.5e-7;
    let doc = Document::new("s", vec![e], Map::new(), None);
    let text = doc.to_json();
    assert!(text.contains("\"x\": 0.30000000000000004,"));
    assert!(text.contains("\"y\": 0,"));
    assert!(text.contains("\"width\": 1e+21,"));
    assert!(text.contains("\"height\": 1.5e-7,"));
    assert!(text.contains("\"updated\": 1700000000000,"));
}
