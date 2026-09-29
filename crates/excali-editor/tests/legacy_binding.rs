//! Legacy arrow binding migration on restore (ex-116).
//!
//! An arrow binding saved before bindings had a `mode` whose target exists
//! is migrated by upstream's `repairBinding`
//! (`packages/excalidraw/data/restore.ts:347-418` at the pinned commit): the
//! end's global position (`LinearElementEditor.getPointAtIndexGlobalCoordinates`)
//! decides `mode` (`isPointInElement`: `inside`, else `orbit`), an orbit
//! end is projected (`projectFixedPointOntoDiagonal` at `DEFAULT_ZOOM`),
//! and `calculateFixedPointForNonElbowArrowBinding` gives the `fixedPoint`.
//! `excali-core` asks its `RestoreEnv` for that; [`RoutingEnv`] answers it.
//!
//! Checked against upstream's own output, with nothing injected:
//!
//! - every case of excali-core's `restore-element.json`
//!   (`tools/goldens/restore-fixtures.mjs`) that reached the migration
//!   (`geometry`), restored with `RoutingEnv` over `TestEnv`;
//! - every library of the catalogue (`fixtures/libraries`) that holds
//!   legacy bindings to existing elements: 51 libraries, 1,245 binding
//!   ends, parsed as upstream's `parseLibraryJSON` does and written back,
//!   against `output_sha256` and, loaded once more, `reload_sha256`
//!   (`tools/goldens/library-fixtures.mjs`);
//! - the `elements-legacy-binding-migrated` parse case of `library.json`,
//!   byte for byte.

use std::io::Read;
use std::path::Path;

use excali_core::json;
use excali_core::library::{parse_library_json, serialize_library_as_json, LibraryItemStatus};
use excali_core::restore::{
    restore_element, BindingEnd, ElementsMap, LegacyBindingRequest, RestoreEnv, RestoreOptions,
    TestEnv,
};
use excali_editor::restore_env::{migrate_legacy_binding, RoutingEnv};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const RESTORE_ELEMENT: &str = include_str!("../../excali-core/tests/fixtures/restore-element.json");
const LIBRARY: &str = include_str!("../../excali-core/tests/fixtures/library.json");

fn object(value: &Value, what: &str) -> Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{what} is not an object"))
        .clone()
}

fn elements(value: &Value, what: &str) -> Vec<Map<String, Value>> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{what} is not an array"))
        .iter()
        .map(|e| object(e, what))
        .collect()
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
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

fn env() -> RoutingEnv<TestEnv> {
    RoutingEnv::new(TestEnv::default())
}

