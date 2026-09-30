//! The `.excalidraw` file: [`Document`].
//!
//! Upstream writes a scene with `serializeAsJSON`
//! (`packages/excalidraw/data/json.ts:52-75`): the object
//! `{type, version, source, elements, appState, files}` passed to
//! `JSON.stringify(data, null, 2)`, where `files` is `undefined` (so
//! omitted) for a database save. It reads one with `JSON.parse` into
//! `ImportedDataState` (`data/types.ts:35-50`), where every key is optional,
//! and accepts it when `isValidExcalidrawData` (`json.ts:115-126`) holds:
//!
//! ```js
//! data?.type === "excalidraw" &&
//!   (!data.elements ||
//!     (Array.isArray(data.elements) &&
//!       (!data.appState || typeof data.appState === "object")))
//! ```
//!
//! [`Document::from_json`] accepts exactly that. So `elements` may be any
//! falsy value (`null`, `false`, `0`, `""`), and then `appState` may be
//! anything; with an `elements` array, `appState` may be falsy, an object,
//! an array or `null`. `version`, `source` and `files` are never checked.
//! Where the file holds a value the typed model has no form for (a falsy
//! `elements`, `appState: []`, `version: "2"`, `source: 123`, `files: 0`)
//! the field is `None` and the value is written back as read until the
//! field is set.
//!
//! Stricter than upstream's check, as a typed codec must be: every item of
//! `elements` must be an object that [`Element::from_map`] can model (a
//! known `type` and the fields that type requires). Upstream would pass
//! other items on to `restoreElements`, which drops or repairs them; that
//! is the restore module's job.
//!
//! [`Document::from_json`] then [`Document::to_json`] gives what
//! `JSON.stringify(JSON.parse(text), null, 2)` gives: keys unknown to the
//! model (top level, in elements, in `appState` and `files`) are kept where
//! they were, known keys keep their order, and a value the typed model
//! would normalise is written back as read until it changes (see
//! [`Element`]). A lone UTF-16 surrogate in a string (`"\ud83d"`, half of a
//! split emoji) reads as U+FFFD in the model and is written back as its
//! escape while the value is unchanged (see [`crate::json`]). A
//! [`Document`] built in Rust is written in `serializeAsJSON`'s key order.
//!
//! `appState` and `files` are kept as JSON objects here, each with its own
//! layout, so the escape is kept per key: changing `appState.b` leaves a
//! lone surrogate in an untouched `appState.a` as read, as upstream's
//! in-place edit of the parsed object does. A changed key is written from
//! the model as a whole.
//!
//! Every object is written in JS property order, as `JSON.stringify` writes
//! any object: array-index keys (`"0"` to `"4294967294"`, e.g. a file id
//! `"42"` or an unknown key `"7"`) first in ascending order, then the other
//! keys in the order described above. [`Document::to_map`] gives the same
//! order.
//!
//! Opening and saving a file as upstream does is [`load_scene_json`]
//! (`loadFromBlob`: parse, check, restore) and
//! [`LoadedScene::to_document`] (`serializeAsJSON(..., "local")`). The
//! loader reads any scene upstream reads, since restore migrates legacy
//! elements before they are typed, and the restored elements are read with
//! [`Element::from_restored`], which keeps a field value of another JSON
//! type (`strokeWidth: "3"`, as `restore.ts:459` keeps it) and writes it
//! back as read.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use std::borrow::Cow;
use std::fmt;

use crate::app_state::{
    clean_app_state_for_export, restore_app_state, AppState, AppStateEnv, ExportedAppState,
    TypeError,
};
use crate::constants::{EXPORT_DATA_TYPE_EXCALIDRAW, VERSION_EXCALIDRAW};
use crate::element::{Element, FileId};
use crate::js::{self, truthy};
use crate::json::{self, Error};
use crate::layout::{Canonical, Layout};
use crate::restore::{
    restore_elements_sentinel_owned, RestoreElementsError, RestoreElementsOptions, RestoreEnv,
};

