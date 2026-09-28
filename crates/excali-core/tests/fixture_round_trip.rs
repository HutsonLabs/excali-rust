//! D1 conformance (ex-g101): every scene-bearing file of upstream's test
//! fixtures (`packages/excalidraw/tests/fixtures` at the pin, copied to
//! `fixtures/upstream/`) parses, is restored, re-serialises and re-parses to
//! an identical element set, and each write is upstream's bytes; unknown
//! fields survive.
//!
//! `tests/fixtures/document-round-trip.json` is upstream's own output,
//! written by `tools/goldens/document-fixtures.mjs` from the pinned checkout
//! and re-checked in CI (the `goldens` job of `.github/workflows/gates.yml`):
//! for each file, the scene text upstream's loader parses (`input`),
//! `serializeAsJSON(..., "local")` of what `loadFromBlob` gives for the file
//! (`output`, `packages/excalidraw/data/blob.ts:137-216`, `json.ts:52-75`),
//! and the same for `output` loaded again (`reload`), in upstream's test
//! mode after `reseed(1)`, which is [`TestEnv::default`]. The library is
//! `loadLibraryFromBlob` then `serializeLibraryAsJSON`.
//!
//! The port does what upstream's load and save do, with the public API:
//! [`Document::from_json`], [`restore_elements`] with `repairBindings` and
//! `deleteInvisibleElements` and no local elements, [`restore_app_state`] of
//! the file's exported `appState` with no local state (`blob.ts:161-179`),
//! then a [`Document`] of the restored elements, the exported `appState`
//! and the files live elements use (`serializeAsJSON`'s
//! `filterOutDeletedFiles`, `json.ts:31-50`), written with
//! [`Document::to_json`].
//!
//! One deliberate difference: upstream's `serializeAsJSON` writes a fixed
//! object, so a top-level key it does not know is dropped on save; the port
//! keeps it (`site/content/architecture/file-format.md`, D1 "unknown fields
//! survive"). Unknown element keys survive in both (`restore.ts:500-508`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use excali_core::app_state::{clean_app_state_for_export, restore_app_state, AppStateEnv};
use excali_core::document::{load_scene_json, Document, LoadSceneError, LoadedScene};
use excali_core::element::Element;
use excali_core::library::{parse_library_json, serialize_library_as_json, LibraryItemStatus};
use excali_core::png::decode_png_metadata;
use excali_core::restore::{restore_elements, RestoreElementsOptions, TestEnv};
use excali_core::svg_payload::decode_svg_base64_payload;
use serde_json::{json, Map, Value};

const GOLDEN: &str = include_str!("fixtures/document-round-trip.json");
const UPSTREAM_FIXTURES: &str = "fixtures/upstream/packages/excalidraw/tests/fixtures";

fn golden() -> &'static Value {
    static G: OnceLock<Value> = OnceLock::new();
    G.get_or_init(|| serde_json::from_str(GOLDEN).expect("document-round-trip.json parses"))
}

fn cases() -> &'static [Value] {
    golden()["cases"].as_array().expect("cases")
}

fn case(id: &str) -> &'static Value {
    cases()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no case {id}"))
}

fn text<'a>(case: &'a Value, key: &str) -> &'a str {
    case[key]
        .as_str()
        .unwrap_or_else(|| panic!("{}: {key}", case["id"]))
}

