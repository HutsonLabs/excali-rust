//! Library files: listing the items, and merging libraries as the editor
//! imports one library into another.
//!
//! Importing a library runs `library.updateLibrary({libraryItems, merge:
//! true})`, which keeps `mergeLibraryItems(localItems, imported)`
//! (`packages/excalidraw/data/library.ts:145-157`): the imported items not
//! already in the library (same element ids and `versionNonce`s in order),
//! then the library's own. `lib merge A B C` starts from A and imports B,
//! then C; the result is written by `serializeLibraryAsJSON`
//! (`data/json.ts:137-145`).

use serde_json::{json, Map, Value};

use excali_core::library::{merge_library_items, serialize_library_as_json, LibraryItem};

/// A number as `JSON.stringify` writes it: integers without a fraction,
/// NaN and the infinities as `null`.
fn number(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        Value::from(x as i64)
    } else {
        serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
    }
}

/// One item as `lib list --json` reports it.
pub fn item_summary(item: &LibraryItem) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), Value::String(item.id.clone()));
    map.insert(
        "name".into(),
        item.name.clone().map_or(Value::Null, Value::String),
    );
    map.insert("status".into(), Value::String(item.status.as_str().into()));
    map.insert("created".into(), number(item.created));
    map.insert("elements".into(), json!(item.elements.len()));
    Value::Object(map)
}

/// One item as `lib list` prints it: id, status, element count, name,
/// separated by tabs.
pub fn item_line(item: &LibraryItem) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        item.id,
        item.status.as_str(),
        item.elements.len(),
        item.name.as_deref().unwrap_or_default()
    )
}

/// `libraries` merged in turn into the first.
pub fn merge(libraries: &[Vec<LibraryItem>]) -> Vec<LibraryItem> {
    let mut merged: Vec<LibraryItem> = Vec::new();
    for (i, items) in libraries.iter().enumerate() {
        merged = if i == 0 {
            items.clone()
        } else {
            merge_library_items(&merged, items)
        };
    }
    merged
}

/// The merged library's file.
pub fn write(items: &[LibraryItem], source: &str) -> String {
    serialize_library_as_json(items, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_javascript_writes_them() {
        assert_eq!(number(1.0), json!(1));
        assert_eq!(number(1.5), json!(1.5));
        assert_eq!(number(f64::NAN), Value::Null);
        assert_eq!(number(1.7e300), json!(1.7e300));
    }

    #[test]
    fn merging_nothing_is_empty() {
        assert!(merge(&[]).is_empty());
    }
}