/// `restoreElement(element, arrayToMap(targets ?? [element]),
/// existing && arrayToMap(existing), opts)` for a fixture case.
fn restore_case(case: &Map<String, Value>) -> Value {
    let element = object(&case["element"], "element");
    let targets = match case.get("targets") {
        Some(t) => elements(t, "targets"),
        None => vec![element.clone()],
    };
    let targets = ElementsMap::new(&targets);
    let existing = case
        .get("existing")
        .map(|e| ElementsMap::new(&elements(e, "existing")));
    let opts = RestoreOptions {
        delete_invisible_elements: case
            .get("opts")
            .and_then(|o| o.get("deleteInvisibleElements"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let restored =
        restore_element(&element, &targets, existing.as_ref(), opts, &mut env()).expect("restores");
    restored.map_or(Value::Null, Value::Object)
}

/// Every `restore-element.json` case that reached the legacy migration
/// restores to upstream's result with `RoutingEnv`, bindings included.
#[test]
fn restore_element_geometry_cases_match_upstream() {
    let fixture: Value = serde_json::from_str(RESTORE_ELEMENT).expect("fixture parses");
    let cases = fixture["cases"].as_array().expect("cases");
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in cases.iter().filter(|c| c.get("geometry").is_some()) {
        let case = object(case, "case");
        let id = case["id"].as_str().expect("id");
        let got = json::to_string_compact(&restore_case(&case));
        let want = json::to_string_compact(&case["result"]);
        if got != want {
            failures.push(format!("{id}:\n got {got}\nwant {want}"));
        }
        checked += 1;
    }
    assert!(checked >= 40, "{checked} geometry cases");
    assert!(
        failures.is_empty(),
        "{} of {checked} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The migrated binding is upstream's: every migrated end of the fixture
/// keeps its target, with upstream's `mode` and `fixedPoint`, none dropped.
#[test]
fn migrated_bindings_are_kept() {
    let fixture: Value = serde_json::from_str(RESTORE_ELEMENT).expect("fixture parses");
    let mut ends = 0;
    for case in fixture["cases"].as_array().expect("cases") {
        let Some(geometry) = case.get("geometry").and_then(Value::as_array) else {
            continue;
        };
        let restored = restore_case(&object(case, "case"));
        for end in geometry {
            let key = match end.as_str() {
                Some("start") => "startBinding",
                Some("end") => "endBinding",
                other => panic!("end {other:?}"),
            };
            let binding = &restored[key];
            assert!(binding.is_object(), "{}: {key} dropped", case["id"]);
            assert_eq!(binding, &case["result"][key], "{}: {key}", case["id"]);
            ends += 1;
        }
    }
    assert!(ends >= 45, "{ends} ends");
}

/// A hand-built scene: an end short of a rectangle's left side snaps to
/// its midpoint (orbit), an end inside it keeps its position (inside).
#[test]
fn mode_and_fixed_point() {
    let rect = json!({
        "id": "r", "type": "rectangle", "x": 100, "y": 0, "width": 100, "height": 100,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": "a1", "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
        "boundElements": null, "updated": 1, "created": 1, "link": null, "locked": false
    });
    let arrow = |end_x: f64| {
        json!({
            "id": "a", "type": "arrow", "x": 0, "y": 50, "width": end_x, "height": 0,
            "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
            "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
            "opacity": 100, "groupIds": [], "frameId": null, "index": "a0",
            "roundness": null, "seed": 1, "version": 1, "versionNonce": 0,
            "isDeleted": false, "boundElements": null, "updated": 1, "created": 1,
            "link": null, "locked": false, "points": [[0, 0], [end_x, 0]],
            "startBinding": null, "endBinding": { "elementId": "r", "focus": 0, "gap": 5 },
            "startArrowhead": null, "endArrowhead": "arrow", "elbowed": false
        })
    };
    for (end_x, mode, fixed_point) in [
        (95.0, "orbit", json!([0, 0.5001])),
        (150.0, "inside", json!([0.5001, 0.5001])),
        (170.0, "inside", json!([0.7, 0.5001])),
    ] {
        let arrow = object(&arrow(end_x), "arrow");
        let all = [arrow.clone(), object(&rect, "rect")];
        let map = ElementsMap::new(&all);
        let bound = object(&rect, "rect");
        let binding = arrow["endBinding"].clone();
        let migrated = migrate_legacy_binding(&LegacyBindingRequest {
            arrow: &arrow,
            binding: &binding,
            bound_element: &bound,
            elements: &map,
            end: BindingEnd::End,
        })
        .expect("migrated");
        assert_eq!(migrated.mode, json!(mode), "{end_x}");
        assert_eq!(migrated.fixed_point, fixed_point, "{end_x}");
        // RoutingEnv answers with the same.
        assert_eq!(
            env().migrate_legacy_binding(LegacyBindingRequest {
                arrow: &arrow,
                binding: &binding,
                bound_element: &bound,
                elements: &map,
                end: BindingEnd::End,
            }),
            Some(migrated)
        );
    }
}

/// Every catalogue library with legacy bindings to existing elements
/// writes upstream's bytes with the migration, and so does its reload.
#[test]
fn catalogue_libraries_with_legacy_bindings_match_upstream() {
    let fixture: Value = serde_json::from_str(LIBRARY).expect("library.json parses");
    let source = fixture["source"].as_str().expect("source");
    let mut libraries = 0;
    let mut ends = 0;
    let mut failures = Vec::new();
    for case in fixture["catalogue"].as_array().expect("catalogue") {
        let Some(geometry) = case.get("geometry").and_then(Value::as_u64) else {
            continue;
        };
        let id = case["id"].as_str().expect("id");
        let text = repo_file(case["file"].as_str().expect("file"));
        let items = parse_library_json(&text, LibraryItemStatus::Published, &mut env())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let output = serialize_library_as_json(&items, source);
        if sha256(&output) != case["output_sha256"].as_str().expect("sha") {
            failures.push(format!("{id}: output"));
        }
        let reloaded = parse_library_json(&output, LibraryItemStatus::Published, &mut env())
            .unwrap_or_else(|e| panic!("{id} reload: {e}"));
        let reload = serialize_library_as_json(&reloaded, source);
        if sha256(&reload) != case["reload_sha256"].as_str().expect("sha") {
            failures.push(format!("{id}: reload"));
        }
        libraries += 1;
        ends += geometry;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!((libraries, ends), (51, 1245));
}

/// `library.json`'s `elements-legacy-binding-migrated`: upstream's output
/// byte for byte.
#[test]
fn library_parse_case_matches_upstream() {
    let fixture: Value = serde_json::from_str(LIBRARY).expect("library.json parses");
    let case = fixture["parse"]
        .as_array()
        .expect("parse")
        .iter()
        .find(|c| c["id"] == "elements-legacy-binding-migrated")
        .expect("case");
    let status = match case["defaultStatus"].as_str() {
        Some("published") => LibraryItemStatus::Published,
        _ => LibraryItemStatus::Unpublished,
    };
    let items = parse_library_json(case["input"].as_str().expect("input"), status, &mut env())
        .expect("parses");
    assert_eq!(
        serialize_library_as_json(&items, fixture["source"].as_str().expect("source")),
        case["output"].as_str().expect("output")
    );
}