/// A `.excalidraw` file.
#[derive(Debug, Clone)]
pub struct Document {
    /// `version`: 2 when upstream writes it.
    pub version: Option<f64>,
    /// `source`: the writer's origin.
    pub source: Option<String>,
    /// `elements`, deleted ones included. `None` when absent or `null`.
    pub elements: Option<Vec<Element>>,
    /// `appState`: upstream writes only the keys exported by
    /// `cleanAppStateForExport` (`appState.ts:321-323`). `None` when absent
    /// or `null`.
    pub app_state: Option<Map<String, Value>>,
    /// `files` (`BinaryFiles`, `types.ts:146`): file id to file data.
    /// `None` when absent.
    pub files: Option<Map<String, Value>>,
    /// Top-level keys the model does not know, as read.
    pub extra: Map<String, Value>,
    layout: Layout,
    /// Layouts of the `appState` and `files` objects as read.
    app_state_layout: Layout,
    files_layout: Layout,
}

impl PartialEq for Document {
    fn eq(&self, other: &Document) -> bool {
        self.version == other.version
            && self.source == other.source
            && self.elements == other.elements
            && self.app_state == other.app_state
            && self.files == other.files
            && self.extra == other.extra
    }
}

/// `serializeAsJSON`'s key order, `json.ts:58-72`.
const KEYS: &[&str] = &["type", "version", "source", "elements", "appState", "files"];
const CANONICAL: Canonical<'static> = &[KEYS];

fn invalid(message: &str) -> Error {
    serde::de::Error::custom(format!("not a valid Excalidraw scene: {message}"))
}

/// `!value` in JS for a JSON value (`undefined` when absent). JSON has no
/// `NaN`, so the falsy values are `null`, `false`, `0`, `-0` and `""`.
fn is_falsy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => false,
    }
}

/// `isValidExcalidrawData` (`json.ts:115-126`).
fn validate(raw: &Map<String, Value>) -> Result<(), Error> {
    if raw.get("type").and_then(Value::as_str) != Some(EXPORT_DATA_TYPE_EXCALIDRAW) {
        return Err(invalid("type must be \"excalidraw\""));
    }
    let elements = raw.get("elements");
    if is_falsy(elements) {
        return Ok(());
    }
    if !matches!(elements, Some(Value::Array(_))) {
        return Err(invalid("elements must be an array"));
    }
    // typeof x === "object" for objects, arrays and null.
    let app_state = raw.get("appState");
    if is_falsy(app_state) || matches!(app_state, Some(Value::Object(_) | Value::Array(_))) {
        return Ok(());
    }
    Err(invalid("appState must be an object"))
}

impl Document {
    /// A scene as `serializeAsJSON(elements, appState, files, "local")`
    /// writes it: `type` `"excalidraw"`, `version` 2, the given `source`.
    /// `files: None` omits the key, as a `"database"` save does.
    pub fn new(
        source: impl Into<String>,
        elements: Vec<Element>,
        app_state: Map<String, Value>,
        files: Option<Map<String, Value>>,
    ) -> Document {
        Document {
            version: Some(VERSION_EXCALIDRAW),
            source: Some(source.into()),
            elements: Some(elements),
            app_state: Some(app_state),
            files,
            extra: Map::new(),
            layout: Layout::default(),
            app_state_layout: Layout::default(),
            files_layout: Layout::default(),
        }
    }

    /// Parse a `.excalidraw` file as `JSON.parse` would, including lone
    /// UTF-16 surrogates in strings, and check it as
    /// `isValidExcalidrawData` does (see the module docs).
    pub fn from_json(text: &str) -> Result<Document, Error> {
        match json::parse(text)? {
            Value::Object(map) => Document::from_encoded(map),
            _ => Err(invalid("not an object")),
        }
    }

    /// Write the file as `JSON.stringify(data, null, 2)` would: two-space
    /// indent, no trailing newline.
    pub fn to_json(&self) -> String {
        json::write_parsed(&Value::Object(self.to_encoded()))
    }

    /// Read a document from a parsed JSON object.
    pub fn from_map(raw: Map<String, Value>) -> Result<Document, Error> {
        Document::from_encoded(json::escape_map(&raw))
    }

