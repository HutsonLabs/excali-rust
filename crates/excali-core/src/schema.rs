//! JSON Schemas (draft 2020-12) for `.excalidraw` and `.excalidrawlib`,
//! generated from the model's types with `schemars`.
//!
//! Upstream has no JSON Schema (`site/content/research/data-model.md`,
//! section 9): its TypeScript types are the format's only description. The
//! schemas here are new documentation built from the Rust types that mirror
//! them, published on the site under `schema/` (`site/static/schema/`,
//! linked from `site/content/architecture/file-format.md`) and regenerated
//! with `cargo run -p excali-core --example write-schemas`; a test fails
//! when the published files and the model disagree.
//!
//! What they describe is a file as upstream and the port write it:
//!
//! - a key that is non-optional in upstream's type is required, whether or
//!   not it may be `null` (`roundness`, `index`, `frameId`, ...); an
//!   optional one (`customData?`, `labelPosition?`) is optional. `polygon`,
//!   `elbowed` and an image's `fileId` are optional too: restore copies
//!   them only when the element it read had them (`restore.ts:605-611,
//!   645-650, 697`), so upstream writes such elements without them;
//! - enumerations are upstream's current values; legacy ones (`draw`,
//!   `dot`, `strokeSharpness`, bindings without `mode`) are migrated by
//!   restore and are not described;
//! - keys the model does not know are allowed everywhere, since the port
//!   keeps them and writes them back.
//!
//! So a file that validates is one the typed codec reads
//! ([`crate::document::Document::from_json`],
//! [`crate::library::LibraryItem::from_map`]); files from older writers
//! may not validate and are still read through restore.

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

use crate::constants::EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY;
use crate::document::Document;
use crate::library::{library_elements_schema, LibraryItem};

/// Where the site serves the schemas: `base_url` of `site/config.toml`
/// plus `/schema/`. Each schema's `$id` is this plus its file name.
pub const BASE_URL: &str = "https://hutsonlabs.github.io/excali-rust/schema/";

/// The meta-schema of both schemas.
pub const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

/// File name of the `.excalidraw` schema under `site/static/schema/`.
pub const EXCALIDRAW_FILE: &str = "excalidraw.schema.json";

/// File name of the `.excalidrawlib` schema under `site/static/schema/`.
pub const EXCALIDRAWLIB_FILE: &str = "excalidrawlib.schema.json";

/// A published schema: its file name and how to generate it.
#[derive(Debug, Clone, Copy)]
pub struct Published {
    pub file_name: &'static str,
    pub schema: fn() -> Value,
}

/// Every published schema, in the order the file-format page lists them.
pub fn published() -> [Published; 2] {
    [
        Published {
            file_name: EXCALIDRAW_FILE,
            schema: excalidraw,
        },
        Published {
            file_name: EXCALIDRAWLIB_FILE,
            schema: excalidrawlib,
        },
    ]
}

fn generator() -> SchemaGenerator {
    SchemaSettings::draft2020_12().into_generator()
}

/// A root schema: `$schema`, `$id`, `title` and `description` first, then
/// the body's keywords, then the definitions it refers to.
fn root(
    file_name: &str,
    title: &str,
    description: &str,
    body: Schema,
    mut generator: SchemaGenerator,
) -> Value {
    let mut map = Map::new();
    map.insert("$schema".into(), json!(DRAFT_2020_12));
    map.insert("$id".into(), json!(format!("{BASE_URL}{file_name}")));
    map.insert("title".into(), json!(title));
    map.insert("description".into(), json!(description));
    if let Value::Object(body) = body.to_value() {
        for (key, value) in body {
            if !map.contains_key(&key) {
                map.insert(key, value);
            }
        }
    }
    map.insert(
        "$defs".into(),
        Value::Object(generator.take_definitions(true)),
    );
    let mut schema = Value::Object(map);
    tidy_descriptions(&mut schema);
    schema
}

/// Descriptions come from doc comments, wrapped at 80 columns: join the
/// wrapped lines of each paragraph.
fn tidy_descriptions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match v {
                    Value::String(text) if key == "description" => {
                        *text = unwrap_paragraphs(text);
                    }
                    _ => tidy_descriptions(v),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(tidy_descriptions),
        _ => {}
    }
}

fn unwrap_paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|p| p.split('\n').map(str::trim).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The schema of a `.excalidraw` file (`ExportedDataState`,
/// `packages/excalidraw/data/types.ts:14-21`).
pub fn excalidraw() -> Value {
    let mut generator = generator();
    let body = Document::json_schema(&mut generator);
    root(
        EXCALIDRAW_FILE,
        "Excalidraw scene (.excalidraw)",
        "A scene as Excalidraw's serializeAsJSON writes it (packages/excalidraw/data/json.ts:52-75) and as excali-rust reads and writes it. Generated from the excali-core model; upstream publishes no schema. Keys not listed are allowed and kept. Files from older writers may use legacy fields that restore migrates; those are not described here.",
        body,
        generator,
    )
}

/// The schema of a `.excalidrawlib` file (`ExportedLibraryData` and the
/// deprecated v1 `library` of `ImportedLibraryData`,
/// `packages/excalidraw/data/types.ts:52-62`), as `isValidLibrary` accepts
/// it (`data/json.ts:128-135`).
pub fn excalidrawlib() -> Value {
    let mut generator = generator();
    let item = generator.subschema_for::<LibraryItem>();
    let v1_item = library_elements_schema(&mut generator);
    let body = schemars::json_schema!({
        "type": "object",
        "properties": {
            "type": {
                "description": "`EXPORT_DATA_TYPES.excalidrawLibrary` (packages/common/src/constants.ts:347).",
                "const": EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY,
            },
            "version": {
                "description": "1 or 2 (`isValidLibrary`, packages/excalidraw/data/json.ts:128-135); `VERSIONS.excalidrawLibrary` (2) when written.",
                "enum": [1, 2],
            },
            "source": {
                "description": "The writer's origin.",
                "type": "string",
            },
            "libraryItems": {
                "description": "The items (version 2).",
                "type": "array",
                "items": item,
            },
            "library": {
                "description": "Deprecated version 1 items (`LibraryItem_v1`, packages/excalidraw/types.ts:647-649): each a bare array of elements. Read as `libraryItems || library`; restore gives each an id, the default status and `created`.",
                "type": "array",
                "items": v1_item,
            },
        },
        "required": ["type", "version"],
        "anyOf": [
            { "required": ["libraryItems"] },
            { "required": ["library"] },
        ],
    });
    root(
        EXCALIDRAWLIB_FILE,
        "Excalidraw library (.excalidrawlib)",
        "A library as Excalidraw's serializeLibraryAsJSON writes it (packages/excalidraw/data/json.ts:137-145, version 2, key libraryItems), or a version 1 file (key library), and as excali-rust reads and writes it. Generated from the excali-core model; upstream publishes no schema. Keys not listed are allowed and kept.",
        body,
        generator,
    )
}

/// A schema as published: two-space indent, as `JSON.stringify(v, null, 2)`
/// writes it, and a final newline.
pub fn to_json(schema: &Value) -> String {
    let mut text = serde_json::to_string_pretty(schema).unwrap_or_default();
    text.push('\n');
    text
}
