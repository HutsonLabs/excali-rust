//! The `.excalidrawlib` library file (research page
//! `site/content/research/data-model.md`, section 4): [`LibraryItem`],
//! [`parse_library_json`], [`restore_library_items`],
//! [`serialize_library_as_json`], [`merge_library_items`] and
//! [`library_items_hash`].
//!
//! Upstream writes a library with `serializeLibraryAsJSON`
//! (`packages/excalidraw/data/json.ts:137-145`):
//! `{type: "excalidrawlib", version: 2, source, libraryItems}` through
//! `JSON.stringify(data, null, 2)`, saved as `.excalidrawlib` with MIME type
//! [`MIME_TYPE_EXCALIDRAWLIB`] (`json.ts:147-159`). Each item
//! (`LibraryItem`, `packages/excalidraw/types.ts:652-660`) is `{id, status,
//! elements, created, name?, error?}`. Version 1 files (`LibraryItem_v1`,
//! `types.ts:647-649`) hold the items under `library` instead, each a bare
//! array of elements.
//!
//! It reads one with `parseLibraryJSON` (`data/blob.ts:218-228`):
//! `JSON.parse`, the `isValidLibrary` check (`json.ts:128-135`), then
//! `restoreLibraryItems(data.libraryItems || data.library, defaultStatus)`
//! (`data/restore.ts:1374-1415`), which is where both versions become v2
//! items. [`parse_library_json`] does all of that, with upstream's
//! semantics for any input: the JS truthiness of `||`, iteration over
//! whatever the items value is (a string is iterable, `null` and objects are
//! not), and the `TypeError`s upstream throws out of the whole parse
//! ([`LibraryError`]).
//!
//! Each item's elements go through `restoreElements(elements, null)`
//! (`restore.ts:946-1012`) without options, so without its repair pass: each
//! element restored by [`crate::restore::restore_element`] (one that throws
//! or is not a restorable type is dropped, and so is a legacy `selection`),
//! a repeated id replaced by a fresh one, then `syncInvalidIndices`
//! ([`crate::fractional_index::sync_invalid_indices`]). Deleted elements are
//! then removed, and an item left without elements is dropped
//! (`restoreLibraryItem`, `restore.ts:1374-1379`).
//!
//! Each restored object is read with [`Element::from_restored`], which keeps
//! a field value of another JSON type than the model's and writes it back
//! as read until the field changes, as upstream keeps the object as it is
//! (`restore.ts:459` copies a truthy `strokeWidth` unchanged, so the 24
//! lines of the catalogue's `aarondiel/logic-gates` with `strokeWidth: "3"`
//! load, ex-117), and as [`LibraryItem`] keeps an odd `id` or `created`.
//!
//! One difference, where the typed model is stricter than upstream's
//! untyped objects: `arrayToMap`, the map arrows look their bound elements
//! up in, holds the object items of the elements array; upstream also keys
//! a string item under itself and other non-object items under
//! `undefined`, which only an arrow binding to such an "element" could see.
//!
//! [`LibraryItem`] is also a typed codec for items already restored (what
//! the persistence adapter stores, `library.ts:70`): [`LibraryItem::from_map`]
//! then [`LibraryItem::to_map`] gives the object back as read, unknown keys
//! included and in their order, and values the typed model has no form for
//! (a numeric `id`, a `status` other than the two) written as read until
//! the field is changed.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;

use crate::constants::{EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY, VERSION_EXCALIDRAW_LIBRARY};
use crate::element::Element;
use crate::fractional_index::{sync_invalid_indices, ChangeStamp};
use crate::js;
use crate::json;
use crate::layout::{Canonical, Layout};
use crate::order_key::OrderKeyError;
use crate::restore::{
    restore_element_encoded, ElementsMap, EscapingEnv, MapKey, RestoreEnv, RestoreOptions,
};

