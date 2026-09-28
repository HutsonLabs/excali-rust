//! Round-trip placeholder for the `.excalidraw` format.
//!
//! Upstream serialises a scene with `JSON.stringify(data, null, 2)`
//! (`packages/excalidraw/data/json.ts`, `serializeAsJSON`). Until the element
//! model lands (ex-101 onwards), this pins the property the whole port rests
//! on: a file read and written back without edits is byte-identical, key
//! order included. The typed `Document` round trip replaces the untyped value
//! here once it exists.

const EMPTY_SCENE: &str = include_str!("fixtures/empty-scene.excalidraw");

#[test]
fn empty_scene_round_trips_byte_for_byte() {
    let written = excali_core::json::round_trip(EMPTY_SCENE).expect("fixture parses");
    assert_eq!(written, EMPTY_SCENE.trim_end());
}

#[test]
fn key_order_is_preserved_not_sorted() {
    let input = "{\n  \"type\": \"excalidraw\",\n  \"elements\": [],\n  \"appState\": {}\n}";
    let written = excali_core::json::round_trip(input).expect("parses");
    assert_eq!(written, input);
}

#[test]
fn invalid_json_is_an_error_not_a_panic() {
    assert!(excali_core::json::round_trip("{\"type\": ").is_err());
}