fn source() -> &'static str {
    golden()["source"].as_str().expect("source")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo(rel: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// -- load and save, as upstream's loadFromBlob and serializeAsJSON ------------------

/// `filterOutDeletedFiles` (`json.ts:31-50`): the files that elements not
/// deleted refer to by `fileId`, in element order.
fn filter_out_deleted_files(elements: &[Map<String, Value>], files: &Map<String, Value>) -> Map<String, Value> {
    let mut next = Map::new();
    for element in elements {
        if element.get("isDeleted") == Some(&Value::Bool(true)) {
            continue;
        }
        let Some(Value::String(file_id)) = element.get("fileId") else {
            continue;
        };
        if file_id.is_empty() {
            continue;
        }
        if let Some(file) = files.get(file_id) {
            next.insert(file_id.clone(), file.clone());
        }
    }
    next
}

/// What upstream writes for `doc` after loading it: `loadFromBlob(file,
/// null, null)` (`blob.ts:161-179`) then `serializeAsJSON(elements,
/// appState, files, "local")` (`json.ts:52-75`). The document's unknown
/// top-level keys are kept (see the module docs).
fn restore_and_save(doc: &Document) -> Document {
    let mut env = TestEnv::default();
    let elements: Vec<Value> = doc
        .elements
        .iter()
        .flatten()
        .map(|e| Value::Object(e.to_map()))
        .collect();
    let restored = restore_elements(
        &elements,
        None,
        RestoreElementsOptions {
            repair_bindings: true,
            delete_invisible_elements: true,
            refresh_dimensions: false,
        },
        &mut env,
    )
    .expect("restoreElements");

    // restoreAppState({theme: undefined, fileHandle: null,
    // ...cleanAppStateForExport(data.appState || {})}, null)
    let mut imported = Map::new();
    imported.insert("fileHandle".into(), Value::Null);
    imported.extend(clean_app_state_for_export(
        doc.app_state.as_ref().unwrap_or(&Map::new()),
    ));
    let app_state = restore_app_state(Some(&imported), None, &APP_ENV).expect("restoreAppState");

    let files = filter_out_deleted_files(&restored, doc.files.as_ref().unwrap_or(&Map::new()));
    let elements = restored
        .into_iter()
        .map(|e| Element::from_map(e).expect("restored element is typed"))
        .collect();
    let mut saved = Document::new(
        source(),
        elements,
        clean_app_state_for_export(app_state.as_map()),
        Some(files),
    );
    saved.extra = doc.extra.clone();
    saved
}

/// `doc` written as upstream writes it, which drops unknown top-level keys.
fn without_extra(doc: &Document) -> String {
    let mut doc = doc.clone();
    doc.extra = Map::new();
    doc.to_json()
}

const APP_ENV: AppStateEnv = AppStateEnv {
    device_pixel_ratio: 1.0,
    test_env: true,
};

/// The port's loader: `loadFromBlob` for scene text, then
/// `serializeAsJSON(..., "local")`.
fn load_and_save(text: &str) -> Result<Document, LoadSceneError> {
    Ok(load_scene_json(text, &mut TestEnv::default(), &APP_ENV)?.to_document(source()))
}

/// The unknown top-level keys of a scene text, in order.
fn top_level_extra(text: &str) -> Map<String, Value> {
    let Value::Object(map) = serde_json::from_str::<Value>(text).unwrap() else {
        panic!("not an object");
    };
    map.into_iter()
        .filter(|(k, _)| !["type", "version", "source", "elements", "appState", "files"].contains(&k.as_str()))
        .collect()
}

/// One scene case: load `input`, restore and write it (upstream's
/// `output`), re-parse, restore and write again (`reload`). A typed input
/// ([`Document::from_json`] reads it; legacy scenes need restore first)
/// gives the same through [`restore_elements`] and [`restore_app_state`]
/// called directly. Returns the first write.
fn check_scene_case(case: &Value) -> Document {
    let id = case["id"].as_str().expect("id");
    let input_text = text(case, "input");
    let extra = top_level_extra(input_text);

    let first = load_and_save(input_text).unwrap_or_else(|e| panic!("{id}: {e}"));
    let written = first.to_json();
    assert_eq!(without_extra(&first), text(case, "output"), "{id}: first write");
    assert_eq!(first.extra, extra, "{id}: unknown top-level keys, first write");
    if extra.is_empty() {
        assert_eq!(written, text(case, "output"), "{id}: first write");
    }
    if let Ok(input) = Document::from_json(input_text) {
        let direct = restore_and_save(&input);
        assert_eq!(direct.to_json(), written, "{id}: Document, restore_elements, restore_app_state");
        let loaded = LoadedScene::from_document(&input, &mut TestEnv::default(), &APP_ENV).unwrap();
        assert_eq!(loaded.to_document(source()).to_json(), written, "{id}: LoadedScene::from_document");
    }

    let reparsed = Document::from_json(&written).unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_eq!(reparsed.elements, first.elements, "{id}: re-parsed element set");
    assert_eq!(reparsed, first, "{id}: re-parsed document");

    let second = restore_and_save(&reparsed);
    assert_eq!(without_extra(&second), text(case, "reload"), "{id}: second write");
    assert_eq!(second.extra, extra, "{id}: unknown top-level keys, second write");
    assert_eq!(
        load_and_save(&written).unwrap().to_json(),
        second.to_json(),
        "{id}: loader, second write"
    );
    let again = Document::from_json(&second.to_json()).unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_eq!(again.elements, reparsed.elements, "{id}: element set after the second write");
    first
}

/// The library case: `loadLibraryFromBlob` then `serializeLibraryAsJSON`,
/// twice.
fn check_library_case(case: &Value) {
    let id = case["id"].as_str().expect("id");
    let parse = |text: &str| {
        parse_library_json(text, LibraryItemStatus::Unpublished, &mut TestEnv::default())
            .unwrap_or_else(|e| panic!("{id}: {e}"))
    };
    let items = parse(text(case, "input"));
    let written = serialize_library_as_json(&items, source());
    assert_eq!(written, text(case, "output"), "{id}: first write");
    let again = parse(&written);
    assert_eq!(again, items, "{id}: re-parsed items");
    assert_eq!(serialize_library_as_json(&again, source()), text(case, "reload"), "{id}: second write");
}

// -- the fixture ------------------------------------------------------------------

#[test]
fn golden_comes_from_the_pinned_upstream() {
    let pin = String::from_utf8(read_repo("site/config.toml")).unwrap();
    let commit = golden()["upstream"].as_str().unwrap();
    assert!(pin.contains(&format!("upstream_commit = \"{commit}\"")), "golden from {commit}, not the pin");
    assert_eq!(source(), "https://excalidraw.com");
}

// -- diagramFixture -----------------------------------------------------------------

/// `tests/fixtures/diagramFixture.ts`: diamondFixture, ellipseFixture and
/// rectangleFixture (one shared id, `index: null`) with
/// `{viewBackgroundColor, gridModeEnabled}` and no files.
#[test]
fn diagram_fixture_round_trips_to_upstreams_bytes() {
    let c = case("diagramFixture");
    let first = check_scene_case(c);
    let elements = first.elements.as_ref().unwrap();
    // Duplicate ids get randomId() (id0, id1 in test mode), and
    // syncInvalidIndices gives a0..a2, bumping each version.
    let ids: Vec<&str> = elements.iter().map(|e| e.base.id.as_str()).collect();
    assert_eq!(ids, ["vWrqOAfkind2qcm7LDAGZ", "id0", "id1"]);
    let types: Vec<Value> = elements.iter().map(|e| e.to_map()["type"].clone()).collect();
    assert_eq!(types, [json!("diamond"), json!("ellipse"), json!("rectangle")]);
    let app_state = first.app_state.as_ref().unwrap();
    assert_eq!(app_state["viewBackgroundColor"], "#ffffff");
    assert_eq!(app_state["gridModeEnabled"], false);
    // The reload is a fixed point.
    assert_eq!(text(c, "output"), text(c, "reload"));
}

/// The same document with an unknown key on its first element and one at
/// the top level: both survive both writes. Upstream writes the element
/// key and drops the top-level one.
#[test]
fn unknown_element_and_top_level_keys_survive_both_writes() {
    let c = case("diagramFixture-unknown-keys");
    let input = Document::from_json(text(c, "input")).unwrap();
    let top = json!({"note": "kept by excali-rust", "list": [1, "two", null]});
    assert_eq!(input.extra.get("futureTopLevel"), Some(&top));
    let nested = json!({"nested": [1, 2, {"z": 1, "a": 2}]});
    assert_eq!(
        input.elements.as_ref().unwrap()[0].extra.get("futureElementKey"),
        Some(&nested)
    );

    let first = check_scene_case(c);
    for (label, written) in [
        ("first", first.to_json()),
        ("second", restore_and_save(&Document::from_json(&first.to_json()).unwrap()).to_json()),
    ] {
        let value: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["futureTopLevel"], top, "{label} write");
        assert_eq!(value["elements"][0]["futureElementKey"], nested, "{label} write");
        let upstream: Value = serde_json::from_str(text(c, "output")).unwrap();
        assert_eq!(upstream["elements"][0]["futureElementKey"], nested, "upstream keeps it");
        assert!(upstream.get("futureTopLevel").is_none(), "upstream drops it");
    }
}