/// `MIME_TYPES.excalidrawlib` (`packages/common/src/constants.ts:316`): the
/// MIME type of a `.excalidrawlib` file, and of the library JSON dragged
/// onto the canvas (legacy form).
pub const MIME_TYPE_EXCALIDRAWLIB: &str = "application/vnd.excalidrawlib+json";
/// `MIME_TYPES.excalidrawlibIds` (`constants.ts:318`): the MIME type of
/// dragged library items, `{itemIds: string[]}` (`data/types.ts:64-66`).
pub const MIME_TYPE_EXCALIDRAWLIB_IDS: &str = "application/vnd.excalidrawlib.ids+json";
/// The file extension upstream saves a library with (`json.ts:154`).
pub const LIBRARY_FILE_EXTENSION: &str = "excalidrawlib";

/// `LibraryItem["status"]` (`types.ts:654`): published items came from
/// libraries.excalidraw.com (or were submitted there), unpublished ones were
/// made locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryItemStatus {
    Published,
    Unpublished,
}

impl LibraryItemStatus {
    /// The value in the file.
    pub const fn as_str(self) -> &'static str {
        match self {
            LibraryItemStatus::Published => "published",
            LibraryItemStatus::Unpublished => "unpublished",
        }
    }

    /// The status a value stands for: `"published"` is published, and
    /// anything else unpublished, as upstream's `status === "published"`
    /// checks read it.
    fn of(value: Option<&Value>) -> LibraryItemStatus {
        match value {
            Some(Value::String(s)) if s == "published" => LibraryItemStatus::Published,
            _ => LibraryItemStatus::Unpublished,
        }
    }
}

/// A library item (`LibraryItem`, `types.ts:652-660`).
#[derive(Debug, Clone)]
pub struct LibraryItem {
    /// `id`. A value that is not a string reads as its JS `String()` form
    /// and is written back as read while unchanged.
    pub id: String,
    /// `status`. A value other than the two reads as unpublished and is
    /// written back as read while unchanged.
    pub status: LibraryItemStatus,
    /// `elements`, none deleted after a restore.
    pub elements: Vec<Element>,
    /// `created`: epoch milliseconds. A value that is not a number reads as
    /// its `Number()` (NaN when that throws) and is written back as read
    /// while unchanged.
    pub created: f64,
    /// `name`. A truthy value that is not a string reads as its `String()`
    /// form; a falsy one other than `""` as `None`. Written back as read
    /// while unchanged.
    pub name: Option<String>,
    /// `error`, read like `name`.
    pub error: Option<String>,
    /// Keys the model does not know, as read.
    pub extra: Map<String, Value>,
    layout: Layout,
}

impl PartialEq for LibraryItem {
    fn eq(&self, other: &LibraryItem) -> bool {
        self.id == other.id
            && self.status == other.status
            && self.elements == other.elements
            && self.created.to_bits() == other.created.to_bits()
            && self.name == other.name
            && self.error == other.error
            && self.extra == other.extra
    }
}

impl schemars::JsonSchema for LibraryItemStatus {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LibraryItemStatus".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "`LibraryItem[\"status\"]` (packages/excalidraw/types.ts:654): published items came from libraries.excalidraw.com, unpublished ones were made locally.",
            "enum": [
                LibraryItemStatus::Published.as_str(),
                LibraryItemStatus::Unpublished.as_str(),
            ],
        })
    }
}

/// The elements of a library item: `readonly NonDeleted<ExcalidrawElement>[]`
/// (`packages/excalidraw/types.ts:647-655`).
pub(crate) fn library_elements_schema(
    generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    let element = generator.subschema_for::<Element>();
    schemars::json_schema!({
        "type": "array",
        "items": {
            "allOf": [
                element,
                { "properties": { "isDeleted": { "const": false } } },
            ],
        },
    })
}

