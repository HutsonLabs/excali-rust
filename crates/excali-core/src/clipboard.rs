//! The clipboard JSON format: what upstream writes on copy and what it
//! reads back on paste (`packages/excalidraw/clipboard.ts` at the pinned
//! commit; research page `site/content/research/data-model.md`, section 5;
//! `site/content/architecture/file-format.md`, "Clipboard").
//!
//! **Copy.** [`serialize_as_clipboard_json`] is `serializeAsClipboardJSON`
//! (`clipboard.ts:143-193`): `JSON.stringify({type: "excalidraw/clipboard",
//! elements, files})`, compact, with
//!
//! - `files` holding the entries of the given files map for the copied
//!   image elements that have a `fileId` (`isInitializedImageElement`), in
//!   element order, or the key left out when no files map is given;
//! - orphaned children detached: an element whose `frameId` names one of the
//!   copied elements that is not a frame or magic frame is written with
//!   `frameId: null`, through `mutateElement` (`version` + 1, a fresh
//!   `versionNonce`, `updated`), on a copy. `getContainingFrame` looks the
//!   frame up among the copied elements only (`clipboard.ts:150`,
//!   `packages/element/src/frame.ts:436-445`), so a child whose frame is not
//!   among them is not found and keeps its `frameId`; restore clears that
//!   one on paste (`restore.ts:876-887`).
//!
//! `copyToClipboard` (`clipboard.ts:195-210`) puts the string on the system
//! clipboard under two MIME types, [`clipboard_items`].
//!
//! **Paste.** [`parse_clipboard`] is `parseClipboard` (`clipboard.ts:
//! 523-555`) for a paste whose text comes from `text/plain`: the text is
//! trimmed (`parseClipboardEventTextData`, `clipboard.ts:357-360`), then
//! [`parse_clipboard_text`] parses it with `JSON.parse`. Any of the three
//! `type`s `"excalidraw"`, `"excalidraw/clipboard"` and
//! `"excalidraw-api/clipboard"` with an `elements` array gives
//! [`ClipboardData::Elements`] (`clipboardContainsElements`,
//! `clipboard.ts:74-88`); anything else, invalid JSON included, is
//! [`ClipboardData::Text`]. The elements are passed on as parsed, untyped:
//! the paste handler restores them, as upstream's does.
//!
//! A `text/html` item is turned into text or mixed content by the DOM
//! (`maybeParseHTMLDataItem`, `clipboard.ts:233-251`); that is the host's
//! job. When the HTML is text only, upstream reads the `text/plain` item
//! untrimmed, else the joined HTML text; [`parse_clipboard_text`] takes that
//! value as it is.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::constants::{
    EXPORT_DATA_TYPE_EXCALIDRAW, EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD,
    EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD_WITH_API,
};
use crate::element::{Element, ElementKind};
use crate::fractional_index::ChangeStamp;
use crate::js;
use crate::json;

/// `MIME_TYPES.text`, `packages/common/src/constants.ts:309`.
pub const MIME_TYPE_TEXT: &str = "text/plain";
/// `MIME_TYPES.excalidrawClipboard`, `constants.ts:314`: the clipboard JSON.
pub const MIME_TYPE_EXCALIDRAW_CLIPBOARD: &str = "application/vnd.excalidraw.clipboard+json";

/// `serializeAsClipboardJSON({elements, files})` (`clipboard.ts:143-193`):
/// the clipboard JSON for `elements`, as `JSON.stringify` writes it (see
/// the module docs). `files` is the scene's `BinaryFiles` map, `None` for
/// upstream's `null`. `stamp` supplies what `mutateElement` draws for each
/// detached child, in element order. The elements are not changed.
///
/// Every element is written as given, deleted ones included; upstream's
/// callers pass the non-deleted selection.
pub fn serialize_as_clipboard_json(
    elements: &[Element],
    files: Option<&Map<String, Value>>,
    stamp: &mut impl ChangeStamp,
) -> String {
    // arrayToMap: the last element of an id wins.
    let by_id: HashMap<&str, &Element> = elements.iter().map(|e| (e.base.id.as_str(), e)).collect();

    let items = elements
        .iter()
        .map(|element| {
            let orphaned = element
                .base
                .frame_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .and_then(|id| by_id.get(id))
                .is_some_and(|frame| !frame.element_type().is_frame_like());
            if !orphaned {
                return Value::Object(element.to_encoded());
            }
            // deepCopyElement, then mutateElement(copy, map, {frameId: null}).
            let mut copy = element.clone();
            copy.base.frame_id = None;
            copy.base.version += 1.0;
            copy.base.version_nonce = stamp.version_nonce();
            copy.base.updated = stamp.updated();
            Value::Object(copy.to_encoded())
        })
        .collect();

    let mut contents = Map::new();
    contents.insert(
        "type".into(),
        Value::from(EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD),
    );
    contents.insert("elements".into(), Value::Array(items));
    if let Some(files) = files {
        contents.insert("files".into(), Value::Object(copied_files(elements, files)));
    }
    json::write_parsed_compact(&Value::Object(contents))
}