    /// [`Document::from_map`] for an object in the sentinel form of
    /// [`crate::json`].
    fn from_encoded(mut raw: Map<String, Value>) -> Result<Document, Error> {
        validate(&raw)?;
        let version = raw.get("version").and_then(Value::as_f64);
        let source = raw
            .get("source")
            .and_then(Value::as_str)
            .map(|s| json::decode_str(s).into_owned());
        // The items are read (and laid out) by the element codec; the
        // document's layout sees the array's shape, `[]`.
        let elements = match raw.get_mut("elements") {
            Some(Value::Array(items)) => Some(
                std::mem::take(items)
                    .into_iter()
                    .map(|item| match item {
                        Value::Object(map) => Element::from_encoded(&map),
                        _ => Err(invalid("an element must be an object")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            _ => None,
        };
        // appState and files are laid out by their own layouts, every key an
        // unknown one; the document's layout sees their shape, `{}`.
        let mut object = |key: &str| match raw.get_mut(key) {
            Some(Value::Object(map)) => {
                let (layout, public) = Layout::read(map, &Map::new(), &[]);
                *map = Map::new();
                (Some(public), layout)
            }
            _ => (None, Layout::default()),
        };
        let (app_state, app_state_layout) = object("appState");
        let (files, files_layout) = object("files");

        let mut doc = Document {
            version,
            source,
            elements,
            app_state,
            files,
            extra: Map::new(),
            layout: Layout::default(),
            app_state_layout,
            files_layout,
        };
        let typed = doc.typed(&["elements", "appState", "files"]);
        let (layout, extra) = Layout::read(&raw, &typed, CANONICAL);
        doc.layout = layout;
        doc.extra = extra;
        Ok(doc)
    }

    /// The JSON object serde writes for this document. A lone surrogate
    /// read from a file is U+FFFD here; only [`Document::to_json`] writes it
    /// back as its escape. Keys are in JS property order.
    pub fn to_map(&self) -> Map<String, Value> {
        json::ordered_like_js(json::decode_map(&self.to_encoded()))
    }

    /// The object to write, in the sentinel form of [`crate::json`], before
    /// JS key ordering (which [`json::write_parsed`] applies).
    fn to_encoded(&self) -> Map<String, Value> {
        self.layout.write(&self.typed(&[]), &self.extra, CANONICAL)
    }

    /// The keys and values the typed model writes, in the sentinel form;
    /// the containers named in `shaped` reduced to their shape (`[]` or
    /// `{}`), as [`Document::from_encoded`] gives them to the layout.
    fn typed(&self, shaped: &[&str]) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("type".into(), Value::from(EXPORT_DATA_TYPE_EXCALIDRAW));
        if let Some(version) = self.version {
            map.insert("version".into(), Value::from(version));
        }
        if let Some(source) = &self.source {
            map.insert("source".into(), Value::from(json::escape_str(source)));
        }
        if let Some(elements) = &self.elements {
            let items = if shaped.contains(&"elements") {
                Vec::new()
            } else {
                elements
                    .iter()
                    .map(|e| Value::Object(e.to_encoded()))
                    .collect()
            };
            map.insert("elements".into(), Value::Array(items));
        }
        for (key, object, layout) in [
            ("appState", &self.app_state, &self.app_state_layout),
            ("files", &self.files, &self.files_layout),
        ] {
            if let Some(object) = object {
                let value = if shaped.contains(&key) {
                    Map::new()
                } else {
                    layout.write(&Map::new(), object, &[])
                };
                map.insert(key.into(), Value::Object(value));
            }
        }
        map
    }
}

impl Serialize for Document {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Document, D::Error> {
        let raw = Map::<String, Value>::deserialize(d)?;
        Document::from_map(raw).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Loading and saving

/// A scene as upstream's file loading gives it: `loadFromBlob(file, null,
/// null)` for a `.excalidraw` file, with no local state
/// (`packages/excalidraw/data/blob.ts:137-216`).
#[derive(Debug, Clone)]
pub struct LoadedScene {
    /// `restoreElements(data.elements, null, {repairBindings: true,
    /// deleteInvisibleElements: true})`.
    pub elements: Vec<Element>,
    /// `restoreAppState({theme: undefined, fileHandle: null,
    /// ...cleanAppStateForExport(data.appState || {})}, null)`: the full
    /// state, defaults for every key the file does not export.
    pub app_state: AppState,
    /// `data.files || {}`: the file's `files` when truthy, whatever its
    /// type, otherwise `{}`. Upstream does not check it, and saving indexes
    /// it as JS does ([`filter_out_deleted_files`]), so an array or a string
    /// gives the entries a `fileId` such as `"0"` names.
    pub files: Value,
    /// Top-level keys `serializeAsJSON` does not write, in file order.
    /// Upstream drops them; the port keeps them (see
    /// [`LoadedScene::to_document`]).
    pub extra: Map<String, Value>,
    /// `files` as read, in the sentinel form of [`crate::json`], so a lone
    /// surrogate in it is written back as read while `files` is unchanged.
    files_raw: Value,
}

impl PartialEq for LoadedScene {
    fn eq(&self, other: &LoadedScene) -> bool {
        self.elements == other.elements
            && self.app_state == other.app_state
            && self.files == other.files
            && self.extra == other.extra
    }
}

/// Why a file is not a scene. Upstream's loader reports each of these as
/// `Error: invalid file` (`blob.ts:180-193`), which is what [`Display`]
/// gives; [`std::error::Error::source`] has the cause.
///
/// [`Display`]: fmt::Display
#[derive(Debug)]
pub enum LoadSceneError {
    /// `JSON.parse` failed.
    Json(Error),
    /// `isValidExcalidrawData` (`json.ts:115-126`) does not hold.
    NotAScene,
    /// `restoreElements` threw as a whole (an element that throws is
    /// dropped instead).
    Restore(RestoreElementsError),
    /// `restoreAppState` threw.
    AppState(TypeError),
    /// A restored element has no typed form: a port defect, since restore
    /// only gives elements of known types with their fields.
    Untyped(Error),
}

impl fmt::Display for LoadSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Error: invalid file")
    }
}

impl std::error::Error for LoadSceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadSceneError::Json(e) | LoadSceneError::Untyped(e) => Some(e),
            LoadSceneError::NotAScene => None,
            LoadSceneError::Restore(e) => Some(e),
            LoadSceneError::AppState(e) => Some(e),
        }
    }
}