// -- every fixture ------------------------------------------------------------------

/// The scene a fixture file carries, as the port reads it, for files the
/// port can read (embedded PNG and SVG); `None` for the TypeScript modules,
/// whose objects only upstream's generator can evaluate.
fn embedded_scene(name: &str, bytes: &[u8]) -> Option<String> {
    if name.ends_with(".png") {
        Some(decode_png_metadata(bytes).unwrap_or_else(|e| panic!("{name}: {e}")).expect(name))
    } else if name.ends_with(".svg") {
        let svg = std::str::from_utf8(bytes).expect("svg is UTF-8");
        Some(decode_svg_base64_payload(svg).unwrap_or_else(|e| panic!("{name}: {e}")))
    } else {
        None
    }
}

/// A scene-bearing fixture: a library, a PNG or SVG with an embedded scene,
/// or one of the two TypeScript modules exporting scenes and elements.
fn is_scene_bearing(name: &str) -> bool {
    name.ends_with(".excalidrawlib")
        || (name.contains("_embedded_") && (name.ends_with(".png") || name.ends_with(".svg")))
        || name == "diagramFixture.ts"
        || name == "elementFixture.ts"
}

/// D1's "every fixture in upstream's test suite": each scene-bearing file
/// under the upstream fixtures has a round-trip case, and every case holds.
/// A file the patterns call not scene-bearing is checked to carry no scene.
#[test]
fn every_scene_bearing_upstream_fixture_round_trips() {
    let dir = repo_root().join(UPSTREAM_FIXTURES);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(names.len() >= 11, "{names:?}");

    let mut covered = BTreeSet::new();
    for name in &names {
        let rel = format!("{UPSTREAM_FIXTURES}/{name}");
        let bytes = read_repo(&rel);
        if !is_scene_bearing(name) {
            match name.rsplit('.').next() {
                Some("png") => assert!(decode_png_metadata(&bytes).is_err(), "{name} carries a scene"),
                Some("svg") => assert!(
                    decode_svg_base64_payload(std::str::from_utf8(&bytes).unwrap()).is_err(),
                    "{name} carries a scene"
                ),
                Some("ts") => {
                    let source = String::from_utf8(bytes).unwrap();
                    assert!(!source.contains("type: \"excalidraw"), "{name} exports a scene");
                    assert!(!source.contains("ExcalidrawElement"), "{name} exports elements");
                }
                _ => panic!("{name}: not a known kind of fixture; classify it"),
            }
            continue;
        }
        let matching: Vec<&Value> = cases()
            .iter()
            .filter(|c| c["files"].as_array().unwrap().iter().any(|f| f == rel.as_str()))
            .collect();
        assert!(
            !matching.is_empty(),
            "{rel} is scene-bearing but has no round-trip case: add one to tools/goldens/document-fixtures.mjs"
        );
        for c in matching {
            match c["kind"].as_str() {
                Some("scene") => {
                    if let Some(scene) = embedded_scene(name, &bytes) {
                        assert_eq!(scene, text(c, "input"), "{name}: the port decodes upstream's input");
                    }
                    check_scene_case(c);
                }
                Some("library") => {
                    assert_eq!(String::from_utf8(bytes.clone()).unwrap(), text(c, "input"), "{name}");
                    check_library_case(c);
                }
                kind => panic!("{name}: unknown case kind {kind:?}"),
            }
            covered.insert(c["id"].as_str().unwrap());
        }
    }
    let all: BTreeSet<&str> = cases().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(covered, all, "every case belongs to a fixture file");
}

