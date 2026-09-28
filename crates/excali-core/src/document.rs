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
//! `appState` and `files` are kept as JSON objects here.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::constants::{EXPORT_DATA_TYPE_EXCALIDRAW, VERSION_EXCALIDRAW};
use crate::element::Element;
use crate::json::{self, Error};
use crate::layout::{Canonical, Layout};

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
        let object = |key: &str| match raw.get(key) {
            Some(Value::Object(map)) => Some(json::decode_map(map)),
            _ => None,
        };
        let app_state = object("appState");
        let files = object("files");

        // An object without lone surrogates is written back exactly as its
        // typed form, so the layout only needs its shape.
        let mut shaped = vec!["elements"];
        for (key, typed) in [("appState", &app_state), ("files", &files)] {
            if let (Some(_), Some(value)) = (typed, raw.get_mut(key)) {
                if !json::has_lone_surrogate(value) {
                    *value = Value::Object(Map::new());
                    shaped.push(key);
                }
            }
        }

        let mut doc = Document {
            version,
            source,
            elements,
            app_state,
            files,
            extra: Map::new(),
            layout: Layout::default(),
        };
        let typed = doc.typed(&shaped);
        let (layout, extra) = Layout::read(&raw, &typed, CANONICAL);
        doc.layout = layout;
        doc.extra = extra;
        Ok(doc)
    }

    /// The JSON object serde writes for this document. A lone surrogate
    /// read from a file is U+FFFD here; only [`Document::to_json`] writes it
    /// back as its escape.
    pub fn to_map(&self) -> Map<String, Value> {
        json::decode_map(&self.to_encoded())
    }

    /// The object to write, in the sentinel form of [`crate::json`].
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
        for (key, object) in [("appState", &self.app_state), ("files", &self.files)] {
            if let Some(object) = object {
                let value = if shaped.contains(&key) {
                    Map::new()
                } else {
                    json::escape_map(object)
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