/// Load a `.excalidraw` file as upstream's `loadFromBlob(file, null, null)`
/// does (`blob.ts:137-216`): `JSON.parse`, `isValidExcalidrawData`, then
/// restore. Unlike [`Document::from_json`] this reads any scene upstream
/// reads, legacy fields and elements of unknown types included (restore
/// migrates or drops them). `env` supplies ids, timestamps and
/// `versionNonce`s as in restore; `app_env` the AppState defaults.
pub fn load_scene_json(
    text: &str,
    env: &mut dyn RestoreEnv,
    app_env: &AppStateEnv,
) -> Result<LoadedScene, LoadSceneError> {
    match json::parse(text).map_err(LoadSceneError::Json)? {
        Value::Object(raw) => load_encoded(raw, env, app_env),
        _ => Err(LoadSceneError::NotAScene),
    }
}

impl LoadedScene {
    /// [`load_scene_json`] for a document already read, e.g. by
    /// [`Document::from_json`].
    pub fn from_document(
        doc: &Document,
        env: &mut dyn RestoreEnv,
        app_env: &AppStateEnv,
    ) -> Result<LoadedScene, LoadSceneError> {
        load_encoded(doc.to_encoded(), env, app_env)
    }

    /// The scene as `serializeAsJSON(elements, appState, files, "local")`
    /// writes it (`json.ts:52-75`): `source` as given (upstream's
    /// `getExportSource()`), the exported `appState`
    /// (`cleanAppStateForExport`) and the files live elements use
    /// ([`filter_out_deleted_files`]). The unknown top-level keys of
    /// [`LoadedScene::extra`] are written after them; upstream drops them.
    pub fn to_document(&self, source: &str) -> Document {
        let files = if json::decode(&self.files_raw) == self.files {
            Cow::Borrowed(&self.files_raw)
        } else {
            Cow::Owned(json::escape(&self.files))
        };
        let kept = filter_files_encoded(&self.elements, &files);
        let mut doc = Document::new(
            source,
            self.elements.clone(),
            clean_app_state_for_export(self.app_state.as_map()),
            None,
        );
        // Laid out as Document::from_encoded lays out a `files` it reads.
        let (files_layout, files) = Layout::read(&kept, &Map::new(), &[]);
        doc.files = Some(files);
        doc.files_layout = files_layout;
        doc.extra = self.extra.clone();
        doc
    }
}