/// The patterns name the files upstream's tests load scenes from.
#[test]
fn scene_bearing_patterns() {
    for name in [
        "fixture_library.excalidrawlib",
        "test_embedded_v1.png",
        "smiley_embedded_v2.svg",
        "diagramFixture.ts",
        "elementFixture.ts",
    ] {
        assert!(is_scene_bearing(name), "{name}");
    }
    for name in ["constants.ts", "deer.png", "smiley.png", "svg-image-exporting-reference.svg"] {
        assert!(!is_scene_bearing(name), "{name}");
    }
}

// -- loading and saving, step by step --------------------------------------------------

/// Upstream's load and save on scenes built for each step (the golden's
/// `edges`): files of live image elements only, falsy and malformed
/// top-level values, legacy appState, dropped and deleted elements, and the
/// files `loadFromBlob` rejects with "Error: invalid file".
#[test]
fn load_and_save_match_upstream_step_by_step() {
    let edges = golden()["edges"].as_array().expect("edges");
    assert!(edges.len() >= 8);
    for c in edges {
        let id = c["id"].as_str().unwrap();
        match load_and_save(text(c, "input")) {
            Ok(first) => {
                assert!(c.get("error").is_none(), "{id}: upstream throws {}", c["error"]);
                let written = first.to_json();
                assert_eq!(written, text(c, "output"), "{id}: first write");
                let second = load_and_save(&written).unwrap_or_else(|e| panic!("{id}: {e}"));
                assert_eq!(second.to_json(), text(c, "reload"), "{id}: second write");
            }
            Err(err) => assert_eq!(Some(err.to_string().as_str()), c["error"].as_str(), "{id}"),
        }
    }
}

#[test]
fn text_that_is_not_json_is_an_invalid_file() {
    let err = load_scene_json("{", &mut TestEnv::default(), &APP_ENV).unwrap_err();
    assert!(matches!(err, LoadSceneError::Json(_)), "{err:?}");
    assert_eq!(err.to_string(), "Error: invalid file");
    let err = load_scene_json("[]", &mut TestEnv::default(), &APP_ENV).unwrap_err();
    assert!(matches!(err, LoadSceneError::NotAScene), "{err:?}");
}
