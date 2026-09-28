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

use excali_core::constants::{
    DEFAULT_ELEMENT_PROPS, DEFAULT_TEXT_ALIGN, DEFAULT_VERTICAL_ALIGN,
};
use excali_core::element::{Element, ElementBase, ElementKind};
use excali_core::library::{
    hash_elements_version, hash_string, is_valid_library, library_items_hash, merge_library_items,
    parse_library_json, restore_library_items, serialize_library_as_json, LibraryError,
    LibraryItem, LibraryItemStatus, MIME_TYPE_EXCALIDRAWLIB,
};
use excali_core::restore::{BindingEnd, LegacyBinding, LegacyBindingRequest, RestoreEnv, TestEnv};
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
///
/// `TestEnv` has no element geometry, so a legacy arrow binding whose
/// target exists is dropped (ex-116); cases that reach that migration are
/// compared with upstream's output when the migration fails
/// (`outputWithoutGeometry`), and `legacy_binding_migration_goes_through_the_env`
/// checks the migrated form.
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
        let expected = case.get("outputWithoutGeometry").unwrap_or(&case["output"]);
        match (expected, &case["error"]) {
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
///
/// 51 of them hold arrow bindings saved before bindings had a `mode`
/// (1,245 binding ends), which upstream migrates with element geometry;
/// `TestEnv` has none (ex-116), so those are compared with upstream's
/// output when the migration fails. `aarondiel/logic-gates` holds 24 lines
/// whose `strokeWidth` is the string `"3"`: restore keeps it, and so does
/// the typed model, which writes it back as read (ex-117).
#[test]
fn catalogue_matches_upstream() {
    let f = fixture();
    let cases = f["catalogue"].as_array().expect("catalogue cases");
    assert_eq!(cases.len(), 232);
    let mut versions = [0; 3];
    let (mut with_geometry, mut ends) = (0, 0);
    for case in cases {
        let id = case["id"].as_str().expect("id");
        let text = repo_file(case["file"].as_str().expect("file"));
        let items =
            parse(&text, LibraryItemStatus::Published).unwrap_or_else(|e| panic!("{id}: {e}"));
        let output = serialize_library_as_json(&items, source(&f));
        versions[case["version"].as_u64().expect("version") as usize] += 1;
        assert_eq!(
            items.len() as u64,
            case["items"].as_u64().expect("items"),
            "{id}"
        );
        let expected = match case.get("output_sha256_without_geometry") {
            Some(sha) => {
                with_geometry += 1;
                ends += case["geometry"].as_u64().expect("geometry");
                sha
            }
            None => &case["output_sha256"],
        };
        assert_eq!(sha256(&output), expected.as_str().expect("sha"), "{id}");
    }
    assert_eq!(versions, [0, 70, 162]);
    assert_eq!((with_geometry, ends), (51, 1245));
}

/// `aarondiel/logic-gates`: the 24 lines whose `strokeWidth` is the string
/// `"3"` are kept with every other element, read as width 3 and written back
/// as `"3"`, and the file hashes to upstream's output.
#[test]
fn logic_gates_keeps_its_string_stroke_widths() {
    let f = fixture();
    let case = f["catalogue"]
        .as_array()
        .expect("catalogue")
        .iter()
        .find(|c| c["id"] == "aarondiel/logic-gates")
        .expect("case");
    assert!(case.get("geometry").is_none());
    let text = repo_file(case["file"].as_str().expect("file"));
    let items = parse(&text, LibraryItemStatus::Published).expect("parses");
    assert_eq!(case["items"], json!(items.len()));

    let raw: Value = serde_json::from_str(&text).expect("json");
    let mut raw_ids = Vec::new();
    let mut string_widths = Vec::new();
    for item in raw["libraryItems"].as_array().expect("libraryItems") {
        for e in item["elements"].as_array().expect("elements") {
            if e["isDeleted"] == json!(true) {
                continue;
            }
            let id = e["id"].as_str().expect("id").to_owned();
            if e["strokeWidth"].is_string() {
                assert_eq!(e["type"], "line");
                string_widths.push(id.clone());
            }
            raw_ids.push(id);
        }
    }
    assert_eq!(string_widths.len(), 24);
    let elements: Vec<&Element> = items.iter().flat_map(|i| &i.elements).collect();
    let ids: Vec<&str> = elements.iter().map(|e| e.base.id.as_str()).collect();
    assert_eq!(ids, raw_ids);
    for e in elements
        .iter()
        .filter(|e| string_widths.contains(&e.base.id))
    {
        assert_eq!(e.base.stroke_width, 3.0, "{}", e.base.id);
        assert_eq!(e.to_map()["strokeWidth"], json!("3"), "{}", e.base.id);
    }
    let output = serialize_library_as_json(&items, source(&f));
    assert_eq!(sha256(&output), case["output_sha256"].as_str().expect("sha"));
}

/// The legacy binding of `elements-legacy-binding-migrated`, answered by an
/// environment with upstream's `mode` and `fixedPoint`, gives upstream's
/// output.
#[test]
fn legacy_binding_migration_goes_through_the_env() {
    struct Geometry(TestEnv);
    impl RestoreEnv for Geometry {
        fn now(&mut self) -> f64 {
            self.0.now()
        }
        fn random_id(&mut self) -> String {
            self.0.random_id()
        }
        fn random_integer(&mut self) -> f64 {
            self.0.random_integer()
        }
        fn migrate_legacy_binding(
            &mut self,
            request: LegacyBindingRequest<'_>,
        ) -> Option<LegacyBinding> {
            assert_eq!(request.end, BindingEnd::Start);
            assert_eq!(request.bound_element["id"], json!("box"));
            Some(LegacyBinding {
                mode: json!("orbit"),
                fixed_point: json!([0.5001, 0.5001]),
            })
        }
    }
    let f = fixture();
    let case = f["parse"]
        .as_array()
        .expect("parse")
        .iter()
        .find(|c| c["id"] == "elements-legacy-binding-migrated")
        .expect("case");
    assert_eq!(case["geometry"], json!(1));
    let items = parse_library_json(
        case["input"].as_str().expect("input"),
        LibraryItemStatus::Unpublished,
        &mut Geometry(TestEnv::default()),
    )
    .expect("parses");
    assert_eq!(
        serialize_library_as_json(&items, source(&f)),
        case["output"].as_str().expect("output")
    );
}

/// Every `merge` case: `mergeLibraryItems(local, other)` of the two parsed
/// libraries.
#[test]
fn merge_matches_upstream() {
    let f = fixture();
    for case in f["merge"].as_array().expect("merge cases") {
        let id = case["id"].as_str().expect("id");
        let local = parse(
            case["local"].as_str().expect("local"),
            LibraryItemStatus::Unpublished,
        )
        .expect("local parses");
        let other = parse(
            case["other"].as_str().expect("other"),
            LibraryItemStatus::Unpublished,
        )
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
        let items = parse(
            case["input"].as_str().expect("input"),
            LibraryItemStatus::Unpublished,
        )
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
    let text = repo_file(
        "fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib",
    );
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
    assert_eq!(
        element.base.index.as_ref().map(|i| i.0.as_str()),
        Some("a0")
    );
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
    assert_eq!(
        items.len(),
        raw["library"].as_array().expect("library").len()
    );
    assert!(items
        .iter()
        .all(|i| i.status == LibraryItemStatus::Published));

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
        assert_eq!(
            serialize_library_as_json(&again, "https://excalidraw.com"),
            text,
            "{rel}"
        );
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
    assert_eq!(
        MIME_TYPE_EXCALIDRAWLIB,
        "application/vnd.excalidrawlib+json"
    );
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
    assert_eq!(
        ids(&merged),
        ["A-nonce", "A-order", "A-prefix", "C", "A", "B"]
    );
    // Only the local items are compared against: duplicates within the
    // other list are all added.
    let twice = merge_library_items(
        &[],
        &[
            item("C", vec![rect("c1", 4.0)]),
            item("C2", vec![rect("c1", 4.0)]),
        ],
    );
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
    let restored = restore_library_items(Some(&items), LibraryItemStatus::Published, &mut env)
        .expect("restores");
    assert_eq!(ids(&restored), ["id0", "keep"]);
    assert_eq!(restored[1].name.as_deref(), Some("Diamond"));
    assert_eq!(restored[1].created, 5.0);
    assert!(restored
        .iter()
        .all(|i| i.status == LibraryItemStatus::Published));
    // `undefined` is the default parameter, `[]`.
    assert!(
        restore_library_items(None, LibraryItemStatus::Unpublished, &mut env)
            .expect("restores")
            .is_empty()
    );
    // `null` is not iterable.
    assert_eq!(
        restore_library_items(Some(&Value::Null), LibraryItemStatus::Unpublished, &mut env)
            .unwrap_err()
            .to_string(),
        "libraryItems is not iterable"
    );
}

/// A known field holding a value of another JSON type than the model's
/// (`strokeWidth: "3"`, `fillStyle: "sparkles"`, `locked: "no"`, ...) keeps
/// its element, as upstream's restore keeps the value
/// (`restore.ts:451-491`): the model reads a typed view of it, and the
/// value is written back as read until the field is changed. Upstream's
/// bytes are the golden's `elements-odd-field-values-kept`.
#[test]
fn elements_with_values_the_model_has_no_form_for_are_kept_as_read() {
    let f = fixture();
    let case = f["parse"]
        .as_array()
        .expect("parse")
        .iter()
        .find(|c| c["id"] == "elements-odd-field-values-kept")
        .expect("case");
    let items = parse(
        case["input"].as_str().expect("input"),
        LibraryItemStatus::Unpublished,
    )
    .expect("parses");
    assert_eq!(ids(&items), ["i", "j"]);
    assert_eq!(
        serialize_library_as_json(&items, source(&f)),
        case["output"].as_str().expect("output")
    );
    let element_ids: Vec<&str> = items[0]
        .elements
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    assert_eq!(element_ids, ["sw", "fs", "mixed", "ln", "t", "f"]);

    // The typed view: `Number(value)` for a number field, truthiness for a
    // boolean, null where the field may be null, else a new element's value.
    let e = |id: &str| {
        items
            .iter()
            .flat_map(|i| &i.elements)
            .find(|e| e.base.id == id)
            .expect(id)
    };
    assert_eq!(e("sw").base.stroke_width, 3.0);
    assert_eq!(e("fs").base.fill_style, DEFAULT_ELEMENT_PROPS.fill_style);
    let mixed = &e("mixed").base;
    assert_eq!(mixed.stroke_style, DEFAULT_ELEMENT_PROPS.stroke_style);
    assert_eq!(mixed.roughness, 1.0);
    assert_eq!(mixed.opacity, 50.0);
    assert_eq!(mixed.angle.0, 1.0);
    assert_eq!(mixed.stroke_color, DEFAULT_ELEMENT_PROPS.stroke_color);
    assert_eq!(mixed.background_color, DEFAULT_ELEMENT_PROPS.background_color);
    assert!(mixed.locked);
    assert_eq!(mixed.seed, 9.0);
    assert_eq!(mixed.frame_id, None);
    assert!(mixed.group_ids.is_empty());
    assert_eq!(mixed.roundness, None);
    assert_eq!(mixed.bound_elements, None);
    assert_eq!(mixed.custom_data, None);
    assert_eq!(mixed.created, None);
    assert_eq!(mixed.updated, 0.0);
    assert_eq!(e("ln").base.stroke_width, 3.0);
    let ElementKind::Text(text) = &e("t").kind else {
        panic!("text")
    };
    assert_eq!(text.font_family.0, 1);
    assert_eq!(text.text_align, DEFAULT_TEXT_ALIGN);
    assert_eq!(text.vertical_align, DEFAULT_VERTICAL_ALIGN);
    assert_eq!(text.container_id, None);
    assert_eq!(text.line_height, 1.25);
    assert!(text.auto_resize);
    let ElementKind::Freedraw(freedraw) = &e("f").kind else {
        panic!("freedraw")
    };
    assert!(freedraw.simulate_pressure);

    // A changed field is written from the model; the others stay as read.
    let mut changed = e("mixed").clone();
    changed.base.stroke_width = 4.0;
    changed.base.opacity = 60.0;
    let map = changed.to_map();
    assert_eq!(map["strokeWidth"], json!(4));
    assert_eq!(map["opacity"], json!(60));
    assert_eq!(map["roughness"], json!("1"));
    assert_eq!(map["customData"], json!(4));
    let mut changed = e("sw").clone();
    changed.base.stroke_width = 1.0;
    assert_eq!(changed.to_map()["strokeWidth"], json!(1));
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
    let Value::Object(raw) = raw else {
        unreachable!()
    };
    let item = LibraryItem::from_map(raw.clone()).expect("reads");
    assert_eq!(item.id, "5");
    assert_eq!(item.status, LibraryItemStatus::Unpublished);
    assert_eq!(item.created, 7.0);
    assert_eq!(item.name.as_deref(), Some("Box"));
    assert_eq!(item.extra["extra"], json!({"k": [1, 2]}));
    let back = item.to_map();
    assert_eq!(back, raw);
    let keys: Vec<&String> = back.keys().collect();
    assert_eq!(
        keys,
        ["2", "elements", "id", "status", "created", "name", "extra"]
    );

    // A changed field is written from the model.
    let mut changed = item.clone();
    changed.status = LibraryItemStatus::Published;
    changed.id = "item".into();
    assert_eq!(changed.to_map()["status"], json!("published"));
    assert_eq!(changed.to_map()["id"], json!("item"));

    // Built in Rust: upstream's `LibraryItem` key order (types.ts:652-660).
    let built = LibraryItem::new(
        "n",
        LibraryItemStatus::Unpublished,
        vec![rect("a", 1.0)],
        9.0,
    );
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
