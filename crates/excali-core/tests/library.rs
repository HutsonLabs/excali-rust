//! The `.excalidrawlib` library format (`site/content/research/data-model.md`,
//! section 4) against upstream's own `parseLibraryJSON`
//! (`packages/excalidraw/data/blob.ts:218-228`), `restoreLibraryItems`
//! (`data/restore.ts:1374-1415`), `serializeLibraryAsJSON`
//! (`data/json.ts:137-145`), `mergeLibraryItems` (`data/library.ts:122-157`)
//! and `getLibraryItemsHash` (`data/library.ts:594-605`) at the pinned
//! commit, recorded by `tools/goldens/library-fixtures.mjs` in
//! `tests/fixtures/library.json`: upstream's test mode (ids `id0`, `id1`,
//! ..., timestamps 1, `reseed(1)` before each parse), which
//! `restore::TestEnv` reproduces.
//!
//! The acceptance fixtures are upstream's
//! `packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib` (v1,
//! key `library`) and the catalogue's `jumpingrivers/r.excalidrawlib` (v1)
//! and `youritjang/stick-figures.excalidrawlib` (v2, key `libraryItems`);
//! every other catalogue library of `fixtures/libraries` is checked against
//! upstream's output too.

use std::io::Read;

use excali_core::element::{Element, ElementBase, ElementKind};
use excali_core::library::{
    hash_elements_version, hash_string, is_valid_library, library_items_hash,
    merge_library_items, parse_library_json, restore_library_items, serialize_library_as_json,
    LibraryError, LibraryItem, LibraryItemStatus, MIME_TYPE_EXCALIDRAWLIB,
};
use excali_core::restore::TestEnv;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const FIXTURE: &str = include_str!("fixtures/library.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("library.json parses")
}

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if rel.ends_with(".gz") {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut text)
            .expect("gzip");
        text
    } else {
        String::from_utf8(bytes).expect("utf-8")
    }
}

fn status(name: &str) -> LibraryItemStatus {
    match name {
        "published" => LibraryItemStatus::Published,
        "unpublished" => LibraryItemStatus::Unpublished,
        other => panic!("status {other}"),
    }
}

fn source(f: &Value) -> &str {
    f["source"].as_str().expect("source")
}

