//! Round trip of the `.excalidraw` format, untyped and typed.
//!
//! Upstream serialises a scene with `JSON.stringify(data, null, 2)`
//! (`packages/excalidraw/data/json.ts`, `serializeAsJSON`). This pins the
//! property the whole port rests on: a file read and written back without
//! edits is byte-identical, key order included, both through the untyped
//! value (`json::round_trip`) and through the typed `Document` (more cases
//! in `document.rs`).
//!
//! The fixture was checked against upstream at the pinned commit
//! 438d89861f53d8a90ad566113ecac1b83761098f (`.tools/upstream`, created by
//! ex-002):
//!
//! - header keys `type`, `version`, `source`, `elements`, `appState`, `files`
//!   in that order: the object literal in `serializeAsJSON`,
//!   `packages/excalidraw/data/json.ts:58-72`;
//! - which `appState` keys are written: exactly the five marked
//!   `export: true` in `APP_STATE_STORAGE_CONF`,
//!   `packages/excalidraw/appState.ts:221-286` (gridSize 221, gridStep 222,
//!   gridModeEnabled 223, viewBackgroundColor 273, lockedMultiSelections
//!   286). The table decides membership only;
//! - the order of those keys: `_clearAppStateForStorage` iterates
//!   `Object.keys(appState)` (`packages/excalidraw/appState.ts:305`), so the
//!   written order is the insertion order of the appState object passed in.
//!   For the default state that is `getDefaultAppState`
//!   (`packages/excalidraw/appState.ts:74-76` gridSize, gridStep,
//!   gridModeEnabled; 117 viewBackgroundColor; 134 lockedMultiSelections).
//!   Fixtures built from any other appState must follow that object's key
//!   order, not the storage-conf table;
//! - `gridSize` 20 and `gridStep` 5 are `DEFAULT_GRID_SIZE` and
//!   `DEFAULT_GRID_STEP`, `packages/common/src/constants.ts:293-294`;
//! - `#ffffff` is `COLOR_PALETTE.white`, `packages/common/src/colors.ts:196`.

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

#[test]
fn empty_scene_round_trips_through_the_typed_document() {
    let doc = excali_core::document::Document::from_json(EMPTY_SCENE).expect("fixture parses");
    assert_eq!(doc.to_json(), EMPTY_SCENE.trim_end());
}

#[test]
fn key_order_is_preserved_through_the_typed_document() {
    let input = "{\n  \"elements\": [],\n  \"appState\": {},\n  \"type\": \"excalidraw\"\n}";
    let doc = excali_core::document::Document::from_json(input).expect("parses");
    assert_eq!(doc.to_json(), input);
}
