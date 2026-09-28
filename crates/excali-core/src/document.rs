//! The `.excalidraw` file: [`Document`].
//!
//! Upstream writes a scene with `serializeAsJSON`
//! (`packages/excalidraw/data/json.ts:52-75`): the object
//! `{type, version, source, elements, appState, files}` passed to
//! `JSON.stringify(data, null, 2)`, where `files` is `undefined` (so
//! omitted) for a database save. It reads one with `JSON.parse` into
//! `ImportedDataState` (`data/types.ts:35-50`), where every key is optional
//! and `elements` and `appState` may be `null`, and accepts it when
//! `isValidExcalidrawData` (`json.ts:115-126`) holds: `type` is
//! `"excalidraw"`, `elements` is falsy or an array, `appState` falsy or an
//! object.
//!
//! [`Document::from_json`] then [`Document::to_json`] gives what
//! `JSON.stringify(JSON.parse(text), null, 2)` gives: keys unknown to the
//! model (top level, in elements, in `appState` and `files`) are kept where
//! they were, known keys keep their order, and a value the typed model
//! would normalise is written back as read until it changes (see
//! [`Element`]). A [`Document`] built in Rust is written in
//! `serializeAsJSON`'s key order.
//!
//! This is the codec for files in upstream's written form. Restoring
//! legacy or partial elements (defaults, renamed fields, unknown types) is
//! the restore module's job; the codec rejects elements it cannot model.
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

/// Keys whose values are containers read by their own codec; the layout
/// only tracks whether they are present, `null` or a container.
const CONTAINERS: [&str; 3] = ["elements", "appState", "files"];

fn invalid(message: &str) -> Error {
    serde::de::Error::custom(format!("not a valid Excalidraw scene: {message}"))
}

/// A container value reduced to its shape, for [`Layout::read`].
fn shape(value: &Value) -> Value {
    match value {
        Value::Array(_) => Value::Array(Vec::new()),
        Value::Object(_) => Value::Object(Map::new()),
        other => other.clone(),
    }
}

/// An optional JSON object: absent or `null` is `None`.
fn optional_object(value: Option<Value>, key: &str) -> Result<Option<Map<String, Value>>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(_) => Err(invalid(&format!("{key} must be an object"))),
    }
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
    /// `isValidExcalidrawData` does.
    pub fn from_json(text: &str) -> Result<Document, Error> {
        match json::parse(text)? {
            Value::Object(map) => Document::from_map(map),
            _ => Err(invalid("not an object")),
        }
    }

    /// Write the file as `JSON.stringify(data, null, 2)` would: two-space
    /// indent, no trailing newline.
    pub fn to_json(&self) -> String {
        json::write_parsed(&Value::Object(self.to_map()))
    }

    /// Read a document from a parsed JSON object.
    pub fn from_map(mut raw: Map<String, Value>) -> Result<Document, Error> {
        if raw.get("type").and_then(Value::as_str) != Some(EXPORT_DATA_TYPE_EXCALIDRAW) {
            return Err(invalid("type must be \"excalidraw\""));
        }
        let version = raw
            .get("version")
            .map(Option::<f64>::deserialize)
            .transpose()?
            .flatten();
        let source = raw
            .get("source")
            .map(Option::<String>::deserialize)
            .transpose()?
            .flatten();

        // The layout sees containers by shape only; their contents are
        // read (and laid out) by their own codecs.
        let shaped: Map<String, Value> = raw
            .iter()
            .map(|(k, v)| {
                let v = if CONTAINERS.contains(&k.as_str()) {
                    shape(v)
                } else {
                    v.clone()
                };
                (k.clone(), v)
            })
            .collect();

        let elements = match raw.get_mut("elements").map(Value::take) {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) => Some(
                items
                    .into_iter()
                    .map(|item| match item {
                        Value::Object(map) => Element::from_map(map),
                        _ => Err(invalid("an element must be an object")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Some(_) => return Err(invalid("elements must be an array")),
        };
        let app_state = optional_object(raw.get_mut("appState").map(Value::take), "appState")?;
        let files = optional_object(raw.get_mut("files").map(Value::take), "files")?;

        let mut doc = Document {
            version,
            source,
            elements,
            app_state,
            files,
            extra: Map::new(),
            layout: Layout::default(),
        };
        let typed = doc.typed(true);
        let (layout, extra) = Layout::read(&shaped, &typed, CANONICAL);
        doc.layout = layout;
        doc.extra = extra;
        Ok(doc)
    }

    /// The JSON object serde writes for this document.
    pub fn to_map(&self) -> Map<String, Value> {
        self.layout
            .write(&self.typed(false), &self.extra, CANONICAL)
    }

    /// The keys and values the typed model writes; with `shapes`, the
    /// containers reduced to their shape as [`Document::from_map`] gives
    /// them to the layout.
    fn typed(&self, shapes: bool) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("type".into(), Value::from(EXPORT_DATA_TYPE_EXCALIDRAW));
        if let Some(version) = self.version {
            map.insert("version".into(), Value::from(version));
        }
        if let Some(source) = &self.source {
            map.insert("source".into(), Value::from(source.as_str()));
        }
        if let Some(elements) = &self.elements {
            let value = if shapes {
                Value::Array(Vec::new())
            } else {
                Value::Array(elements.iter().map(|e| Value::Object(e.to_map())).collect())
            };
            map.insert("elements".into(), value);
        }
        for (key, object) in [("appState", &self.app_state), ("files", &self.files)] {
            if let Some(object) = object {
                let value = if shapes { Map::new() } else { object.clone() };
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