impl schemars::JsonSchema for LibraryItem {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LibraryItem".into()
    }

    /// `LibraryItem` (`packages/excalidraw/types.ts:652-660`). Unknown keys
    /// are allowed and kept.
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let status = generator.subschema_for::<LibraryItemStatus>();
        let mut elements = library_elements_schema(generator);
        elements.insert(
            "description".into(),
            "The item's elements, none deleted.".into(),
        );
        schemars::json_schema!({
            "description": "A library item (`LibraryItem`, packages/excalidraw/types.ts:652-660).",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "status": status,
                "elements": elements,
                "created": {
                    "description": "Epoch milliseconds.",
                    "type": "number",
                },
                "name": { "type": "string" },
                "error": { "type": "string" },
            },
            "required": REQUIRED,
        })
    }
}

/// `LibraryItem`'s key order (`types.ts:652-660`), which is also the order
/// `actionAddToLibrary` builds a new item in (`actionAddToLibrary.ts:36-41`).
const KEYS: &[&str] = &["id", "status", "elements", "created", "name", "error"];
const CANONICAL: Canonical<'static> = &[KEYS];
/// The keys [`LibraryItem::from_map`] requires.
const REQUIRED: &[&str] = &["id", "status", "elements", "created"];

fn invalid(message: &str) -> json::Error {
    serde::de::Error::custom(format!("not a valid library item: {message}"))
}

/// The typed reading of `name` and `error`, from a sentinel-form value.
fn optional_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(json::decode_str(s).into_owned()),
        v if js::truthy(v) => js::to_string(v)
            .ok()
            .map(|s| json::decode_str(&s).into_owned()),
        _ => None,
    }
}

impl LibraryItem {
    /// An item as upstream's "add to library" builds one
    /// (`actionAddToLibrary.ts:36-41`): `{id, status, elements, created}`.
    /// Upstream draws `id` from `randomId()` and `created` from
    /// `Date.now()`; the caller supplies them here.
    pub fn new(
        id: impl Into<String>,
        status: LibraryItemStatus,
        elements: Vec<Element>,
        created: f64,
    ) -> LibraryItem {
        LibraryItem {
            id: id.into(),
            status,
            elements,
            created,
            name: None,
            error: None,
            extra: Map::new(),
            layout: Layout::default(),
        }
    }

    /// Read an item that is already restored: an object with `id`,
    /// `status`, `created` and an `elements` array whose every item
    /// [`Element::from_map`] reads. Nothing is restored or migrated (see
    /// [`restore_library_items`] for items from a file).
    pub fn from_map(raw: Map<String, Value>) -> Result<LibraryItem, json::Error> {
        LibraryItem::from_encoded(json::escape_map(&raw))
    }

