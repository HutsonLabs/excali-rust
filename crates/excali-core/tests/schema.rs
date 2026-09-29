//! JSON Schemas for `.excalidraw` and `.excalidrawlib` (ex-115), generated
//! from the model's types by `excali_core::schema` and published under
//! `site/static/schema/`.
//!
//! Upstream has no JSON Schema (`site/content/research/data-model.md`,
//! section 9); the TypeScript types are the spec: `_ExcalidrawElementBase`
//! and the per-type element types (`packages/element/src/types.ts:35-437`),
//! `ExportedDataState` (`packages/excalidraw/data/types.ts:14-21`),
//! `BinaryFileData` (`packages/excalidraw/types.ts:118-146`), the exported
//! `appState` keys (`appState.ts:153-291`, `types.ts:465-565`),
//! `ExportedLibraryData` / `ImportedLibraryData` (`data/types.ts:52-62`),
//! `LibraryItem` (`types.ts:649-662`) and `isValidLibrary`
//! (`data/json.ts:128-135`). A key that is non-optional there is required
//! in the schema, whether or not it may be `null`; an optional one (`?`) is
//! optional. Unknown keys are allowed everywhere, as the port keeps them.
//!
//! The schemas describe files as upstream and the port write them. Every
//! file below is upstream's own output: the scene fixtures generated from
//! upstream's constructors, the elements upstream's `restoreElements`
//! returns (`tests/fixtures/restore-elements.json`), and all 232 catalogue
//! libraries after the port's restore, which matches upstream's output byte
//! for byte (`tests/library.rs`).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use excali_core::document::Document;
use excali_core::element::{Arrowhead, Element, ElementType};
use excali_core::library::{parse_library_json, serialize_library_as_json, LibraryItemStatus};
use excali_core::restore::TestEnv;
use excali_core::schema;
use serde_json::{json, Map, Value};