fn parse(text: &str, default_status: LibraryItemStatus) -> Result<Vec<LibraryItem>, LibraryError> {
    parse_library_json(text, default_status, &mut TestEnv::default())
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// -- upstream's output --------------------------------------------------------------

/// Every `parse` case: the items `parseLibraryJSON` gives, written by
/// `serializeLibraryAsJSON` byte for byte, or the error it throws.
#[test]
fn parse_matches_upstream() {
    let f = fixture();
    let cases = f["parse"].as_array().expect("parse cases");
    assert!(cases.len() > 60);
    for case in cases {
        let id = case["id"].as_str().expect("id");
        let input = case["input"].as_str().expect("input");
        let default_status = status(case["defaultStatus"].as_str().expect("defaultStatus"));
        let result = parse(input, default_status);
        match (&case["output"], &case["error"]) {
            (Value::String(output), _) => {
                let items = result.unwrap_or_else(|e| panic!("{id}: {e}"));
                assert_eq!(
                    serialize_library_as_json(&items, source(&f)),
                    *output,
                    "{id}"
                );
            }
            (_, Value::String(error)) => {
                let err = result.err().unwrap_or_else(|| panic!("{id}: parsed"));
                match err {
                    // V8's SyntaxError wording is not reproduced.
                    LibraryError::Json(_) => {
                        assert!(id.starts_with("json-syntax") || id == "json-empty", "{id}")
                    }
                    err => assert_eq!(err.to_string(), *error, "{id}"),
                }
            }
            _ => panic!("{id}: no output or error"),
        }
    }
}

/// Every library of the catalogue (`fixtures/libraries`, 232 files, 70 of
/// version 1 and 162 of version 2): parsed as an import from
/// libraries.excalidraw.com (`"published"`), written back, and hashed.
#[test]
fn catalogue_matches_upstream() {
    let f = fixture();
    let cases = f["catalogue"].as_array().expect("catalogue cases");
    assert_eq!(cases.len(), 232);
    let mut versions = [0; 3];
    for case in cases {
        let id = case["id"].as_str().expect("id");
        let text = repo_file(case["file"].as_str().expect("file"));
        let items = parse(&text, LibraryItemStatus::Published).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(items.len() as u64, case["items"].as_u64().expect("items"), "{id}");
        let output = serialize_library_as_json(&items, source(&f));
        assert_eq!(sha256(&output), case["output_sha256"].as_str().expect("sha"), "{id}");
        versions[case["version"].as_u64().expect("version") as usize] += 1;
    }
    assert_eq!(versions, [0, 70, 162]);
}

/// Every `merge` case: `mergeLibraryItems(local, other)` of the two parsed
/// libraries.
#[test]
fn merge_matches_upstream() {
    let f = fixture();
    for case in f["merge"].as_array().expect("merge cases") {
        let id = case["id"].as_str().expect("id");
        let local = parse(case["local"].as_str().expect("local"), LibraryItemStatus::Unpublished)
            .expect("local parses");
        let other = parse(case["other"].as_str().expect("other"), LibraryItemStatus::Unpublished)
            .expect("other parses");
        let merged = merge_library_items(&local, &other);
        assert_eq!(
            serialize_library_as_json(&merged, source(&f)),
            case["output"].as_str().expect("output"),
            "{id}"
        );
    }
}

/// Every `hash` case: `getLibraryItemsHash` of the parsed items.
#[test]
fn hash_matches_upstream() {
    let f = fixture();
    for case in f["hash"].as_array().expect("hash cases") {
        let id = case["id"].as_str().expect("id");
        let items = parse(case["input"].as_str().expect("input"), LibraryItemStatus::Unpublished)
            .expect("parses");
        assert_eq!(
            f64::from(library_items_hash(&items)),
            case["hash"].as_f64().expect("hash"),
            "{id}"
        );
    }
}

// -- acceptance: the v1 and v2 files ----------------------------------------------

/// `library.test.tsx` "import library via drag&drop": upstream's v1
/// fixture gives one unpublished item holding element `A`, with a fresh id
/// and creation time.
#[test]
fn upstream_v1_fixture_parses() {
    let text = repo_file("fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib");
    let items = parse(&text, LibraryItemStatus::Unpublished).expect("parses");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.status, LibraryItemStatus::Unpublished);
    assert_eq!(item.id, "id0");
    assert_eq!(item.created, 1.0);
    assert_eq!(item.elements.len(), 1);
    let element = &item.elements[0];
    assert_eq!(element.base.id, "A");
    // Legacy keys are migrated away by restore.
    let map = element.to_map();
    assert!(!map.contains_key("strokeSharpness"));
    assert!(!map.contains_key("boundElementIds"));
    assert_eq!(map["boundElements"], json!([]));
    assert_eq!(element.base.index.as_ref().map(|i| i.0.as_str()), Some("a0"));
    // Key order of the migrated item (restore.ts:1389-1394).
    let keys: Vec<String> = item.to_map().keys().cloned().collect();
    assert_eq!(keys, ["status", "elements", "id", "created"]);
}

#[test]
fn catalogue_v1_and_v2_files_parse() {
    let r = repo_file("fixtures/libraries/jumpingrivers/r.excalidrawlib.gz");
    let raw: Value = serde_json::from_str(&r).expect("json");
    assert_eq!(raw["version"], json!(1));
    assert!(raw.get("library").is_some() && raw.get("libraryItems").is_none());
    let items = parse(&r, LibraryItemStatus::Published).expect("v1 parses");
    assert_eq!(items.len(), raw["library"].as_array().expect("library").len());
    assert!(items.iter().all(|i| i.status == LibraryItemStatus::Published));

    let sticks = repo_file("fixtures/libraries/youritjang/stick-figures.excalidrawlib.gz");
    let raw: Value = serde_json::from_str(&sticks).expect("json");
    assert_eq!(raw["version"], json!(2));
    let raw_items = raw["libraryItems"].as_array().expect("libraryItems");
    let items = parse(&sticks, LibraryItemStatus::Unpublished).expect("v2 parses");
    assert_eq!(items.len(), raw_items.len());
    for (item, raw) in items.iter().zip(raw_items) {
        // A v2 item keeps its own id, status and creation time.
        assert_eq!(item.id, raw["id"].as_str().expect("id"));
        assert_eq!(item.status, status(raw["status"].as_str().expect("status")));
        assert_eq!(item.created, raw["created"].as_f64().expect("created"));
        let raw_ids: Vec<&str> = raw["elements"]
            .as_array()
            .expect("elements")
            .iter()
            .filter(|e| e["isDeleted"] != json!(true))
            .map(|e| e["id"].as_str().expect("element id"))
            .collect();
        let ids: Vec<&str> = item.elements.iter().map(|e| e.base.id.as_str()).collect();
        assert_eq!(ids, raw_ids);
    }
}