/// The `files` entries of the copied images (`clipboard.ts:156-164`), in the
/// sentinel form: for each image with a truthy `fileId`, in element order,
/// `acc[fileId] = files[fileId]` when that is truthy. Only own keys of the
/// parsed map are found; an inherited one (`toString`) would be a function,
/// which `JSON.stringify` leaves out, and assigning `__proto__` sets the
/// prototype instead of a key, so neither is written.
fn copied_files(elements: &[Element], files: &Map<String, Value>) -> Map<String, Value> {
    let mut acc = Map::new();
    for element in elements {
        let ElementKind::Image(image) = &element.kind else {
            continue;
        };
        let Some(file_id) = image.file_id.as_ref().map(|f| f.0.as_str()) else {
            continue;
        };
        if file_id.is_empty() || file_id == "__proto__" {
            continue;
        }
        if let Some(file) = files.get(file_id).filter(|v| js::truthy(Some(v))) {
            acc.insert(json::escape_str(file_id).into_owned(), json::escape(file));
        }
    }
    acc
}

/// The system clipboard entries `copyToClipboard` writes the clipboard JSON
/// under (`clipboard.ts:201-209`): the Excalidraw clipboard MIME type and
/// plain text, in that order.
pub fn clipboard_items(json: &str) -> [(&'static str, &str); 2] {
    [
        (MIME_TYPE_EXCALIDRAW_CLIPBOARD, json),
        (MIME_TYPE_TEXT, json),
    ]
}

/// What a paste holds (`ClipboardData`, `clipboard.ts:47-54`), for text
/// input.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardData {
    /// Elements to paste: `{elements, files, text, programmaticAPI}`.
    Elements(PastedElements),
    /// Anything else: `{text}`, the text as read.
    Text(String),
}

/// Pasted Excalidraw data (`clipboard.ts:543-550`).
#[derive(Debug, Clone, PartialEq)]
pub struct PastedElements {
    /// The `elements` array as parsed, items of any shape, for restore. A
    /// lone UTF-16 surrogate in a string reads as U+FFFD (see
    /// [`crate::json`]).
    pub elements: Vec<Value>,
    /// The `files` value as parsed, whatever it is (`null` included);
    /// `None` when the key is absent.
    pub files: Option<Value>,
    /// For a plain paste, `JSON.stringify(elements, null, 2)`: the elements
    /// are pasted as that text. `None` otherwise.
    pub text: Option<String>,
    /// The `type` was `"excalidraw-api/clipboard"`: the data came from the
    /// host API rather than a user copy.
    pub programmatic_api: bool,
}

/// `parseClipboard(dataList, isPlainPaste)` (`clipboard.ts:523-555`) for a
/// paste with no HTML item: `text_plain` is the `text/plain` string, `None`
/// when there is none. It is trimmed as `String.prototype.trim` does and
/// passed to [`parse_clipboard_text`].
pub fn parse_clipboard(text_plain: Option<&str>, is_plain_paste: bool) -> ClipboardData {
    let value = text_plain
        .unwrap_or("")
        .trim_matches(js::is_whitespace_char);
    parse_clipboard_text(value, is_plain_paste)
}

/// The JSON step of `parseClipboard` (`clipboard.ts:538-554`) on the paste's
/// text `value`, taken as it is: [`ClipboardData::Elements`] when it parses
/// to an object whose `type` is one of `"excalidraw"`,
/// `"excalidraw/clipboard"` and `"excalidraw-api/clipboard"` and whose
/// `elements` is an array, else [`ClipboardData::Text`] of `value`.
pub fn parse_clipboard_text(value: &str, is_plain_paste: bool) -> ClipboardData {
    let text = || ClipboardData::Text(value.to_owned());
    // JSON.parse; a parse error, and `null.type` on `null`, are caught.
    let Ok(Value::Object(data)) = json::parse(value) else {
        return text();
    };
    let accepted = data.get("type").and_then(Value::as_str).is_some_and(|t| {
        [
            EXPORT_DATA_TYPE_EXCALIDRAW,
            EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD,
            EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD_WITH_API,
        ]
        .contains(&t)
    });
    if !accepted {
        return text();
    }
    let programmatic_api = data.get("type").and_then(Value::as_str)
        == Some(EXPORT_DATA_TYPE_EXCALIDRAW_CLIPBOARD_WITH_API);
    let Some(Value::Array(elements)) = data.get("elements") else {
        return text();
    };
    let text = is_plain_paste.then(|| json::write_parsed(&Value::Array(elements.clone())));
    ClipboardData::Elements(PastedElements {
        elements: elements.iter().map(json::decode).collect(),
        files: data.get("files").map(json::decode),
        text,
        programmatic_api,
    })
}