const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn repo_file(rel: &str) -> String {
    let path = repo_path(rel);
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

fn fixture(name: &str) -> String {
    repo_file(&format!("crates/excali-core/tests/fixtures/{name}"))
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

fn validator(schema: &Value) -> jsonschema::Validator {
    jsonschema::draft202012::new(schema).unwrap_or_else(|e| panic!("schema compiles: {e}"))
}

/// A validator for one definition of a published schema.
fn def_validator(root: &Value, name: &str) -> jsonschema::Validator {
    assert!(
        root["$defs"].get(name).is_some(),
        "the schema defines {name}"
    );
    validator(&json!({
        "$schema": DRAFT_2020_12,
        "$ref": format!("#/$defs/{name}"),
        "$defs": root["$defs"],
    }))
}

fn errors(v: &jsonschema::Validator, instance: &Value) -> Vec<String> {
    v.iter_errors(instance)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect()
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value, what: &str) {
    let errs = errors(v, instance);
    assert!(errs.is_empty(), "{what}: {errs:#?}");
}

/// Invalid, with an error at `path` (a JSON pointer into the instance) or
/// below it.
fn assert_invalid_at(v: &jsonschema::Validator, instance: &Value, path: &str, what: &str) {
    let paths: Vec<String> = v
        .iter_errors(instance)
        .map(|e| e.instance_path().to_string())
        .collect();
    assert!(!paths.is_empty(), "{what}: accepted");
    let below = format!("{path}/");
    assert!(
        paths.iter().any(|p| p == path || p.starts_with(&below)),
        "{what}: no error at {path:?}, errors at {paths:?}"
    );
}

fn excalidraw() -> Value {
    schema::excalidraw()
}

fn excalidrawlib() -> Value {
    schema::excalidrawlib()
}

/// The elements of `every-type.excalidraw`, one of each persisted type
/// (and an elbow arrow), built by upstream's constructors.
fn every_type() -> Vec<Value> {
    parse(&fixture("every-type.excalidraw"))["elements"]
        .as_array()
        .expect("elements")
        .clone()
}

fn element_of(ty: &str) -> Value {
    every_type()
        .into_iter()
        .find(|e| e["type"] == ty)
        .unwrap_or_else(|| panic!("fixture has a {ty}"))
}

fn with(mut value: Value, key: &str, v: Value) -> Value {
    value
        .as_object_mut()
        .expect("object")
        .insert(key.to_owned(), v);
    value
}

fn without(mut value: Value, key: &str) -> Value {
    value.as_object_mut().expect("object").shift_remove(key);
    value
}

// -- publication --------------------------------------------------------------

/// The files under `site/static/schema/` are exactly what the model
/// generates; regenerate them with
/// `cargo run -p excali-core --example write-schemas`.
#[test]
fn published_files_match_the_model() {
    let published = schema::published();
    let names: Vec<&str> = published.iter().map(|p| p.file_name).collect();
    assert_eq!(
        names,
        ["excalidraw.schema.json", "excalidrawlib.schema.json"]
    );
    for p in published {
        let expected = schema::to_json(&(p.schema)());
        let path = format!("site/static/schema/{}", p.file_name);
        assert!(
            repo_path(&path).exists(),
            "{path} missing: run `cargo run -p excali-core --example write-schemas`"
        );
        assert!(
            repo_file(&path) == expected,
            "{path} is stale: run `cargo run -p excali-core --example write-schemas`"
        );
    }
}

#[test]
fn published_json_is_two_space_indented_with_a_final_newline() {
    let text = schema::to_json(&json!({"a": [1, {"b": true}]}));
    assert_eq!(
        text,
        "{\n  \"a\": [\n    1,\n    {\n      \"b\": true\n    }\n  ]\n}\n"
    );
}

#[test]
fn schemas_are_draft_2020_12_with_ids_on_the_site() {
    assert_eq!(
        schema::BASE_URL,
        "https://hutsonlabs.github.io/excali-rust/schema/"
    );
    // base_url in site/config.toml is where the site, and so the schemas,
    // are served from.
    let config = repo_file("site/config.toml");
    assert!(config.contains("base_url = \"https://hutsonlabs.github.io/excali-rust\""));
    for p in schema::published() {
        let s = (p.schema)();
        jsonschema::meta::validate(&s)
            .unwrap_or_else(|e| panic!("{} is a valid schema: {e}", p.file_name));
        assert_eq!(s["$schema"], DRAFT_2020_12, "{}", p.file_name);
        assert_eq!(
            s["$id"],
            format!("{}{}", schema::BASE_URL, p.file_name),
            "{}",
            p.file_name
        );
        assert!(s["title"].as_str().is_some_and(|t| !t.is_empty()));
        assert!(s["description"].as_str().is_some_and(|t| !t.is_empty()));
        // Self-contained: every $ref points into the file's own $defs.
        let mut refs = Vec::new();
        collect_refs(&s, &mut refs);
        assert!(!refs.is_empty());
        for r in refs {
            let name = r
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("{}: external $ref {r}", p.file_name));
            assert!(s["$defs"].get(name).is_some(), "{}: {r}", p.file_name);
        }
    }
}

fn collect_refs(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                match (k.as_str(), v) {
                    ("$ref", Value::String(r)) => out.push(r.clone()),
                    _ => collect_refs(v, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|v| collect_refs(v, out)),
        _ => {}
    }
}

#[test]
fn file_format_page_links_the_schemas() {
    let page = repo_file("site/content/architecture/file-format.md");
    // The page is served at /architecture/file-format/ below base_url, and
    // site/static/schema/ at /schema/.
    for file in ["excalidraw.schema.json", "excalidrawlib.schema.json"] {
        assert!(
            page.contains(&format!("](../../schema/{file})")),
            "file-format.md links {file}"
        );
    }
}

// -- .excalidraw ---------------------------------------------------------------

#[test]
fn scene_fixtures_are_valid() {
    let v = validator(&excalidraw());
    for name in [
        "empty-scene.excalidraw",
        "every-type.excalidraw",
        "unknown-keys.excalidraw",
        "unknown-keys-edited.excalidraw",
    ] {
        assert_valid(&v, &parse(&fixture(name)), name);
    }
}

/// A scene the port reads and writes back is still valid.
#[test]
fn scenes_written_by_the_port_are_valid() {
    let v = validator(&excalidraw());
    for name in ["every-type.excalidraw", "unknown-keys.excalidraw"] {
        let doc = Document::from_json(&fixture(name)).expect("reads");
        assert_valid(&v, &parse(&doc.to_json()), name);
    }
    let elements: Vec<Element> = every_type()
        .into_iter()
        .map(|e| Element::from_map(e.as_object().expect("object").clone()).expect("element"))
        .collect();
    let app_state = parse(&fixture("empty-scene.excalidraw"))["appState"]
        .as_object()
        .expect("appState")
        .clone();
    let built = Document::new(
        "https://github.com/HutsonLabs/excali-rust",
        elements,
        app_state,
        None,
    );
    assert_valid(&v, &parse(&built.to_json()), "Document::new, no files");
}

#[test]
fn every_element_type_is_described() {
    let v = def_validator(&excalidraw(), "Element");
    let elements = every_type();
    let types: BTreeSet<&str> = elements
        .iter()
        .map(|e| e["type"].as_str().expect("type"))
        .collect();
    // Selection is never persisted; the fixture has every other type.
    let persisted: BTreeSet<&str> = ElementType::ALL
        .iter()
        .filter(|t| **t != ElementType::Selection)
        .map(|t| t.as_str())
        .collect();
    assert_eq!(types, persisted);
    for e in every_type() {
        assert_valid(&v, &e, &e["type"].to_string());
    }
    // Upstream's type union includes selection (types.ts:89-91,223-234).
    let selection = with(element_of("rectangle"), "type", json!("selection"));
    assert_valid(&v, &selection, "selection");
    // Legacy draw is restore's business (restore.ts:612-637); unknown
    // types are dropped by restore.
    for ty in ["draw", "laser", "Rectangle", ""] {
        let e = with(element_of("line"), "type", json!(ty));
        assert_invalid_at(&v, &e, "", ty);
    }
    assert_invalid_at(&v, &without(element_of("line"), "type"), "", "no type");
}

/// `_ExcalidrawElementBase` (`types.ts:40-87`): every key but `customData`
/// is required, `null` where the type allows it.
#[test]
fn base_fields_follow_the_upstream_type() {
    let v = def_validator(&excalidraw(), "Element");
    let required = [
        "id",
        "x",
        "y",
        "strokeColor",
        "backgroundColor",
        "fillStyle",
        "strokeWidth",
        "strokeStyle",
        "roundness",
        "roughness",
        "opacity",
        "width",
        "height",
        "angle",
        "seed",
        "version",
        "versionNonce",
        "index",
        "isDeleted",
        "groupIds",
        "frameId",
        "boundElements",
        "updated",
        "created",
        "link",
        "locked",
    ];
    for e in every_type() {
        let ty = e["type"].as_str().expect("type").to_owned();
        for key in required {
            assert_invalid_at(
                &v,
                &without(e.clone(), key),
                "",
                &format!("{ty} without {key}"),
            );
        }
        assert_valid(&v, &without(e.clone(), "customData"), &ty);
        assert_valid(
            &v,
            &with(e.clone(), "customData", json!({"any": [1, "thing"]})),
            &ty,
        );
    }
    let rect = element_of("rectangle");
    let nullable = [
        "roundness",
        "index",
        "frameId",
        "boundElements",
        "created",
        "link",
    ];
    for key in nullable {
        assert_valid(&v, &with(rect.clone(), key, Value::Null), key);
    }
    let not_nullable = [
        "id",
        "x",
        "strokeColor",
        "fillStyle",
        "strokeWidth",
        "angle",
        "seed",
        "version",
        "isDeleted",
        "groupIds",
        "updated",
        "locked",
    ];
    for key in not_nullable {
        assert_invalid_at(
            &v,
            &with(rect.clone(), key, Value::Null),
            &format!("/{key}"),
            key,
        );
    }
    let set = [
        ("roundness", json!({"type": 3})),
        ("roundness", json!({"type": 2, "value": 32})),
        ("index", json!("a0")),
        ("frameId", json!("frame1")),
        (
            "boundElements",
            json!([{"id": "a", "type": "arrow"}, {"id": "t", "type": "text"}]),
        ),
        ("link", json!("https://example.com")),
        ("groupIds", json!(["g1", "g2"])),
        ("angle", json!(std::f64::consts::FRAC_PI_2)),
        ("strokeWidth", json!(0.5)),
    ];
    for (key, value) in set {
        assert_valid(
            &v,
            &with(rect.clone(), key, value.clone()),
            &format!("{key} {value}"),
        );
    }
}

/// The enumerations of `site/content/research/data-model.md`, section 2.
#[test]
fn enumerations_follow_upstream() {
    let v = def_validator(&excalidraw(), "Element");
    let rect = element_of("rectangle");
    for fill in ["hachure", "cross-hatch", "solid", "zigzag"] {
        assert_valid(&v, &with(rect.clone(), "fillStyle", json!(fill)), fill);
    }
    for fill in ["dots", "crossHatch", "Solid"] {
        assert_invalid_at(
            &v,
            &with(rect.clone(), "fillStyle", json!(fill)),
            "/fillStyle",
            fill,
        );
    }
    for stroke in ["solid", "dashed", "dotted"] {
        assert_valid(
            &v,
            &with(rect.clone(), "strokeStyle", json!(stroke)),
            stroke,
        );
    }
    assert_invalid_at(
        &v,
        &with(rect.clone(), "strokeStyle", json!("double")),
        "/strokeStyle",
        "strokeStyle double",
    );
    // ROUNDNESS, constants.ts:447-464.
    for t in [1, 2, 3] {
        assert_valid(
            &v,
            &with(rect.clone(), "roundness", json!({"type": t})),
            "roundness",
        );
    }
    for t in [json!(0), json!(4), json!(2.5), json!("3"), Value::Null] {
        assert_invalid_at(
            &v,
            &with(rect.clone(), "roundness", json!({"type": t})),
            "/roundness",
            &format!("roundness type {t}"),
        );
    }
    assert_invalid_at(
        &v,
        &with(rect.clone(), "roundness", json!({"value": 4})),
        "/roundness",
        "roundness without type",
    );
    assert_invalid_at(
        &v,
        &with(
            rect.clone(),
            "boundElements",
            json!([{"id": "l", "type": "line"}]),
        ),
        "/boundElements/0/type",
        "bound line",
    );

    let text = element_of("text");
    for align in ["left", "center", "right"] {
        assert_valid(&v, &with(text.clone(), "textAlign", json!(align)), align);
    }
    for align in ["top", "middle", "bottom"] {
        assert_valid(
            &v,
            &with(text.clone(), "verticalAlign", json!(align)),
            align,
        );
    }
    assert_invalid_at(
        &v,
        &with(text.clone(), "textAlign", json!("justify")),
        "/textAlign",
        "justify",
    );
    assert_invalid_at(
        &v,
        &with(text.clone(), "verticalAlign", json!("center")),
        "/verticalAlign",
        "vertical center",
    );
    // Font family ids are numbers (FONT_FAMILY, constants.ts:140-151);
    // unnamed ids (4, a host's own font) are kept.
    for family in [1, 4, 5, 10, 100, 1000] {
        assert_valid(
            &v,
            &with(text.clone(), "fontFamily", json!(family)),
            "family",
        );
    }
    for family in [json!("Virgil"), json!("5"), json!(-1), json!(1.5)] {
        assert_invalid_at(
            &v,
            &with(text.clone(), "fontFamily", family.clone()),
            "/fontFamily",
            &family.to_string(),
        );
    }

    // Arrowheads (types.ts:342-365); the legacy names are restore's
    // (normalizeArrowhead, arrowheads.ts:3-21).
    let arrow = element_of("arrow");
    for head in Arrowhead::ALL {
        for key in ["startArrowhead", "endArrowhead"] {
            assert_valid(
                &v,
                &with(arrow.clone(), key, json!(head.as_str())),
                head.as_str(),
            );
        }
    }
    for head in [
        "dot",
        "crowfoot_one",
        "crowfoot_many",
        "crowfoot_one_or_many",
        "none",
    ] {
        assert_invalid_at(
            &v,
            &with(arrow.clone(), "endArrowhead", json!(head)),
            "/endArrowhead",
            head,
        );
    }

    let image = element_of("image");
    for status in ["pending", "saved", "error"] {
        assert_valid(&v, &with(image.clone(), "status", json!(status)), status);
    }
    assert_invalid_at(
        &v,
        &with(image.clone(), "status", json!("loading")),
        "/status",
        "image status",
    );

    let freedraw = element_of("freedraw");
    for variability in ["variable", "constant"] {
        assert_valid(
            &v,
            &with(
                freedraw.clone(),
                "strokeOptions",
                json!({"variability": variability, "streamline": 0.5}),
            ),
            variability,
        );
    }
    assert_invalid_at(
        &v,
        &with(
            freedraw.clone(),
            "strokeOptions",
            json!({"variability": "fixed", "streamline": 0.5}),
        ),
        "/strokeOptions/variability",
        "variability",
    );
}

/// The per-type fields (`types.ts:97-437`): required unless optional (`?`)
/// upstream, or left out by restore, so absent from what upstream writes
/// (`polygon`, `elbowed`: `restore.ts:645-650, 697`; an image's `fileId`,
/// copied as `element.fileId` whether or not it is there,
/// `restore.ts:605-611`).
#[test]
fn per_type_fields_follow_the_upstream_types() {
    let v = def_validator(&excalidraw(), "Element");
    let cases: &[(&str, &[&str], &[&str])] = &[
        ("stickynote", &["baseHeight"], &[]),
        ("image", &["status", "scale", "crop"], &["fileId"]),
        ("frame", &["name"], &[]),
        ("magicframe", &["name"], &[]),
        (
            "text",
            &[
                "fontSize",
                "fontFamily",
                "baseFontSize",
                "text",
                "textAlign",
                "verticalAlign",
                "containerId",
                "originalText",
                "autoResize",
                "lineHeight",
            ],
            &["labelPosition"],
        ),
        (
            "line",
            &[
                "points",
                "startBinding",
                "endBinding",
                "startArrowhead",
                "endArrowhead",
            ],
            &["polygon"],
        ),
        (
            "arrow",
            &[
                "points",
                "startBinding",
                "endBinding",
                "startArrowhead",
                "endArrowhead",
            ],
            &["elbowed", "fixedSegments", "startIsSpecial", "endIsSpecial"],
        ),
        (
            "freedraw",
            &["points", "pressures", "simulatePressure", "strokeOptions"],
            &[],
        ),
    ];
    for (ty, required, optional) in cases {
        let e = element_of(ty);
        assert_valid(&v, &e, ty);
        for key in *required {
            assert_invalid_at(
                &v,
                &without(e.clone(), key),
                "",
                &format!("{ty} without {key}"),
            );
        }
        for key in *optional {
            assert_valid(&v, &without(e.clone(), key), &format!("{ty} without {key}"));
        }
    }
    // Nullable per-type fields.
    let nullable: &[(&str, &[&str])] = &[
        ("image", &["fileId", "crop"]),
        ("frame", &["name"]),
        ("text", &["baseFontSize", "containerId", "labelPosition"]),
        (
            "arrow",
            &[
                "startBinding",
                "endBinding",
                "startArrowhead",
                "endArrowhead",
                "fixedSegments",
                "startIsSpecial",
                "endIsSpecial",
            ],
        ),
    ];
    for (ty, keys) in nullable {
        for key in *keys {
            assert_valid(
                &v,
                &with(element_of(ty), key, Value::Null),
                &format!("{ty} {key}"),
            );
        }
    }
    for (ty, key) in [
        ("stickynote", "baseHeight"),
        ("image", "status"),
        ("image", "scale"),
        ("text", "text"),
        ("text", "autoResize"),
        ("line", "points"),
        ("line", "polygon"),
        ("arrow", "elbowed"),
        ("freedraw", "pressures"),
    ] {
        assert_invalid_at(
            &v,
            &with(element_of(ty), key, Value::Null),
            &format!("/{key}"),
            &format!("{ty} {key} null"),
        );
    }
}

#[test]
fn points_bindings_and_segments_have_upstream_shapes() {
    let v = def_validator(&excalidraw(), "Element");
    let arrow = element_of("arrow");
    // LocalPoint is [x, y] (packages/math/src/types.ts).
    for points in [
        json!([[0]]),
        json!([[0, 1, 2]]),
        json!([["0", 1]]),
        json!([{"x": 0, "y": 0}]),
    ] {
        assert_invalid_at(
            &v,
            &with(arrow.clone(), "points", points.clone()),
            "/points/0",
            &points.to_string(),
        );
    }
    assert_valid(&v, &with(arrow.clone(), "points", json!([])), "no points");
    // FixedPointBinding, types.ts:320-333.
    let binding = json!({"elementId": "r1", "fixedPoint": [0.5, 1], "mode": "orbit"});
    for mode in ["inside", "orbit", "skip"] {
        let b = with(binding.clone(), "mode", json!(mode));
        assert_valid(&v, &with(arrow.clone(), "startBinding", b), mode);
    }
    for key in ["elementId", "fixedPoint", "mode"] {
        assert_invalid_at(
            &v,
            &with(arrow.clone(), "endBinding", without(binding.clone(), key)),
            "/endBinding",
            &format!("binding without {key}"),
        );
    }
    // A binding saved before modes existed (focus/gap) is restore's to
    // migrate (restore.ts:347-418).
    assert_invalid_at(
        &v,
        &with(
            arrow.clone(),
            "endBinding",
            json!({"elementId": "r1", "focus": 0, "gap": 5}),
        ),
        "/endBinding",
        "legacy binding",
    );
    // FixedSegment, types.ts:385-389.
    let elbow = every_type()
        .into_iter()
        .find(|e| e["elbowed"] == json!(true))
        .expect("elbow arrow");
    let segment = json!({"start": [0, 30], "end": [40, 30], "index": 2});
    assert_valid(
        &v,
        &with(elbow.clone(), "fixedSegments", json!([segment])),
        "segment",
    );
    assert_invalid_at(
        &v,
        &with(
            elbow.clone(),
            "fixedSegments",
            json!([without(segment.clone(), "index")]),
        ),
        "/fixedSegments/0",
        "segment without index",
    );
    let crop =
        json!({"x": 0, "y": 0, "width": 10, "height": 10, "naturalWidth": 20, "naturalHeight": 20});
    let image = element_of("image");
    assert_valid(&v, &with(image.clone(), "crop", crop.clone()), "crop");
    assert_invalid_at(
        &v,
        &with(image.clone(), "crop", without(crop, "naturalWidth")),
        "/crop",
        "crop without naturalWidth",
    );
    assert_invalid_at(
        &v,
        &with(image, "scale", json!([1])),
        "/scale",
        "scale of one number",
    );
}

#[test]
fn unknown_keys_are_allowed_everywhere() {
    let scene = excalidraw();
    let v = validator(&scene);
    let mut doc = parse(&fixture("every-type.excalidraw"));
    doc["futureTopLevel"] = json!({"k": 1});
    doc["appState"]["theme"] = json!("dark");
    doc["appState"]["zoom"] = json!({"value": 1});
    doc["files"]["file1"]["futureFileKey"] = json!(true);
    for e in doc["elements"].as_array_mut().expect("elements") {
        e["pluginData"] = json!([1, 2]);
    }
    assert_valid(&v, &doc, "unknown keys");
    let binding = json!({"elementId": "r1", "fixedPoint": [0.5, 1], "mode": "orbit", "extra": 1});
    let arrow = with(element_of("arrow"), "startBinding", binding);
    assert_valid(
        &def_validator(&scene, "Element"),
        &arrow,
        "unknown binding key",
    );
}

#[test]
fn scene_envelope_follows_exported_data_state() {
    let v = validator(&excalidraw());
    let doc = parse(&fixture("every-type.excalidraw"));
    // ExportedDataState (data/types.ts:14-21); files is undefined, so
    // absent, for a database save (json.ts:67-71).
    for key in ["type", "version", "source", "elements", "appState"] {
        assert_invalid_at(
            &v,
            &without(doc.clone(), key),
            "",
            &format!("without {key}"),
        );
    }
    assert_valid(&v, &without(doc.clone(), "files"), "without files");
    for ty in ["excalidrawlib", "excalidraw/clipboard", "Excalidraw"] {
        assert_invalid_at(&v, &with(doc.clone(), "type", json!(ty)), "/type", ty);
    }
    assert_invalid_at(
        &v,
        &with(doc.clone(), "version", json!("2")),
        "/version",
        "version string",
    );
    assert_invalid_at(
        &v,
        &with(doc.clone(), "source", json!(1)),
        "/source",
        "source number",
    );
    assert_invalid_at(
        &v,
        &with(doc.clone(), "elements", json!({})),
        "/elements",
        "elements object",
    );
    assert_invalid_at(
        &v,
        &with(doc.clone(), "appState", json!([])),
        "/appState",
        "appState array",
    );
    assert_invalid_at(
        &v,
        &with(doc.clone(), "files", json!([])),
        "/files",
        "files array",
    );
    let mut bad = doc.clone();
    bad["elements"][3]["fillStyle"] = json!("dots");
    assert_invalid_at(&v, &bad, "/elements/3/fillStyle", "element in a scene");
}

/// The five keys `cleanAppStateForExport` keeps (`appState.ts:153-291`),
/// typed as `AppState` has them (`types.ts:465-565`).
#[test]
fn exported_app_state_keys_are_typed() {
    let v = validator(&excalidraw());
    let doc = parse(&fixture("empty-scene.excalidraw"));
    let state = |key: &str, value: Value| {
        let mut d = doc.clone();
        d["appState"][key] = value;
        d
    };
    assert_valid(
        &v,
        &with(doc.clone(), "appState", json!({})),
        "empty appState",
    );
    let good = [
        ("gridSize", json!(20)),
        ("gridStep", json!(5)),
        ("gridModeEnabled", json!(true)),
        ("viewBackgroundColor", json!("#121212")),
        ("lockedMultiSelections", json!({"g1": true})),
    ];
    for (key, value) in good {
        assert_valid(&v, &state(key, value), key);
    }
    let bad = [
        ("gridSize", json!("20")),
        ("gridStep", Value::Null),
        ("gridModeEnabled", json!(1)),
        ("viewBackgroundColor", json!(0)),
        ("lockedMultiSelections", json!({"g1": false})),
        ("lockedMultiSelections", json!(["g1"])),
    ];
    for (key, value) in bad {
        let at = match key {
            "lockedMultiSelections" if value.is_object() => format!("/appState/{key}/g1"),
            _ => format!("/appState/{key}"),
        };
        assert_invalid_at(
            &v,
            &state(key, value.clone()),
            &at,
            &format!("{key} {value}"),
        );
    }
}

/// `BinaryFileData` (`types.ts:118-146`).
#[test]
fn files_follow_binary_file_data() {
    let v = validator(&excalidraw());
    let doc = parse(&fixture("every-type.excalidraw"));
    let file = doc["files"]["file1"].clone();
    let with_file = |f: Value| {
        let mut d = doc.clone();
        d["files"]["file1"] = f;
        d
    };
    for key in ["mimeType", "id", "dataURL", "created"] {
        assert_invalid_at(
            &v,
            &with_file(without(file.clone(), key)),
            "/files/file1",
            &format!("file without {key}"),
        );
    }
    for key in ["lastRetrieved", "version"] {
        assert_valid(&v, &with_file(without(file.clone(), key)), key);
    }
    assert_valid(
        &v,
        &with_file(with(file.clone(), "version", json!(2))),
        "version",
    );
    // IMAGE_MIME_TYPES and MIME_TYPES.binary, constants.ts:296-330.
    for mime in [
        "image/svg+xml",
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/bmp",
        "image/x-icon",
        "image/avif",
        "image/jfif",
        "application/octet-stream",
    ] {
        assert_valid(
            &v,
            &with_file(with(file.clone(), "mimeType", json!(mime))),
            mime,
        );
    }
    for mime in ["text/plain", "image/tiff", "application/json"] {
        assert_invalid_at(
            &v,
            &with_file(with(file.clone(), "mimeType", json!(mime))),
            "/files/file1/mimeType",
            mime,
        );
    }
    // DataURL: "data:<mime>;base64,...".
    assert_invalid_at(
        &v,
        &with_file(with(
            file.clone(),
            "dataURL",
            json!("https://example.com/a.png"),
        )),
        "/files/file1/dataURL",
        "not a data URL",
    );
    assert_invalid_at(
        &v,
        &with_file(with(file, "created", json!("2023-11-14"))),
        "/files/file1/created",
        "created string",
    );
}

/// Every element upstream's restore returned, in
/// `tests/fixtures/restore-elements.json` (`restoreElements` on 117
/// scenes) and `tests/fixtures/restore-element.json` (`restoreElement`, 408
/// cases): the schema accepts it exactly when the typed codec reads it.
#[test]
fn upstream_restore_output_agrees_with_the_codec() {
    let v = def_validator(&excalidraw(), "Element");
    let mut outputs: Vec<(String, Value)> = Vec::new();
    let scenes = parse(&fixture("restore-elements.json"));
    for case in scenes["cases"].as_array().expect("cases") {
        let id = case["id"].as_str().expect("id");
        for (i, e) in case["result"].as_array().into_iter().flatten().enumerate() {
            outputs.push((format!("{id} element {i}"), e.clone()));
        }
    }
    let single = parse(&fixture("restore-element.json"));
    for case in single["cases"].as_array().expect("cases") {
        if case["result"].is_object() {
            let id = case["id"].as_str().expect("id");
            outputs.push((id.to_owned(), case["result"].clone()));
        }
    }
    let (mut valid, mut rejected) = (0, Vec::new());
    let mut disagreements = Vec::new();
    for (what, e) in &outputs {
        let map = e.as_object().unwrap_or_else(|| panic!("{what}: an object"));
        let errs = errors(&v, e);
        match Element::from_map(map.clone()) {
            Ok(_) if errs.is_empty() => valid += 1,
            Ok(_) => disagreements.push(format!("{what}: schema rejects: {errs:?}")),
            Err(_) if !errs.is_empty() => rejected.push(format!("{what}: {errs:?}")),
            Err(err) => {
                disagreements.push(format!("{what}: codec rejects ({err}), schema accepts"))
            }
        }
    }
    assert!(disagreements.is_empty(), "{disagreements:#?}");
    assert_eq!(valid + rejected.len(), outputs.len());
    // The other 60 are the fixtures' malformed inputs whose values
    // upstream's restore keeps as read (`version: "31"`, `id: 7`,
    // `textAlign: "justify"`, a binding with `mode: 1`, `x: "00"`, ...), which the
    // typed model has no form for; both reject those.
    assert_eq!(
        (valid, rejected.len()),
        (594, 60),
        "valid {valid}, rejected: {rejected:#?}"
    );
}

/// Whatever the schema accepts as an element, the typed codec reads: the
/// schema is never looser than `Element::from_map`.
#[test]
fn schema_is_never_looser_than_the_codec() {
    let v = def_validator(&excalidraw(), "Element");
    let mut candidates = every_type();
    let rect = element_of("rectangle");
    let text = element_of("text");
    let arrow = element_of("arrow");
    candidates.extend([
        with(rect.clone(), "fillStyle", json!("dots")),
        with(rect.clone(), "type", json!("draw")),
        with(rect.clone(), "roundness", json!({"type": 4})),
        with(rect.clone(), "roundness", json!({"type": 2.5})),
        with(
            rect.clone(),
            "boundElements",
            json!([{"id": "l", "type": "line"}]),
        ),
        with(rect.clone(), "strokeWidth", json!("3")),
        with(rect.clone(), "groupIds", json!("g1")),
        with(text.clone(), "fontFamily", json!("5")),
        with(text.clone(), "fontFamily", json!(-1)),
        with(text.clone(), "lineHeight", Value::Null),
        with(arrow.clone(), "points", json!([[0]])),
        with(arrow.clone(), "endArrowhead", json!("dot")),
        with(
            arrow.clone(),
            "endBinding",
            json!({"elementId": "r1", "focus": 0, "gap": 5}),
        ),
        with(arrow.clone(), "elbowed", Value::Null),
        with(rect.clone(), "customData", json!([1])),
        with(rect.clone(), "customData", Value::Null),
    ]);
    let mut rejected_by_both = 0;
    for e in &candidates {
        let schema_ok = errors(&v, e).is_empty();
        let codec_ok = Element::from_map(e.as_object().expect("object").clone()).is_ok();
        if schema_ok {
            assert!(codec_ok, "schema accepts what the codec rejects: {e}");
        } else if !codec_ok {
            rejected_by_both += 1;
        }
    }
    assert!(rejected_by_both >= 14, "{rejected_by_both}");
}

// -- .excalidrawlib -----------------------------------------------------------------

fn catalogue() -> Vec<(String, String)> {
    let f = parse(&fixture("library.json"));
    f["catalogue"]
        .as_array()
        .expect("catalogue")
        .iter()
        .map(|c| {
            (
                c["id"].as_str().expect("id").to_owned(),
                c["file"].as_str().expect("file").to_owned(),
            )
        })
        .collect()
}

/// All 232 catalogue libraries (70 of version 1, 162 of version 2), as
/// upstream writes them after `parseLibraryJSON` (the port's output is
/// upstream's byte for byte, `tests/library.rs`), are valid, but for the
/// value upstream's restore keeps against its own types: the 24 lines of
/// `aarondiel/logic-gates` keep `strokeWidth: "3"` (`restore.ts:459`,
/// ex-117), which the schema, as upstream's `strokeWidth: number`, rejects.
#[test]
fn catalogue_libraries_written_after_restore_are_valid() {
    let v = validator(&excalidrawlib());
    let cases = catalogue();
    assert_eq!(cases.len(), 232);
    let mut items = 0;
    for (id, file) in cases {
        let text = repo_file(&file);
        let restored =
            parse_library_json(&text, LibraryItemStatus::Published, &mut TestEnv::default())
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        items += restored.len();
        let written = parse(&serialize_library_as_json(
            &restored,
            "https://excalidraw.com",
        ));
        if id != "aarondiel/logic-gates" {
            assert_valid(&v, &written, &id);
            continue;
        }
        let errs = errors(&v, &written);
        assert_eq!(errs.len(), 24, "{errs:#?}");
        for err in errs {
            assert!(
                err.ends_with("/strokeWidth: \"3\" is not of type \"number\""),
                "{err}"
            );
        }
    }
    assert!(items > 3000, "{items} items");
}

/// Upstream's own v1 fixture, restored and written as v2.
#[test]
fn upstream_fixture_library_written_after_restore_is_valid() {
    let v = validator(&excalidrawlib());
    let text = repo_file(
        "fixtures/upstream/packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib",
    );
    let raw = parse(&text);
    assert_eq!(raw["version"], json!(1));
    let restored = parse_library_json(
        &text,
        LibraryItemStatus::Unpublished,
        &mut TestEnv::default(),
    )
    .expect("parses");
    assert!(!restored.is_empty());
    let written = parse(&serialize_library_as_json(
        &restored,
        "https://excalidraw.com",
    ));
    assert_valid(&v, &written, "fixture_library");
}

fn library_v2() -> Value {
    json!({
        "type": "excalidrawlib",
        "version": 2,
        "source": "https://excalidraw.com",
        "libraryItems": [{
            "id": "item1",
            "status": "published",
            "elements": [element_of("rectangle"), element_of("text")],
            "created": 1659863337886_u64,
            "name": "Box",
        }],
    })
}

/// `ExportedLibraryData` (`data/types.ts:52-57`) and the deprecated v1
/// `library` key (`data/types.ts:59-62`), as `isValidLibrary` checks them
/// (`data/json.ts:128-135`): `version` is the number 1 or 2.
#[test]
fn library_envelope_follows_upstream() {
    let v = validator(&excalidrawlib());
    let lib = library_v2();
    assert_valid(&v, &lib, "v2");
    assert_valid(&v, &without(lib.clone(), "source"), "v2 without source");
    let v1 = json!({
        "type": "excalidrawlib",
        "version": 1,
        "source": "https://excalidraw.com",
        "library": [[element_of("rectangle")], [element_of("line"), element_of("text")]],
    });
    assert_valid(&v, &v1, "v1");
    assert_invalid_at(
        &v,
        &with(v1.clone(), "library", json!([[{"type": "rectangle"}]])),
        "/library/0/0",
        "v1 element without fields",
    );
    assert_invalid_at(
        &v,
        &with(lib.clone(), "type", json!("excalidraw")),
        "/type",
        "type",
    );
    for version in [json!(3), json!(0), json!("2"), Value::Null] {
        assert_invalid_at(
            &v,
            &with(lib.clone(), "version", version.clone()),
            "/version",
            &version.to_string(),
        );
    }
    assert_invalid_at(&v, &without(lib.clone(), "type"), "", "without type");
    assert_invalid_at(&v, &without(lib.clone(), "version"), "", "without version");
    assert_invalid_at(
        &v,
        &without(lib.clone(), "libraryItems"),
        "",
        "neither libraryItems nor library",
    );
    assert_invalid_at(
        &v,
        &with(lib.clone(), "libraryItems", json!({})),
        "/libraryItems",
        "libraryItems object",
    );
    let mut futured = lib.clone();
    futured["future"] = json!(1);
    futured["libraryItems"][0]["future"] = json!(1);
    assert_valid(&v, &futured, "unknown keys");
}

/// `LibraryItem` (`types.ts:652-660`).
#[test]
fn library_items_follow_upstream() {
    let v = validator(&excalidrawlib());
    let lib = library_v2();
    let item = |f: &dyn Fn(Value) -> Value| {
        let mut l = lib.clone();
        l["libraryItems"][0] = f(l["libraryItems"][0].clone());
        l
    };
    for key in ["id", "status", "elements", "created"] {
        assert_invalid_at(
            &v,
            &item(&|i| without(i, key)),
            "/libraryItems/0",
            &format!("item without {key}"),
        );
    }
    for key in ["name", "error"] {
        assert_valid(&v, &item(&|i| without(i, key)), key);
    }
    assert_valid(&v, &item(&|i| with(i, "error", json!("failed"))), "error");
    assert_valid(
        &v,
        &item(&|i| with(i, "status", json!("unpublished"))),
        "unpublished",
    );
    assert_invalid_at(
        &v,
        &item(&|i| with(i, "status", json!("draft"))),
        "/libraryItems/0/status",
        "status draft",
    );
    assert_invalid_at(
        &v,
        &item(&|i| with(i, "created", json!("1659863337886"))),
        "/libraryItems/0/created",
        "created string",
    );
    assert_invalid_at(
        &v,
        &item(&|i| with(i, "id", json!(1))),
        "/libraryItems/0/id",
        "numeric id",
    );
    let mut bad = lib.clone();
    bad["libraryItems"][0]["elements"][1]["textAlign"] = json!("justify");
    assert_invalid_at(
        &v,
        &bad,
        "/libraryItems/0/elements/1/textAlign",
        "element in an item",
    );
}

/// Both schemas describe elements with the same definitions.
#[test]
fn both_schemas_share_the_element_definitions() {
    let scene = excalidraw();
    let lib = excalidrawlib();
    let shared: Map<String, Value> = scene["$defs"]
        .as_object()
        .expect("$defs")
        .iter()
        .filter(|(k, _)| lib["$defs"].get(k.as_str()).is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert!(shared.contains_key("Element"));
    for (name, def) in &shared {
        assert_eq!(&lib["$defs"][name], def, "{name}");
    }
}

/// Library items hold no deleted elements (`NonDeleted<ExcalidrawElement>`,
/// `types.ts:647-655`), in both versions.
#[test]
fn library_elements_are_not_deleted() {
    let v = validator(&excalidrawlib());
    let mut lib = library_v2();
    lib["libraryItems"][0]["elements"][0]["isDeleted"] = json!(true);
    assert_invalid_at(
        &v,
        &lib,
        "/libraryItems/0/elements/0/isDeleted",
        "deleted element in an item",
    );
    let v1 = json!({
        "type": "excalidrawlib",
        "version": 1,
        "library": [[with(element_of("rectangle"), "isDeleted", json!(true))]],
    });
    assert_invalid_at(&v, &v1, "/library/0/0/isDeleted", "deleted v1 element");
}

/// The typed views behind the `appState` and `files` definitions read what
/// the schema accepts and reject what it rejects.
#[test]
fn exported_app_state_and_files_are_typed_views() {
    use excali_core::app_state::{ExportedAppState, EXPORTED_KEYS};
    use excali_core::document::{BinaryFileData, FileMimeType};

    let scene = excalidraw();
    let keys: Vec<&str> = scene["$defs"]["ExportedAppState"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, EXPORTED_KEYS);

    let doc = parse(&fixture("every-type.excalidraw"));
    let state = doc["appState"].as_object().expect("appState");
    let typed = ExportedAppState::from_map(state).expect("reads");
    assert_eq!(typed.grid_size, Some(20.0));
    assert_eq!(typed.grid_step, Some(5.0));
    assert_eq!(typed.grid_mode_enabled, Some(false));
    assert_eq!(typed.view_background_color.as_deref(), Some("#ffffff"));
    assert_eq!(typed.locked_multi_selections, Some(Map::new()));
    // Keys other than the five are dropped, as cleanAppStateForExport does.
    let mut with_theme = state.clone();
    with_theme.insert("theme".into(), json!("dark"));
    assert_eq!(
        ExportedAppState::from_map(&with_theme).expect("reads"),
        typed
    );
    let app_state = def_validator(&scene, "ExportedAppState");
    for (key, value) in [
        ("gridSize", json!("20")),
        ("gridModeEnabled", json!(1)),
        ("lockedMultiSelections", json!({"g1": false})),
    ] {
        let mut bad = state.clone();
        bad.insert(key.into(), value.clone());
        assert!(ExportedAppState::from_map(&bad).is_err(), "{key} {value}");
        assert!(!app_state.is_valid(&Value::Object(bad)), "{key} {value}");
    }
    let mut locked = state.clone();
    locked.insert("lockedMultiSelections".into(), json!({"g1": true}));
    assert!(ExportedAppState::from_map(&locked).is_ok());

    let files = def_validator(&scene, "BinaryFileData");
    let file = &doc["files"]["file1"];
    let typed = BinaryFileData::from_value(file).expect("reads");
    assert_eq!(typed.mime_type, FileMimeType::Png);
    assert_eq!(typed.id.0, "file1");
    assert_eq!(typed.created, 1_700_000_000_000.0);
    assert_eq!(typed.last_retrieved, Some(1_700_000_000_000.0));
    assert_eq!(typed.version, None);
    let written = serde_json::to_value(&typed).expect("writes");
    assert_valid(&files, &written, "written file");
    let keys: Vec<&String> = written.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        file.as_object().expect("object").keys().collect::<Vec<_>>()
    );
    assert_eq!(BinaryFileData::from_value(&written).expect("reads"), typed);
    for bad in [
        with(file.clone(), "mimeType", json!("text/plain")),
        with(file.clone(), "dataURL", json!("https://example.com/a.png")),
        without(file.clone(), "created"),
    ] {
        assert!(BinaryFileData::from_value(&bad).is_err(), "{bad}");
        assert!(!files.is_valid(&bad), "{bad}");
    }
    // Every MIME type the schema allows is a FileMimeType.
    let mimes = scene["$defs"]["FileMimeType"]["enum"]
        .as_array()
        .expect("enum");
    assert_eq!(mimes.len(), 10);
    for mime in mimes {
        let f = with(file.clone(), "mimeType", mime.clone());
        assert!(BinaryFileData::from_value(&f).is_ok(), "{mime}");
    }
}