    /// [`LibraryItem::from_map`] for an object in the sentinel form.
    fn from_encoded(mut raw: Map<String, Value>) -> Result<LibraryItem, json::Error> {
        if let Some(key) = REQUIRED.iter().find(|k| !raw.contains_key(**k)) {
            return Err(invalid(&format!("missing field `{key}`")));
        }
        let elements = match raw.get_mut("elements") {
            Some(Value::Array(items)) => std::mem::take(items)
                .into_iter()
                .map(|item| match item {
                    Value::Object(map) => Element::from_encoded(&map),
                    _ => Err(invalid("an element must be an object")),
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(invalid("elements must be an array")),
        };
        Ok(LibraryItem::read(&raw, elements))
    }

    /// The item for a sentinel-form object whose `elements` value is
    /// already read (and holds `[]`, its shape for the layout).
    fn read(raw: &Map<String, Value>, elements: Vec<Element>) -> LibraryItem {
        let id = match raw.get("id") {
            Some(Value::String(s)) => json::decode_str(s).into_owned(),
            v => js::to_string(v)
                .map(|s| json::decode_str(&s).into_owned())
                .unwrap_or_default(),
        };
        let created = match raw.get("created") {
            Some(v) => js::to_number(Some(v)).unwrap_or(f64::NAN),
            None => f64::NAN,
        };
        let mut item = LibraryItem {
            id,
            status: LibraryItemStatus::of(raw.get("status")),
            elements,
            created,
            name: optional_string(raw.get("name")),
            error: optional_string(raw.get("error")),
            extra: Map::new(),
            layout: Layout::default(),
        };
        let typed = item.typed(true);
        let (layout, extra) = Layout::read(raw, &typed, CANONICAL);
        item.layout = layout;
        item.extra = extra;
        item
    }

    /// The JSON object serde writes for this item, keys in JS property
    /// order. A lone surrogate read from a file is U+FFFD here; only
    /// [`serialize_library_as_json`] writes it back as its escape.
    pub fn to_map(&self) -> Map<String, Value> {
        json::ordered_like_js(json::decode_map(&self.to_encoded()))
    }

    /// The object to write, in the sentinel form, before JS key ordering.
    fn to_encoded(&self) -> Map<String, Value> {
        self.layout
            .write(&self.typed(false), &self.extra, CANONICAL)
    }

    /// The keys and values the typed model writes, in the sentinel form;
    /// with `shaped`, `elements` reduced to `[]`, as the layout reads it.
    fn typed(&self, shaped: bool) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert(
            "id".into(),
            Value::String(json::escape_str(&self.id).into_owned()),
        );
        map.insert("status".into(), Value::from(self.status.as_str()));
        let elements = if shaped {
            Vec::new()
        } else {
            self.elements
                .iter()
                .map(|e| Value::Object(e.to_encoded()))
                .collect()
        };
        map.insert("elements".into(), Value::Array(elements));
        map.insert("created".into(), js::number(self.created));
        for (key, value) in [("name", &self.name), ("error", &self.error)] {
            if let Some(value) = value {
                map.insert(
                    key.into(),
                    Value::String(json::escape_str(value).into_owned()),
                );
            }
        }
        map
    }

    /// `getLibraryItemHash` (`library.ts:594-596`),
    /// `` `${item.id}:${item.name || ""}:${hashElementsVersion(item.elements)}` ``,
    /// as UTF-16 code units. `id` and `name` are the values written (so a
    /// value kept as read is hashed as upstream sees it).
    fn hash_key(&self) -> Vec<u16> {
        let encoded = self.to_encoded();
        let text = |value: Option<&Value>| {
            // A template literal is `ToString`; an object with its own
            // `toString` key would throw upstream and is hashed as "".
            js::to_string(value).unwrap_or_default()
        };
        let id = text(encoded.get("id"));
        let name = if js::truthy(encoded.get("name")) {
            text(encoded.get("name"))
        } else {
            String::new()
        };
        let mut key = json::to_utf16(&id);
        key.push(u16::from(b':'));
        key.extend(json::to_utf16(&name));
        key.push(u16::from(b':'));
        key.extend(
            hash_elements_version(&self.elements)
                .to_string()
                .encode_utf16(),
        );
        key
    }
}

impl Serialize for LibraryItem {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(s)
    }
}

impl<'de> Deserialize<'de> for LibraryItem {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<LibraryItem, D::Error> {
        let raw = Map::<String, Value>::deserialize(d)?;
        LibraryItem::from_map(raw).map_err(serde::de::Error::custom)
    }
}

/// What reading a library throws upstream. The messages are V8's.
#[derive(Debug)]
pub enum LibraryError {
    /// `JSON.parse` failed (a `SyntaxError`; the message is serde_json's).
    Json(json::Error),
    /// `isValidLibrary` is false: not an object with `type`
    /// `"excalidrawlib"` and `version` 1 or 2 (`blob.ts:223-225`).
    InvalidLibrary,
    /// `data.libraryItems || data.library` is `null`, `false`, `0`, a
    /// number, `true` or an object: `for...of` throws.
    NotIterable,
    /// An item is `null`: `_item.id` throws.
    ItemNull,
    /// An item's `elements` is truthy and not an array: `arrayToMap` calls
    /// `.reduce` on it.
    ElementsNotArray,
    /// An element is `null`: `arrayToMap` reads its `id`.
    ElementNull,
    /// `syncInvalidIndices` could not generate indices.
    Index(OrderKeyError),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Json(e) => write!(f, "{e}"),
            LibraryError::InvalidLibrary => f.write_str("Invalid library"),
            LibraryError::NotIterable => f.write_str("libraryItems is not iterable"),
            LibraryError::ItemNull | LibraryError::ElementNull => {
                f.write_str("Cannot read properties of null (reading 'id')")
            }
            LibraryError::ElementsNotArray => f.write_str("items.reduce is not a function"),
            LibraryError::Index(e) => f.write_str(e.message()),
        }
    }
}