/// Parsing is stable: what `serializeLibraryAsJSON` writes parses back to
/// the same items.
#[test]
fn serialized_library_parses_back_to_the_same_items() {
    for rel in [
        "fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib",
        "fixtures/libraries/jumpingrivers/r.excalidrawlib.gz",
        "fixtures/libraries/youritjang/stick-figures.excalidrawlib.gz",
    ] {
        let items = parse(&repo_file(rel), LibraryItemStatus::Unpublished).expect("parses");
        let text = serialize_library_as_json(&items, "https://excalidraw.com");
        let again = parse(&text, LibraryItemStatus::Unpublished).expect("parses back");
        assert_eq!(again, items, "{rel}");
        assert_eq!(serialize_library_as_json(&again, "https://excalidraw.com"), text, "{rel}");
    }
}

// -- the rules ------------------------------------------------------------------

#[test]
fn valid_library_envelope() {
    let valid = |v: Value| is_valid_library(&v);
    assert!(valid(json!({"type": "excalidrawlib", "version": 1})));
    assert!(valid(json!({"type": "excalidrawlib", "version": 2})));
    assert!(valid(json!({"type": "excalidrawlib", "version": 2.0})));
    assert!(!valid(json!({"type": "excalidrawlib", "version": 3})));
    assert!(!valid(json!({"type": "excalidrawlib", "version": "2"})));
    assert!(!valid(json!({"type": "excalidrawlib"})));
    assert!(!valid(json!({"type": "excalidraw", "version": 2})));
    assert!(!valid(json!(null)));
    assert!(!valid(json!([])));
    assert!(!valid(json!("excalidrawlib")));
    assert_eq!(MIME_TYPE_EXCALIDRAWLIB, "application/vnd.excalidrawlib+json");
}

fn rect(id: &str, nonce: f64) -> Element {
    let mut base = ElementBase::new(id, 0.0, 0.0, 1.0, 1.0);
    base.version_nonce = nonce;
    Element::new(base, ElementKind::Rectangle)
}

fn item(id: &str, elements: Vec<Element>) -> LibraryItem {
    LibraryItem::new(id, LibraryItemStatus::Published, elements, 1.0)
}

fn ids(items: &[LibraryItem]) -> Vec<&str> {
    items.iter().map(|i| i.id.as_str()).collect()
}

/// `isUniqueItem` (`library.ts:122-141`): an item is already there when an
/// existing item has the same number of elements with the same `id` and
/// `versionNonce` in the same order; `mergeLibraryItems` puts the new ones
/// first, in their order.
#[test]
fn merge_dedupes_by_element_id_and_nonce_in_order() {
    let local = vec![
        item("A", vec![rect("a1", 1.0), rect("a2", 2.0)]),
        item("B", vec![rect("b1", 3.0)]),
    ];
    let other = vec![
        // same elements under another item id: a duplicate
        item("A-copy", vec![rect("a1", 1.0), rect("a2", 2.0)]),
        // another nonce
        item("A-nonce", vec![rect("a1", 1.0), rect("a2", 9.0)]),
        // same elements, other z-order
        item("A-order", vec![rect("a2", 2.0), rect("a1", 1.0)]),
        // a prefix
        item("A-prefix", vec![rect("a1", 1.0)]),
        item("B", vec![rect("b1", 3.0)]),
        item("C", vec![rect("c1", 4.0)]),
    ];
    let merged = merge_library_items(&local, &other);
    assert_eq!(ids(&merged), ["A-nonce", "A-order", "A-prefix", "C", "A", "B"]);
    // Only the local items are compared against: duplicates within the
    // other list are all added.
    let twice = merge_library_items(&[], &[item("C", vec![rect("c1", 4.0)]), item("C2", vec![rect("c1", 4.0)])]);
    assert_eq!(ids(&twice), ["C", "C2"]);
    assert!(merge_library_items(&local, &[]) == local);
}

#[test]
fn restore_items_from_a_value() {
    let items = json!([
        [{"type": "ellipse", "id": "e"}],
        {"id": "keep", "elements": [{"type": "diamond", "id": "d"}], "created": 5, "name": "Diamond"},
        {"elements": [{"type": "diamond", "id": "gone", "isDeleted": true}]},
        {"elements": []},
        3
    ]);
    let mut env = TestEnv::default();
    let restored = restore_library_items(Some(&items), LibraryItemStatus::Published, &mut env).expect("restores");
    assert_eq!(ids(&restored), ["id0", "keep"]);
    assert_eq!(restored[1].name.as_deref(), Some("Diamond"));
    assert_eq!(restored[1].created, 5.0);
    assert!(restored.iter().all(|i| i.status == LibraryItemStatus::Published));
    // `undefined` is the default parameter, `[]`.
    assert!(restore_library_items(None, LibraryItemStatus::Unpublished, &mut env).expect("restores").is_empty());
    // `null` is not iterable.
    assert_eq!(
        restore_library_items(Some(&Value::Null), LibraryItemStatus::Unpublished, &mut env)
            .unwrap_err()
            .to_string(),
        "libraryItems is not iterable"
    );
}