/// `filterOutDeletedFiles` (`json.ts:31-50`): for each element not deleted
/// with a truthy `fileId`, in element order, `next[fileId] =
/// files[fileId]` when that is truthy. `files` is indexed as JS indexes
/// it: an object by its own keys, an array or a string by canonical index
/// keys (`"0"`, not `"00"`; a string gives one UTF-16 code unit) and
/// `length`; anything else has no key `JSON.stringify` would write (its
/// prototype's are functions). `__proto__` is never written: assigning it
/// sets the prototype of `next`.
pub fn filter_out_deleted_files(elements: &[Element], files: &Value) -> Map<String, Value> {
    json::decode_map(&filter_files_encoded(elements, &json::escape(files)))
}

/// [`filter_out_deleted_files`] in the sentinel form.
fn filter_files_encoded(elements: &[Element], files: &Value) -> Map<String, Value> {
    let mut next = Map::new();
    for element in elements {
        let map = element.to_encoded();
        if truthy(map.get("isDeleted")) || !truthy(map.get("fileId")) {
            continue;
        }
        // files[element.fileId]: the id as a property key.
        let Ok(id) = js::to_string(map.get("fileId")) else {
            continue;
        };
        if id == "__proto__" {
            continue;
        }
        if let Some(file) = property(files, &id).filter(|f| truthy(Some(f))) {
            next.insert(id, file);
        }
    }
    next
}

/// `value[key]` for a parsed JSON value in the sentinel form, leaving out
/// inherited properties (functions, and `__proto__`, which the caller
/// skips).
fn property(value: &Value, key: &str) -> Option<Value> {
    let index = || {
        json::is_array_index(key)
            .then(|| key.parse::<usize>().ok())
            .flatten()
    };
    match value {
        Value::Object(map) => map.get(key).cloned(),
        Value::Array(items) if key == "length" => Some(Value::from(items.len())),
        Value::Array(items) => index().and_then(|i| items.get(i)).cloned(),
        Value::String(s) => {
            let units = json::to_utf16(s);
            if key == "length" {
                return Some(Value::from(units.len()));
            }
            let unit = index().and_then(|i| units.get(i))?;
            Some(Value::String(json::from_utf16(std::slice::from_ref(unit))))
        }
        _ => None,
    }
}

/// [`load_scene_json`] on a parsed object in the sentinel form.
fn load_encoded(
    mut raw: Map<String, Value>,
    env: &mut dyn RestoreEnv,
    app_env: &AppStateEnv,
) -> Result<LoadedScene, LoadSceneError> {
    validate(&raw).map_err(|_| LoadSceneError::NotAScene)?;
    // restoreElements(undefined) restores `[]`; a truthy `elements` is an
    // array here. Taken out of the document (left `null` in its place, a
    // key the rest does not read) so restore owns the elements.
    let items: Vec<Value> = match raw.get_mut("elements").map(Value::take) {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    };
    let opts = RestoreElementsOptions {
        repair_bindings: true,
        delete_invisible_elements: true,
        refresh_dimensions: false,
    };
    let elements = restore_elements_sentinel_owned(items, opts, env)
        .map_err(LoadSceneError::Restore)?
        .iter()
        .map(Element::from_restored_encoded)
        .collect::<Result<Vec<_>, _>>()
        .map_err(LoadSceneError::Untyped)?;

    // cleanAppStateForExport(data.appState || {}) keeps the exported keys a
    // value has; only an object has any.
    let mut imported = Map::new();
    imported.insert("fileHandle".to_owned(), Value::Null);
    if let Some(Value::Object(app_state)) = raw.get("appState") {
        imported.extend(clean_app_state_for_export(&json::decode_map(app_state)));
    }
    let app_state =
        restore_app_state(Some(&imported), None, app_env).map_err(LoadSceneError::AppState)?;

    // data.files || {}
    let files_raw = match raw.get("files") {
        Some(files) if truthy(Some(files)) => files.clone(),
        _ => Value::Object(Map::new()),
    };
    let extra = raw
        .iter()
        .filter(|(key, _)| !KEYS.contains(&key.as_str()))
        .map(|(key, value)| (json::decode_str(key).into_owned(), json::decode(value)))
        .collect();
    Ok(LoadedScene {
        elements,
        app_state,
        files: json::decode(&files_raw),
        extra,
        files_raw,
    })
}