impl std::error::Error for LibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LibraryError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<json::Error> for LibraryError {
    fn from(e: json::Error) -> LibraryError {
        LibraryError::Json(e)
    }
}

/// `isValidLibrary(json)` (`json.ts:128-135`): an object whose `type` is
/// `"excalidrawlib"` and whose `version` is the number 1 or 2.
pub fn is_valid_library(value: &Value) -> bool {
    let Value::Object(map) = value else {
        return false;
    };
    map.get("type").and_then(Value::as_str) == Some(EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY)
        && matches!(
            map.get("version").and_then(Value::as_f64),
            Some(v) if v == 1.0 || v == 2.0
        )
}

/// `parseLibraryJSON(json, defaultStatus)` (`blob.ts:218-228`): parse the
/// text as `JSON.parse` would, check it with [`is_valid_library`], and
/// restore `libraryItems || library` with [`restore_library_items`].
/// Upstream's default status is unpublished; an import from
/// libraries.excalidraw.com passes published (`library.ts:754-760`).
///
/// `env` supplies ids, timestamps (also `Date.now()` for `created`) and
/// the `versionNonce`s of index updates, as in restore.
pub fn parse_library_json(
    text: &str,
    default_status: LibraryItemStatus,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<LibraryItem>, LibraryError> {
    let data = json::parse(text)?;
    if !is_valid_library(&data) {
        return Err(LibraryError::InvalidLibrary);
    }
    let items = js::or(data.get("libraryItems"), || data.get("library").cloned());
    restore_items_encoded(items.as_ref(), default_status, &mut EscapingEnv(env))
}

/// `restoreLibraryItems(libraryItems, defaultStatus)`
/// (`restore.ts:1381-1415`) on a parsed value; `None` is `undefined`,
/// which the default parameter makes `[]`.
///
/// - a v1 item (an array) becomes `{status: defaultStatus, elements: item,
///   id: randomId(), created: Date.now()}`;
/// - any other item is `{...item, id: item.id || randomId(), status:
///   item.status || defaultStatus, created: item.created || Date.now()}`;
///   keys keep their place, unknown ones included;
/// - its elements are restored as the module docs describe, and the item
///   gets them (`elements` keeping its place, else last) unless none is
///   left, when it is dropped.
///
/// A string is iterated by code point (each one an item that is dropped);
/// `null` and other values are not iterable ([`LibraryError::NotIterable`]).
pub fn restore_library_items(
    items: Option<&Value>,
    default_status: LibraryItemStatus,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<LibraryItem>, LibraryError> {
    let items = items.map(json::escape);
    restore_items_encoded(items.as_ref(), default_status, &mut EscapingEnv(env))
}

/// [`restore_library_items`] on sentinel-form values.
fn restore_items_encoded(
    items: Option<&Value>,
    default_status: LibraryItemStatus,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<LibraryItem>, LibraryError> {
    let items: Vec<Value> = match items {
        None => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        // `for...of` over a string yields its code points.
        Some(Value::String(s)) => json::decode_str(s)
            .chars()
            .map(|c| Value::String(c.to_string()))
            .collect(),
        Some(_) => return Err(LibraryError::NotIterable),
    };
    let status = Value::from(default_status.as_str());
    let mut restored = Vec::new();
    for item in items {
        let object = match item {
            Value::Array(elements) => {
                let mut object = Map::new();
                object.insert("status".into(), status.clone());
                object.insert("elements".into(), Value::Array(elements));
                object.insert("id".into(), Value::String(env.random_id()));
                object.insert("created".into(), js::number(env.now()));
                object
            }
            Value::Null => return Err(LibraryError::ItemNull),
            item => {
                // `{..._item}`: an object's own keys; a primitive has no
                // `elements`, so the item is dropped below either way.
                let mut object = match item {
                    Value::Object(map) => map,
                    _ => Map::new(),
                };
                let id = js::or(object.get("id"), || Some(Value::String(env.random_id())));
                let st = js::or(object.get("status"), || Some(status.clone()));
                let created = js::or(object.get("created"), || Some(js::number(env.now())));
                for (key, value) in [("id", id), ("status", st), ("created", created)] {
                    object.insert(key.into(), value.unwrap_or(Value::Null));
                }
                object
            }
        };
        if let Some(item) = restore_item(object, env)? {
            restored.push(item);
        }
    }
    Ok(restored)
}

/// `restoreLibraryItem` (`restore.ts:1374-1379`).
fn restore_item(
    mut object: Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Option<LibraryItem>, LibraryError> {
    let elements: Vec<Element> = restore_elements(object.get("elements"), env)?
        .into_iter()
        .filter(|e| !e.base.is_deleted)
        .collect();
    if elements.is_empty() {
        return Ok(None);
    }
    // `{...libraryItem, elements}`; the layout sees the array's shape.
    object.insert("elements".into(), Value::Array(Vec::new()));
    Ok(Some(LibraryItem::read(&object, elements)))
}

/// `restoreElements(elements, null)` (`restore.ts:946-1012`) without
/// options, on sentinel-form values, with the typed model's reading of each
/// restored element ([`Element::from_restored`]; see the module docs).
fn restore_elements(
    elements: Option<&Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Element>, LibraryError> {
    let elements: &[Value] = match elements {
        v if !js::truthy(v) => &[],
        Some(Value::Array(items)) => items,
        _ => return Err(LibraryError::ElementsNotArray),
    };
    // arrayToMap(targetElements): `element.id` of every item.
    if elements.iter().any(Value::is_null) {
        return Err(LibraryError::ElementNull);
    }
    let targets = ElementsMap::from_encoded(
        elements
            .iter()
            .filter_map(|e| e.as_object().cloned())
            .collect(),
    );
    let mut existing_ids: HashSet<MapKey> = HashSet::new();
    let mut restored = Vec::with_capacity(elements.len());
    for element in elements {
        // `restoreElement({...element})` of a primitive or an array has no
        // `type` and gives null.
        let Value::Object(element) = element else {
            continue;
        };
        // Legacy selection elements are filtered out.
        if element.get("type").and_then(Value::as_str) == Some("selection") {
            continue;
        }
        // A throw is caught and the element dropped (`restore.ts:977-979`).
        let Ok(Some(mut migrated)) =
            restore_element_encoded(element, &targets, None, RestoreOptions::default(), env)
        else {
            continue;
        };
        let key = MapKey::of(migrated.get("id"));
        if key.as_ref().is_some_and(|k| existing_ids.contains(k)) {
            migrated.insert("id".into(), Value::String(env.random_id()));
        }
        // An object or array id is its own key: never seen again.
        if let Some(key) = MapKey::of(migrated.get("id")) {
            existing_ids.insert(key);
        }
        restored.push(migrated);
    }
    let mut typed: Vec<Element> = restored
        .iter()
        .filter_map(|m| Element::from_restored_encoded(m).ok())
        .collect();
    sync_invalid_indices(&mut typed, &mut EnvStamp(env)).map_err(LibraryError::Index)?;
    Ok(typed)
}

/// `mutateElement`'s stamp during restore: `randomInteger()` and
/// `getUpdatedTimestamp()` from the restore environment.
struct EnvStamp<'a>(&'a mut dyn RestoreEnv);

impl ChangeStamp for EnvStamp<'_> {
    fn version_nonce(&mut self) -> f64 {
        self.0.random_integer()
    }

    fn updated(&mut self) -> f64 {
        self.0.now()
    }
}

/// `serializeLibraryAsJSON(libraryItems)` (`json.ts:137-145`) with
/// `getExportSource()` as `source` (`window.EXCALIDRAW_EXPORT_SOURCE ||
/// window.location.origin` upstream): the v2 envelope written as
/// `JSON.stringify(data, null, 2)` writes it.
pub fn serialize_library_as_json(items: &[LibraryItem], source: &str) -> String {
    let mut data = Map::new();
    data.insert(
        "type".into(),
        Value::from(EXPORT_DATA_TYPE_EXCALIDRAW_LIBRARY),
    );
    data.insert("version".into(), js::number(VERSION_EXCALIDRAW_LIBRARY));
    data.insert(
        "source".into(),
        Value::String(json::escape_str(source).into_owned()),
    );
    data.insert(
        "libraryItems".into(),
        Value::Array(
            items
                .iter()
                .map(|item| Value::Object(item.to_encoded()))
                .collect(),
        ),
    );
    json::write_parsed(&Value::Object(data))
}

/// `isUniqueItem` (`library.ts:122-141`): no item of `existing` has the
/// same number of elements with the same `id` and `versionNonce`
/// (`===`), in the same order.
fn is_unique_item(existing: &[LibraryItem], target: &LibraryItem) -> bool {
    !existing.iter().any(|item| {
        item.elements.len() == target.elements.len()
            && item.elements.iter().zip(&target.elements).all(|(a, b)| {
                a.base.id == b.base.id && a.base.version_nonce == b.base.version_nonce
            })
    })
}

/// `mergeLibraryItems(localItems, otherItems)` (`library.ts:145-157`): the
/// items of `other` not already in `local` (see `isUniqueItem`: same
/// element ids and `versionNonce`s in order, whatever the item's own id),
/// in their order, then `local`.
pub fn merge_library_items(local: &[LibraryItem], other: &[LibraryItem]) -> Vec<LibraryItem> {
    other
        .iter()
        .filter(|item| is_unique_item(local, item))
        .chain(local)
        .cloned()
        .collect()
}

/// `hashString(s)` (`packages/element/src/index.ts:34-41`): djb2 over the
/// UTF-16 code units, `(hash << 5) + hash + unit` in JS arithmetic (the
/// shift on `ToInt32(hash)`, the sums in doubles), as `hash >>> 0`.
pub fn hash_string(s: &str) -> u32 {
    djb2(s.encode_utf16().map(f64::from))
}

/// `hashElementsVersion(elements)` (`packages/element/src/index.ts:21-27`):
/// djb2 over the elements' `versionNonce`s, in order.
pub fn hash_elements_version(elements: &[Element]) -> u32 {
    djb2(elements.iter().map(|e| e.base.version_nonce))
}

fn djb2(values: impl Iterator<Item = f64>) -> u32 {
    let mut hash = 5381.0_f64;
    for value in values {
        hash = f64::from(js::to_int32(hash).wrapping_shl(5)) + hash + value;
    }
    js::to_uint32(hash)
}

/// `getLibraryItemsHash(items)` (`library.ts:598-605`): `hashString` of
/// every item's `id:name:hashElementsVersion`, sorted by UTF-16 code units
/// (`Array.prototype.sort`'s default) and joined with `,`. Upstream uses it
/// to tell whether the library changed since it was last saved.
pub fn library_items_hash(items: &[LibraryItem]) -> u32 {
    let mut keys: Vec<Vec<u16>> = items.iter().map(LibraryItem::hash_key).collect();
    keys.sort();
    let joined = keys.join(&u16::from(b','));
    djb2(joined.into_iter().map(f64::from))
}