/// Stricter than upstream, as the typed model must be: an element whose
/// restored form [`Element::from_map`] cannot read (a `fillStyle` no
/// version of Excalidraw writes) is dropped with the elements
/// `restoreElement` rejects, where upstream keeps the object as it is.
#[test]
fn elements_the_typed_model_cannot_read_are_dropped() {
    let text = json!({
        "type": "excalidrawlib",
        "version": 2,
        "libraryItems": [
            {"id": "i", "status": "published", "created": 1, "elements": [
                {"type": "rectangle", "id": "odd", "fillStyle": "sparkles"},
                {"type": "rectangle", "id": "ok"}
            ]},
            {"id": "j", "status": "published", "created": 1, "elements": [
                {"type": "rectangle", "id": "odd", "fillStyle": "sparkles"}
            ]}
        ]
    })
    .to_string();
    let items = parse(&text, LibraryItemStatus::Unpublished).expect("parses");
    assert_eq!(ids(&items), ["i"]);
    let element_ids: Vec<&str> = items[0].elements.iter().map(|e| e.base.id.as_str()).collect();
    assert_eq!(element_ids, ["ok"]);
}

/// `LibraryItem` as a typed codec for items already restored (a
/// persisted library): unknown keys and key order are kept, values the
/// typed model has no form for are written back as read.
#[test]
fn library_item_codec_round_trip() {
    let raw = json!({
        "2": "index key",
        "elements": [rect("a", 1.0).to_map()],
        "id": 5,
        "status": "draft",
        "created": 7,
        "name": "Box",
        "extra": {"k": [1, 2]}
    });
    let Value::Object(raw) = raw else { unreachable!() };
    let item = LibraryItem::from_map(raw.clone()).expect("reads");
    assert_eq!(item.id, "5");
    assert_eq!(item.status, LibraryItemStatus::Unpublished);
    assert_eq!(item.created, 7.0);
    assert_eq!(item.name.as_deref(), Some("Box"));
    assert_eq!(item.extra["extra"], json!({"k": [1, 2]}));
    let back = item.to_map();
    assert_eq!(back, raw);
    let keys: Vec<&String> = back.keys().collect();
    assert_eq!(keys, ["2", "elements", "id", "status", "created", "name", "extra"]);

    // A changed field is written from the model.
    let mut changed = item.clone();
    changed.status = LibraryItemStatus::Published;
    changed.id = "item".into();
    assert_eq!(changed.to_map()["status"], json!("published"));
    assert_eq!(changed.to_map()["id"], json!("item"));

    // Built in Rust: upstream's `LibraryItem` key order (types.ts:652-660).
    let built = LibraryItem::new("n", LibraryItemStatus::Unpublished, vec![rect("a", 1.0)], 9.0);
    let keys: Vec<String> = built.to_map().keys().cloned().collect();
    assert_eq!(keys, ["id", "status", "elements", "created"]);

    for missing in ["id", "status", "elements", "created"] {
        let mut partial = raw.clone();
        partial.shift_remove(missing);
        assert!(LibraryItem::from_map(partial).is_err(), "{missing}");
    }
    let mut bad: Map<String, Value> = raw.clone();
    bad.insert("elements".into(), json!([1]));
    assert!(LibraryItem::from_map(bad).is_err());
}

#[test]
fn serialize_envelope() {
    let text = serialize_library_as_json(&[], "https://example.com");
    assert_eq!(
        text,
        "{\n  \"type\": \"excalidrawlib\",\n  \"version\": 2,\n  \"source\": \"https://example.com\",\n  \"libraryItems\": []\n}"
    );
}

/// djb2 over UTF-16 code units and over `versionNonce`s
/// (`packages/element/src/index.ts:21-41`), as unsigned 32-bit integers.
#[test]
fn djb2_hashes() {
    assert_eq!(hash_string(""), 5381);
    assert_eq!(hash_string("a"), 177_670);
    assert_eq!(hash_string("ab"), 5_863_208);
    assert_eq!(hash_elements_version(&[]), 5381);
    assert_eq!(hash_elements_version(&[rect("a", 1.0)]), 177_574);
    assert_eq!(library_items_hash(&[]), 5381);
}