// ---------------------------------------------------------------------------
// Files

/// `BinaryFileData["mimeType"]` (`packages/excalidraw/types.ts:118-122`): an
/// image type of `IMAGE_MIME_TYPES` or `MIME_TYPES.binary`
/// (`packages/common/src/constants.ts:296-330`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum FileMimeType {
    #[serde(rename = "image/svg+xml")]
    Svg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
    #[serde(rename = "image/bmp")]
    Bmp,
    #[serde(rename = "image/x-icon")]
    Icon,
    #[serde(rename = "image/avif")]
    Avif,
    #[serde(rename = "image/jfif")]
    Jfif,
    // A future or unknown file type. (A plain comment: a doc comment would
    // make the schema a oneOf of consts instead of one enum.)
    #[serde(rename = "application/octet-stream")]
    Binary,
}

/// A file of the `files` map (`BinaryFileData`,
/// `packages/excalidraw/types.ts:118-142`). [`Document::files`] keeps the
/// entries as read; this is their typed view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(
    description = "A file of the `files` map (`BinaryFileData`, `packages/excalidraw/types.ts:118-142`): an image an image element refers to by `fileId`."
)]
pub struct BinaryFileData {
    pub mime_type: FileMimeType,
    /// The file id: the SHA-1 hex of the bytes, or a 40-character nanoid
    /// (`data/blob.ts:259-273`); also the key of the entry.
    pub id: FileId,
    /// `data:<mime>;base64,...` (`DataURL`, `getDataURL_sync`,
    /// `data/blob.ts:288-296`).
    #[serde(rename = "dataURL")]
    #[schemars(pattern(r"^data:"))]
    pub data_url: String,
    /// Epoch milliseconds.
    pub created: f64,
    /// Epoch milliseconds of the last load from storage; storage uses it to
    /// delete unused files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub last_retrieved: Option<f64>,
    /// Version of the file, to tell whether `dataURL` changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub version: Option<f64>,
}

impl BinaryFileData {
    /// The typed view of a `files` entry; `Err` when a key has another
    /// type than `BinaryFileData` gives it, or `dataURL` is no data URL.
    pub fn from_value(value: &Value) -> Result<BinaryFileData, Error> {
        let file = BinaryFileData::deserialize(value)?;
        if !file.data_url.starts_with("data:") {
            return Err(serde::de::Error::custom("dataURL must be a data: URL"));
        }
        Ok(file)
    }
}

// ---------------------------------------------------------------------------
// JSON Schema

impl JsonSchema for Document {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Document".into()
    }

    /// `ExportedDataState` (`packages/excalidraw/data/types.ts:14-21`), as
    /// `serializeAsJSON` writes it (`data/json.ts:52-75`): `files` is
    /// absent for a database save. Unknown keys are allowed and kept.
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let element = generator.subschema_for::<Element>();
        let app_state = generator.subschema_for::<ExportedAppState>();
        let file = generator.subschema_for::<BinaryFileData>();
        schemars::json_schema!({
            "type": "object",
            "properties": {
                "type": {
                    "description": "`EXPORT_DATA_TYPES.excalidraw` (packages/common/src/constants.ts:345).",
                    "const": EXPORT_DATA_TYPE_EXCALIDRAW,
                },
                "version": {
                    "description": "`VERSIONS.excalidraw` (packages/common/src/constants.ts:416-419): 2 when written.",
                    "type": "number",
                },
                "source": {
                    "description": "The writer's origin (`EXCALIDRAW_EXPORT_SOURCE`, packages/common/src/constants.ts:351-352).",
                    "type": "string",
                },
                "elements": {
                    "description": "The scene's elements in z-order, deleted ones (`isDeleted: true`) included.",
                    "type": "array",
                    "items": element,
                },
                "appState": app_state,
                "files": {
                    "description": "`BinaryFiles` (packages/excalidraw/types.ts:146): file id to file, for the image elements that reference one.",
                    "type": "object",
                    "additionalProperties": file,
                },
            },
            "required": ["type", "version", "source", "elements", "appState"],
        })
    }
}
